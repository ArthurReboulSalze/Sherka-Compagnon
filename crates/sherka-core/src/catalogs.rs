//! Read-only discovery against LM Studio and VoiceStudio. These calls neither
//! download nor load a model and never hold the companion/SQLite lock.
use crate::connections::base_url;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::time::Duration;

const MAX_RESPONSE: usize = 2 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("{0}")]
    Address(String),
    #[error("Le serveur ne répond pas. Vérifie qu'il est démarré et que son adresse est correcte.")]
    Unreachable,
    #[error("Le serveur a mis trop de temps à répondre. Tu peux réessayer.")]
    Timeout,
    #[error(
        "Le serveur demande une authentification. Cette connexion locale ne gère pas encore les jetons d'accès."
    )]
    Authentication,
    #[error("Le serveur a renvoyé HTTP {0} sur {1}.")]
    Http(u16, String),
    #[error("Le catalogue reçu dépasse la limite de 2 Mio.")]
    TooLarge,
    #[error("Le serveur a renvoyé un catalogue incompatible sur {0}.")]
    Format(String),
}

impl DiscoveryError {
    fn missing(&self) -> bool {
        matches!(self, Self::Http(404 | 405, _))
    }
}

fn transport(error: reqwest::Error) -> DiscoveryError {
    if error.is_timeout() {
        DiscoveryError::Timeout
    } else {
        DiscoveryError::Unreachable
    }
}

pub struct CatalogClient {
    client: Client,
    base: String,
}

impl CatalogClient {
    pub fn new(base: &str) -> Result<Self, DiscoveryError> {
        Ok(Self {
            base: base_url(base).map_err(|e| DiscoveryError::Address(e.to_string()))?,
            client: Client::builder()
                .connect_timeout(Duration::from_secs(3))
                .timeout(Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::none())
                .no_proxy()
                .build()
                .map_err(transport)?,
        })
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, DiscoveryError> {
        let mut response = self
            .client
            .get(format!("{}{path}", self.base))
            .send()
            .await
            .map_err(transport)?;
        match response.status() {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                return Err(DiscoveryError::Authentication);
            }
            status if !status.is_success() => {
                return Err(DiscoveryError::Http(status.as_u16(), path.into()));
            }
            _ => {}
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE as u64)
        {
            return Err(DiscoveryError::TooLarge);
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(transport)? {
            if body.len() + chunk.len() > MAX_RESPONSE {
                return Err(DiscoveryError::TooLarge);
            }
            body.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&body).map_err(|_| DiscoveryError::Format(path.into()))
    }

    pub async fn language_models(&self) -> Result<LanguageCatalog, DiscoveryError> {
        let path = "/api/v1/models";
        match self.get::<NativeModels>(path).await {
            Ok(response) => {
                let excluded_models = response.models.iter().filter(|m| m.kind != "llm").count();
                let mut models = Vec::new();
                for model in response.models.into_iter().filter(|m| m.kind == "llm") {
                    if model.loaded_instances.is_empty() {
                        models.push(LanguageModel {
                            vision: model.capabilities.vision,
                            api: None,
                            key: model.key.clone(),
                            id: model.key,
                            label: model.display_name,
                            loaded: Some(false),
                            quantization: model.quantization.and_then(|q| q.name),
                            max_context_length: model.max_context_length,
                            loaded_context_length: None,
                            reasoning: model.capabilities.reasoning,
                        });
                    } else {
                        for instance in model.loaded_instances {
                            models.push(LanguageModel {
                                vision: model.capabilities.vision,
                                api: None,
                                key: model.key.clone(),
                                id: instance.id,
                                label: model.display_name.clone(),
                                loaded: Some(true),
                                quantization: model
                                    .quantization
                                    .as_ref()
                                    .and_then(|q| q.name.clone()),
                                max_context_length: model.max_context_length,
                                loaded_context_length: instance
                                    .config
                                    .and_then(|c| c.context_length),
                                reasoning: model.capabilities.reasoning.clone(),
                            });
                        }
                    }
                }
                Ok(LanguageCatalog {
                    source: path.into(),
                    models,
                    excluded_models,
                })
            }
            Err(error) if error.missing() => {
                let path = "/api/v0/models";
                match self.get::<LegacyModels>(path).await {
                    Ok(response) => {
                        let excluded_models = response
                            .data
                            .iter()
                            .filter(|m| !["llm", "vlm"].contains(&m.kind.as_str()))
                            .count();
                        let models = response
                            .data
                            .into_iter()
                            .filter(|m| ["llm", "vlm"].contains(&m.kind.as_str()))
                            .map(|m| LanguageModel {
                                vision: Some(m.kind == "vlm"),
                                api: None,
                                key: m.id.clone(),
                                label: m.id.clone(),
                                id: m.id,
                                loaded: m.state.map(|s| s == "loaded"),
                                quantization: m.quantization,
                                max_context_length: m.max_context_length,
                                loaded_context_length: m.loaded_context_length,
                                reasoning: None,
                            })
                            .collect();
                        Ok(LanguageCatalog {
                            source: path.into(),
                            models,
                            excluded_models,
                        })
                    }
                    // The generic list does not reliably identify embedding models;
                    // never guess their type from the name.
                    Err(error) if error.missing() => Err(DiscoveryError::Format(
                        "/api/v1/models et /api/v0/models (API native LM Studio requise)".into(),
                    )),
                    Err(error) => Err(error),
                }
            }
            Err(error) => Err(error),
        }
    }

    pub async fn voices(&self) -> Result<VoiceCatalog, DiscoveryError> {
        let (capabilities, engines, models, voices, api_models) = tokio::try_join!(
            self.get::<Capabilities>("/.well-known/voicestudio-speech"),
            self.get::<EngineList>("/engines/tts"),
            self.get::<InstalledModels>("/models"),
            self.get::<Voices>("/v1/audio/voices"),
            self.get::<ApiModels>("/v1/models"),
        )?;
        if capabilities.protocol != "voicestudio.speech.v1" {
            return Err(DiscoveryError::Format(
                "/.well-known/voicestudio-speech (protocole inconnu)".into(),
            ));
        }
        let tts_models: Vec<_> = models
            .models
            .into_iter()
            .filter(|m| m.role == "TTS")
            .collect();
        let tts_engines = engines
            .backends
            .into_iter()
            .map(|engine| {
                let weights: Vec<_> = tts_models
                    .iter()
                    .filter(|m| m.engines.contains(&engine.id))
                    .collect();
                let listed = api_models.data.iter().any(|m| {
                    m.id == engine.id
                        && m.voicestudio.kind == "tts"
                        && !m.voicestudio.alias_for_active_engine
                });
                let readiness = if !engine.available || !listed {
                    "missing_engine"
                } else if weights.is_empty() {
                    "unknown"
                } else if weights
                    .iter()
                    .any(|m| m.installed && !m.incomplete && m.supported)
                {
                    "ready"
                } else {
                    "missing_model"
                };
                TtsEngine {
                    active: engine.id == engines.active,
                    id: engine.id,
                    label: engine.display_name,
                    supports_cloning: engine.supports_cloning.unwrap_or(false),
                    readiness: readiness.into(),
                    models: weights.iter().map(|m| m.repo_id.clone()).collect(),
                }
            })
            .collect();
        let voice_alias_count = voices
            .voices
            .iter()
            .filter(|v| v.kind == "openai_alias")
            .count();
        let dictation = match self.get::<DictationCatalog>("/dictation/models").await {
            Ok(catalog) => catalog,
            Err(error) if error.missing() => DictationCatalog {
                engine_available: false,
                models: vec![],
            },
            Err(error) => return Err(error),
        };
        Ok(VoiceCatalog {
            service_version: capabilities.service_version,
            streaming_transcription: capabilities.features.streaming_transcription,
            batch_transcription: capabilities.features.batch_transcription,
            tts_engines,
            tts_models,
            voices: voices
                .voices
                .into_iter()
                .filter(|v| v.kind != "openai_alias")
                .collect(),
            voice_alias_count,
            dictation_engine_available: dictation.engine_available,
            dictation_models: dictation.models,
            stt_models: api_models
                .data
                .into_iter()
                .filter(|m| m.voicestudio.kind == "stt" && !m.voicestudio.alias_for_active_engine)
                .map(|m| m.id)
                .collect(),
        })
    }
}

#[derive(Debug, Serialize)]
pub struct LanguageCatalog {
    pub source: String,
    pub models: Vec<LanguageModel>,
    pub excluded_models: usize,
}
#[derive(Debug, Clone, Serialize)]
pub struct LanguageModel {
    pub vision: Option<bool>,
    pub api: Option<ApiModelCapabilities>,
    pub key: String,
    pub id: String,
    pub label: String,
    pub loaded: Option<bool>,
    pub quantization: Option<String>,
    pub max_context_length: Option<u64>,
    pub loaded_context_length: Option<u64>,
    pub reasoning: Option<ReasoningCapabilities>,
}
#[derive(Debug, Clone, Serialize, Default)]
pub struct ApiModelCapabilities {
    pub max_output_tokens: Option<u32>,
    pub structured_output: bool,
    pub adaptive_thinking: bool,
    pub parameters: Vec<String>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReasoningCapabilities {
    pub allowed_options: Vec<String>,
    pub default: Option<String>,
}
#[derive(Default, Deserialize)]
struct ModelCapabilities {
    vision: Option<bool>,
    reasoning: Option<ReasoningCapabilities>,
}
#[derive(Deserialize)]
struct NativeModels {
    models: Vec<NativeModel>,
}
#[derive(Deserialize)]
struct NativeModel {
    #[serde(rename = "type")]
    kind: String,
    key: String,
    display_name: String,
    loaded_instances: Vec<Instance>,
    quantization: Option<Quantization>,
    max_context_length: Option<u64>,
    #[serde(default)]
    capabilities: ModelCapabilities,
}
#[derive(Deserialize)]
struct Instance {
    id: String,
    config: Option<InstanceConfig>,
}
#[derive(Deserialize)]
struct InstanceConfig {
    context_length: Option<u64>,
}
#[derive(Deserialize)]
struct Quantization {
    name: Option<String>,
}
#[derive(Deserialize)]
struct LegacyModels {
    data: Vec<LegacyModel>,
}
#[derive(Deserialize)]
struct LegacyModel {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    state: Option<String>,
    quantization: Option<String>,
    max_context_length: Option<u64>,
    loaded_context_length: Option<u64>,
}
#[derive(Debug, Serialize)]
pub struct VoiceCatalog {
    pub service_version: String,
    pub streaming_transcription: bool,
    pub batch_transcription: bool,
    pub tts_engines: Vec<TtsEngine>,
    pub tts_models: Vec<InstalledModel>,
    pub voices: Vec<Voice>,
    pub voice_alias_count: usize,
    pub stt_models: Vec<String>,
    pub dictation_engine_available: bool,
    pub dictation_models: Vec<DictationModel>,
}
#[derive(Debug, Deserialize, Serialize)]
pub struct DictationModel {
    pub id: String,
    pub label: String,
    pub languages: String,
    pub installed: bool,
}
#[derive(Deserialize)]
struct DictationCatalog {
    engine_available: bool,
    models: Vec<DictationModel>,
}
#[derive(Debug, Serialize)]
pub struct TtsEngine {
    pub id: String,
    pub label: String,
    pub active: bool,
    pub supports_cloning: bool,
    pub readiness: String,
    pub models: Vec<String>,
}
#[derive(Debug, Deserialize, Serialize)]
pub struct InstalledModel {
    pub repo_id: String,
    pub label: String,
    pub role: String,
    pub engines: Vec<String>,
    pub installed: bool,
    #[serde(default)]
    pub incomplete: bool,
    pub supported: bool,
}
#[derive(Debug, Deserialize, Serialize)]
pub struct Voice {
    pub voice_id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub language: Option<String>,
}
#[derive(Deserialize)]
struct Capabilities {
    protocol: String,
    service_version: String,
    features: Features,
}
#[derive(Deserialize)]
struct Features {
    streaming_transcription: bool,
    batch_transcription: bool,
}
#[derive(Deserialize)]
struct EngineList {
    active: String,
    backends: Vec<Engine>,
}
#[derive(Deserialize)]
struct Engine {
    id: String,
    display_name: String,
    available: bool,
    supports_cloning: Option<bool>,
}
#[derive(Deserialize)]
struct InstalledModels {
    models: Vec<InstalledModel>,
}
#[derive(Deserialize)]
struct Voices {
    voices: Vec<Voice>,
}
#[derive(Deserialize)]
struct ApiModels {
    data: Vec<ApiModel>,
}
#[derive(Deserialize)]
struct ApiModel {
    id: String,
    voicestudio: ModelKind,
}
#[derive(Deserialize)]
struct ModelKind {
    kind: String,
    alias_for_active_engine: bool,
}
