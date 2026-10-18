//! Actual LM Studio / VoiceStudio contracts. No microphone, UI or database
//! access here. Dropping these futures closes their HTTP/WebSocket transport.
use crate::{Error, Result, connections::Connections, conversation::ChatMessage};
use futures_util::{SinkExt, StreamExt};
use reqwest::{Client, Response, multipart};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::{Message as WsMessage, protocol::WebSocketConfig};

#[derive(Clone)]
pub struct Providers {
    pub(crate) http: Client,
    pub(crate) api_key: Option<String>,
    pub(crate) api_model: Option<crate::catalogs::LanguageModel>,
    pub connections: Connections,
    pub images: Vec<String>,
}

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct ChatOutcome {
    pub length_limited: bool,
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
}

pub(crate) fn failure(text: impl Into<String>) -> Error {
    Error::Validation(text.into())
}
pub(crate) fn network(service: &str, error: &reqwest::Error) -> Error {
    let local = service.starts_with("LM Studio") || service.starts_with("VoiceStudio");
    failure(if !local && error.is_timeout() {
        format!(
            "{service} met trop de temps à répondre. Réessaie plus tard ou réduis la réflexion du modèle."
        )
    } else if !local {
        format!(
            "Connexion à {service} interrompue. Vérifie ta connexion Internet et l’adresse de l’API."
        )
    } else if error.is_timeout() {
        format!(
            "{service} met trop de temps à répondre. Vérifie le chargement du modèle et la mémoire GPU."
        )
    } else {
        format!("Connexion à {service} interrompue. Vérifie que son serveur est démarré.")
    })
}

impl Providers {
    pub fn supports_images(
        &self,
        model: &crate::catalogs::LanguageModel,
    ) -> crate::attachments::Support {
        crate::attachments::Support {
            images: !self.connections.llm_provider.is_agent()
                && model.vision.unwrap_or(self.connections.vision_confirmed),
            known: model.vision.is_some(),
        }
    }
    pub async fn image_support(&self) -> Result<crate::attachments::Support> {
        if self.connections.llm_provider.is_agent() {
            return Ok(crate::attachments::Support {
                images: false,
                known: true,
            });
        }
        let model = if self.connections.llm_provider.is_local() {
            let catalog = crate::catalogs::CatalogClient::new(&self.connections.lm_studio_url)
                .map_err(|_| failure("LM Studio est indisponible."))?
                .language_models()
                .await
                .map_err(|_| failure("LM Studio est indisponible."))?;
            catalog
                .models
                .into_iter()
                .find(|m| {
                    Some(&m.id) == self.connections.llm_model.as_ref()
                        || Some(&m.key) == self.connections.llm_model_key.as_ref()
                })
                .ok_or_else(|| failure("Choisis un modèle de conversation dans Compagnons."))?
        } else {
            self.remote_model().await?
        };
        Ok(self.supports_images(&model))
    }
    pub fn new(connections: Connections) -> Result<Self> {
        let api_key = crate::credentials::load(&connections)?;
        Self::with_api_key(connections, api_key)
    }
    /// Explicit credentials for isolated contract tests and backend use only.
    pub fn with_api_key(connections: Connections, api_key: Option<String>) -> Result<Self> {
        let connections = connections.validated()?;
        if let Some(key) = &api_key {
            crate::credentials::validate_key(key)?;
        }
        let http = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(180))
            .build()
            .map_err(|_| failure("Impossible d'initialiser les connexions."))?;
        Ok(Self {
            images: vec![],
            http,
            connections,
            api_key,
            api_model: None,
        })
    }

    pub(crate) async fn checked(mut response: Response, service: &str) -> Result<Response> {
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        let local = service.starts_with("LM Studio") || service.starts_with("VoiceStudio");
        // Inspect only bounded, documented error codes. Never display arbitrary
        // provider messages, even when they echo prompts or credentials.
        let credit_exhausted = if !local && status.as_u16() == 429 {
            tokio::time::timeout(Duration::from_secs(2), async {
                let mut bytes = Vec::new();
                while let Ok(Some(chunk)) = response.chunk().await {
                    if bytes.len() + chunk.len() > 16_384 {
                        return false;
                    }
                    bytes.extend_from_slice(&chunk);
                }
                let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
                    return false;
                };
                let error = &value["error"];
                (service.starts_with("Z.ai") && (error["code"] == "1113" || error["code"] == 1113))
                    || ["code", "type"].iter().any(|field| {
                        matches!(
                            error[*field].as_str(),
                            Some("insufficient_quota" | "credit_balance_exhausted")
                        )
                    })
            })
            .await
            .unwrap_or(false)
        } else {
            false
        };
        let advice = if credit_exhausted || !local && status.as_u16() == 402 {
            "Le crédit API ou le forfait de ce fournisseur est indisponible. Vérifie la facturation et l’adresse de l’API ; un abonnement de chat ou de code peut être distinct."
        } else {
            match status.as_u16() {
                401 | 403 => "Vérifie la clé API et les droits d’accès du fournisseur.",
                404 => "Vérifie l'adresse et le modèle choisi dans les réglages.",
                409 => "Vérifie que le modèle choisi est installé et actif dans ce logiciel.",
                429 => "Le serveur est occupé. Attends la fin de son calcul avant de réessayer.",
                500..=599 if !local => {
                    "Le fournisseur est temporairement indisponible. Réessaie plus tard."
                }
                500..=599 => {
                    "Vérifie le modèle, les journaux du logiciel et la mémoire GPU disponible."
                }
                _ if !local => {
                    "Vérifie l’adresse de l’API, le modèle et ses options dans Réglages."
                }
                _ => "Vérifie les réglages du modèle et de la voix dans ce logiciel.",
            }
        };
        // Never expose arbitrary response bodies (which may echo prompts/keys).
        Err(failure(format!(
            "{service} a refusé la demande (HTTP {}). {advice}",
            status.as_u16()
        )))
    }

    pub async fn chat(
        &self,
        messages: Vec<ChatMessage>,
        tokens: mpsc::Sender<String>,
    ) -> Result<()> {
        self.chat_budgeted(messages, tokens, 384).await.map(|_| ())
    }

    pub async fn chat_budgeted(
        &self,
        messages: Vec<ChatMessage>,
        tokens: mpsc::Sender<String>,
        max_tokens: u32,
    ) -> Result<ChatOutcome> {
        if !self.connections.llm_provider.is_local() {
            return self.api_chat(messages, tokens, max_tokens).await;
        }
        let model = self
            .connections
            .llm_model
            .as_deref()
            .ok_or_else(|| failure("Choisis un modèle LM Studio dans les réglages."))?;
        let mut body = json!({"model":model,"messages":messages,"stream":true,"stream_options":{"include_usage":true},"temperature":0.65,"max_tokens":max_tokens});
        crate::attachments::inject(&mut body, &self.images, false)?;
        if let Some(effort) = self.connections.reasoning_effort() {
            body["reasoning_effort"] = json!(effort);
        }
        let response = self
            .http
            .post(format!(
                "{}/v1/chat/completions",
                self.connections.lm_studio_url
            ))
            .json(&body)
            .send()
            .await
            .map_err(|e| network("LM Studio", &e))?;
        let response = Self::checked(response, "LM Studio").await?;
        if !response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|s| s.starts_with("text/event-stream"))
        {
            return Err(failure(
                "LM Studio n'a pas renvoyé un flux de conversation SSE.",
            ));
        }
        let mut stream = response.bytes_stream();
        let mut decoder = Sse::default();
        let mut total = 0;
        let mut finished = false;
        let mut outcome = ChatOutcome::default();
        while let Some(bytes) = stream.next().await {
            for event in decoder.feed(&bytes.map_err(|e| network("LM Studio", &e))?)? {
                if event.trim() == "[DONE]" {
                    return if total > 0 {
                        Ok(outcome)
                    } else {
                        Err(failure(if outcome.length_limited {
                            "Le modèle a épuisé sa réserve de réponse pendant la réflexion. Réduis la pensée ou augmente les tokens réservés à la réponse dans Réglages."
                        } else {
                            "LM Studio a renvoyé une réponse vide."
                        }))
                    };
                }
                let value: Value = serde_json::from_str(&event)
                    .map_err(|_| failure("Le flux LM Studio contient un événement illisible."))?;
                if value.get("error").is_some() {
                    return Err(failure(
                        "LM Studio a signalé une erreur pendant la réponse. Vérifie ses journaux.",
                    ));
                }
                let choice = &value["choices"][0];
                if let Some(usage) = value.get("usage") {
                    outcome.prompt_tokens = usage["prompt_tokens"].as_u64();
                    outcome.completion_tokens = usage["completion_tokens"].as_u64();
                    outcome.reasoning_tokens =
                        usage["completion_tokens_details"]["reasoning_tokens"].as_u64();
                }
                if let Some(s) = choice["delta"]["content"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                {
                    total += s.len();
                    if total > 32_000 {
                        return Err(failure(
                            "La réponse dépasse la limite de sécurité de Sherka.",
                        ));
                    }
                    tokens
                        .send(s.to_string())
                        .await
                        .map_err(|_| Error::StaleOperation)?;
                }
                if choice["finish_reason"].is_string() {
                    finished = true;
                    outcome.length_limited = choice["finish_reason"] == "length";
                }
            }
        }
        if finished && total > 0 {
            Ok(outcome)
        } else {
            Err(failure(
                "LM Studio a fermé la connexion avant la fin de sa réponse.",
            ))
        }
    }

    pub async fn catalog(&self) -> Result<crate::catalogs::LanguageCatalog> {
        if !self.connections.llm_provider.is_local() {
            return self.api_catalog().await;
        }
        crate::catalogs::CatalogClient::new(&self.connections.lm_studio_url)
            .map_err(|e| failure(e.to_string()))?
            .language_models()
            .await
            .map_err(|e| failure(e.to_string()))
    }

    pub async fn load_model(&self, key: &str) -> Result<()> {
        if !self.connections.llm_provider.is_local() {
            return Err(failure("Ce fournisseur ne charge pas de modèle local."));
        }
        // Preserve the user's LM Studio loading preset, notably large Gemma
        // context/KV settings. Never allocate the theoretical maximum blindly.
        let response = self
            .http
            .post(format!(
                "{}/api/v1/models/load",
                self.connections.lm_studio_url
            ))
            .json(&json!({"model":key}))
            .send()
            .await
            .map_err(|e| network("LM Studio", &e))?;
        Self::checked(response, "LM Studio (chargement)").await?;
        Ok(())
    }
    pub async fn unload_model(&self, instance: &str) -> Result<()> {
        if !self.connections.llm_provider.is_local() {
            return Err(failure("Ce fournisseur ne charge pas de modèle local."));
        }
        let response = self
            .http
            .post(format!(
                "{}/api/v1/models/unload",
                self.connections.lm_studio_url
            ))
            .json(&json!({"instance_id":instance}))
            .send()
            .await
            .map_err(|e| network("LM Studio", &e))?;
        Self::checked(response, "LM Studio (déchargement)").await?;
        Ok(())
    }

    /// Bounded structured operation for memory, idle consolidation or a separate
    /// illustration scene. It never shares the spoken token/audio stream.
    pub async fn structured<T: serde::de::DeserializeOwned>(
        &self,
        messages: Vec<ChatMessage>,
        schema: Value,
        limit: u64,
    ) -> Result<T> {
        const OUTPUT: u32 = 768;
        if crate::conversation::estimate(&messages) + OUTPUT as usize + 512 > limit as usize {
            return Err(failure(
                "La demande de mémoire dépasse le budget du modèle. Utilise une demande plus courte ou le panneau Mémoire.",
            ));
        }
        if !self.connections.llm_provider.is_local() {
            return self
                .api_structured(messages, schema, OUTPUT, Duration::from_secs(60), limit)
                .await;
        }
        let mut body = json!({"model":self.connections.llm_model,"messages":messages,"stream":false,"temperature":0,"max_tokens":OUTPUT,"response_format":{"type":"json_schema","json_schema":{"name":"sherka_memory","strict":true,"schema":schema}}});
        if let Some(effort) = self.connections.reasoning_effort() {
            body["reasoning_effort"] = json!(effort);
        }
        let mut response = self
            .http
            .post(format!(
                "{}/v1/chat/completions",
                self.connections.lm_studio_url
            ))
            .timeout(Duration::from_secs(60))
            .json(&body)
            .send()
            .await
            .map_err(|e| network("LM Studio (mémoire)", &e))?;
        response = Self::checked(response, "LM Studio (mémoire)").await?;
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| network("LM Studio (mémoire)", &e))?
        {
            if bytes.len() + chunk.len() > 64_000 {
                return Err(failure("La proposition de mémoire est trop longue."));
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes)?;
        if value["choices"][0]["finish_reason"] != "stop" {
            return Err(failure(
                "La proposition de mémoire est incomplète ; aucune modification n'a été faite.",
            ));
        }
        let content = value["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| failure("Proposition de mémoire illisible."))?;
        serde_json::from_str(content).map_err(|_|failure("La proposition de mémoire n'a pas le format attendu ; aucune modification n'a été faite."))
    }

    /// A short local decision; private memories/history are not provided.
    /// Invalid plans never become external requests.
    pub async fn web_plan(&self, user: &str, limit: u64) -> Result<crate::web::Plan> {
        let (messages, schema) = crate::web::plan_request(user, &self.connections.language);
        if crate::conversation::estimate(&messages) + 1024 > limit as usize {
            return Err(failure(
                "Le contexte disponible est trop petit pour décider de la recherche Internet.",
            ));
        }
        if !self.connections.llm_provider.is_local() {
            let plan: crate::web::Plan = self
                .api_structured(messages, schema, 2048, Duration::from_secs(20), limit)
                .await?;
            if plan.search {
                crate::web::query(&plan.query)?;
            } else if !plan.query.is_empty() {
                return Err(failure("La décision de recherche Internet est invalide."));
            }
            return Ok(plan);
        }
        let mut body = json!({"model":self.connections.llm_model,"messages":messages,"stream":false,"temperature":0,"max_tokens":256,
            "response_format":{"type":"json_schema","json_schema":{"name":"sherka_web_search","strict":true,"schema":schema}}});
        if self.connections.reasoning.is_some() {
            body["reasoning_effort"] = json!("none");
        }
        let mut response = self
            .http
            .post(format!(
                "{}/v1/chat/completions",
                self.connections.lm_studio_url
            ))
            .timeout(Duration::from_secs(20))
            .json(&body)
            .send()
            .await
            .map_err(|e| network("LM Studio", &e))?;
        response = Self::checked(response, "LM Studio").await?;
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| network("LM Studio", &e))?
        {
            if bytes.len() + chunk.len() > 32_000 {
                return Err(failure(
                    "La décision de recherche Internet est trop longue.",
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes)?;
        if value["choices"][0]["finish_reason"] != "stop" {
            return Err(failure("La décision de recherche Internet est incomplète."));
        }
        let content = value["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| failure("La décision de recherche Internet est illisible."))?;
        let plan: crate::web::Plan = serde_json::from_str(content)?;
        if plan.search {
            crate::web::query(&plan.query)?;
        } else if !plan.query.is_empty() {
            return Err(failure("La décision de recherche Internet est invalide."));
        }
        Ok(plan)
    }

    /// VoiceStudio 0.5.6 synthesizes the whole request before returning PCM.
    /// Callers prepare coherent chunks ahead of playback; not token-level TTS.
    pub async fn speech(&self, text: &str) -> Result<Vec<i16>> {
        let c = &self.connections;
        let engine = c
            .tts_engine
            .as_deref()
            .ok_or_else(|| failure("Choisis un moteur vocal dans les réglages."))?;
        let voice = c
            .tts_voice
            .as_deref()
            .ok_or_else(|| failure("Choisis une voix dans les réglages."))?;
        let response = self.http.post(format!("{}/v1/audio/speech", c.voice_studio_url))
            .json(&json!({"model":engine,"voice":voice,"input":text,"response_format":"pcm","stream_format":"audio","language":c.language,"speed":c.speed}))
            .send().await.map_err(|e| network("VoiceStudio (voix)", &e))?;
        let mut response = Self::checked(response, "VoiceStudio (voix)").await?;
        let mime = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if !mime.starts_with("audio/pcm") && !mime.starts_with("application/octet-stream") {
            return Err(failure(
                "VoiceStudio n'a pas renvoyé le PCM 24 kHz demandé.",
            ));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| network("VoiceStudio (voix)", &e))?
        {
            if bytes.len() + chunk.len() > 24_000 * 2 * 45 {
                return Err(failure("La phrase audio dépasse 45 secondes."));
            }
            bytes.extend_from_slice(&chunk);
        }
        if bytes.is_empty() || bytes.len() % 2 != 0 {
            return Err(failure("VoiceStudio a renvoyé un son vide ou invalide."));
        }
        Ok(bytes
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect())
    }

    pub async fn batch_transcribe(&self, pcm: &[i16]) -> Result<String> {
        let c = &self.connections;
        let model = c
            .stt_model
            .as_deref()
            .ok_or_else(|| failure("Choisis un moteur de transcription dans les réglages."))?;
        let file = multipart::Part::bytes(wav(pcm))
            .file_name("utterance.wav")
            .mime_str("audio/wav")
            .map_err(|_| failure("Format audio invalide."))?;
        let form = multipart::Form::new()
            .part("file", file)
            .text("model", model.to_owned())
            .text("language", c.language.clone())
            .text("response_format", "json");
        let response = self
            .http
            .post(format!("{}/v1/audio/transcriptions", c.voice_studio_url))
            .multipart(form)
            .send()
            .await
            .map_err(|e| network("VoiceStudio (transcription)", &e))?;
        let mut response = Self::checked(response, "VoiceStudio (transcription)").await?;
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| network("VoiceStudio (transcription)", &e))?
        {
            if bytes.len() + chunk.len() > 256_000 {
                return Err(failure("Transcription trop volumineuse."));
            }
            bytes.extend_from_slice(&chunk);
        }
        let result: Value = serde_json::from_slice(&bytes)
            .map_err(|_| failure("VoiceStudio a renvoyé une transcription illisible."))?;
        result["text"]
            .as_str()
            .map(|s| s.trim().into())
            .ok_or_else(|| failure("La transcription VoiceStudio ne contient pas de texte."))
    }

    /// A logical utterance can span multiple 20-second technical sessions.
    /// Their summaries are concatenated and committed once, only on local EOF.
    /// Bounded PCM is retained solely in RAM for automatic HTTP fallback.
    pub async fn transcribe<F>(
        &self,
        mut input: mpsc::Receiver<Vec<i16>>,
        mut progress: F,
    ) -> Result<String>
    where
        F: FnMut(&str, bool) + Send,
    {
        let mut result = String::new();
        let mut use_ws = true;
        let sherpa = self
            .connections
            .stt_model
            .as_deref()
            .is_some_and(|s| s.starts_with("sherpa-"));
        // This VoiceStudio version's WS capture route follows dictation prefs
        // for non-Sherpa IDs, even when ?model=faster-whisper is supplied.
        // Avoid transcribing twice when that would silently select another model.
        if !sherpa
            && let Ok(response) = self
                .http
                .get(format!(
                    "{}/dictation/prefs",
                    self.connections.voice_studio_url
                ))
                .timeout(Duration::from_secs(3))
                .send()
                .await
            && response.status().is_success()
            && response.content_length().is_some_and(|n| n < 4096)
            && let Ok(prefs) = response.json::<Value>().await
            && prefs["enabled"] == true
            && prefs["model_id"]
                .as_str()
                .is_some_and(|s| s.starts_with("sherpa-"))
        {
            use_ws = false;
            progress(
                "Whisper : transcription automatique en fin de phrase. La dictée directe de VoiceStudio utilise un autre modèle.",
                true,
            );
        }
        let mut ended = false;
        while !ended {
            let Some(first) = input.recv().await else {
                break;
            };
            let mut pcm = first;
            let mut url = url::Url::parse(&format!(
                "{}/v1/audio/transcriptions/stream",
                self.connections.voice_studio_url
            ))
            .map_err(|_| failure("Adresse VoiceStudio invalide."))?;
            let scheme = if url.scheme() == "https" { "wss" } else { "ws" };
            url.set_scheme(scheme)
                .map_err(|_| failure("Adresse VoiceStudio invalide."))?;
            url.query_pairs_mut()
                .append_pair("pcm", "1")
                .append_pair("sr", "16000")
                .append_pair("model", self.connections.stt_model.as_deref().unwrap_or(""));
            let mut socket = if use_ws {
                // The bounded WS configuration protects against oversized JSON.
                let config = WebSocketConfig::default()
                    .max_message_size(Some(256_000))
                    .max_frame_size(Some(256_000));
                match tokio::time::timeout(
                    Duration::from_secs(8),
                    tokio_tungstenite::connect_async_with_config(url.as_str(), Some(config), false),
                )
                .await
                {
                    Ok(Ok((socket, _))) => Some(socket),
                    _ => {
                        use_ws = false;
                        progress(
                            "Transcription HTTP automatique (streaming indisponible).",
                            true,
                        );
                        None
                    }
                }
            } else {
                None
            };
            if let Some(ws) = socket.as_mut()
                && ws
                    .send(WsMessage::Binary(pcm_bytes(&pcm).into()))
                    .await
                    .is_err()
            {
                socket = None;
            }
            let mut final_text = None;
            let mut segments = String::new();
            while pcm.len() < 16_000 * 20 {
                if let Some(ws) = socket.as_mut() {
                    tokio::select! {
                        frame = input.recv() => match frame {
                            Some(frame) => {
                                let sent = ws.send(WsMessage::Binary(pcm_bytes(&frame).into())).await;
                                pcm.extend(frame);
                                if sent.is_err() { socket = None; }
                            },
                            None => { ended = true; break; }
                        },
                        event = ws.next() => {
                            match event {
                                Some(Ok(WsMessage::Text(text))) => {
                                    let v: Value = serde_json::from_str(&text).map_err(|_| failure("Événement de transcription illisible."))?;
                                    match v["type"].as_str() {
                                        Some("partial")|Some("final") => {
                                            let partial = v["text"].as_str().unwrap_or("");
                                            if v["type"] == "final" { segments=crate::transcript::join(&segments,partial); }
                                            let shown=crate::transcript::join(&segments,partial);
                                            progress(&crate::transcript::join(&result,&shown),false);
                                            // Mid-session finals are display-only. The final
                                            // response must acknowledge our input_audio.end.
                                        },
                                        Some("error") => socket = None,
                                        _ => {}
                                    }
                                },
                                Some(Ok(WsMessage::Ping(p))) => { let _ = ws.send(WsMessage::Pong(p)).await; },
                                Some(Ok(_)) => {},
                                _ => socket = None,
                            }
                        }
                    }
                } else {
                    match input.recv().await {
                        Some(frame) => pcm.extend(frame),
                        None => {
                            ended = true;
                            break;
                        }
                    }
                }
            }
            if let Some(mut ws) = socket {
                let flush = async {
                    ws.send(WsMessage::Text("{\"type\":\"input_audio.end\"}".into()))
                        .await
                        .ok()?;
                    while let Some(Ok(event)) = ws.next().await {
                        if let WsMessage::Text(text) = event {
                            let v: Value = serde_json::from_str(&text).ok()?;
                            if v["type"] == "error" {
                                return None;
                            }
                            if v["type"] == "final" && v["final_kind"] != "summary" {
                                segments = crate::transcript::join(
                                    &segments,
                                    v["text"].as_str().unwrap_or(""),
                                );
                            }
                            if v["type"] == "final" && v["final_kind"] == "summary" {
                                if let Some(engine) = v["engine"].as_str()
                                    && Some(engine) != self.connections.stt_model.as_deref()
                                    && !(sherpa && engine == "sherpa-onnx-asr")
                                    && !(engine == "none" && v["text"].as_str() == Some(""))
                                {
                                    return None;
                                }
                                return v["text"]
                                    .as_str()
                                    .map(|s| crate::transcript::join(&segments, s));
                            }
                        }
                    }
                    None
                };
                final_text = tokio::time::timeout(Duration::from_secs(45), flush)
                    .await
                    .ok()
                    .flatten();
            }
            let text = match final_text {
                Some(text) => text,
                None => {
                    use_ws = false;
                    progress(
                        "Transcription HTTP automatique (streaming indisponible).",
                        true,
                    );
                    if sherpa {
                        let catalog =
                            crate::catalogs::CatalogClient::new(&self.connections.voice_studio_url)
                                .map_err(|e| failure(e.to_string()))?
                                .voices()
                                .await
                                .map_err(|e| failure(e.to_string()))?;
                        let model=catalog.stt_models.first().cloned().ok_or_else(|| failure("Aucun moteur de transcription HTTP de repli n'est disponible dans VoiceStudio."))?;
                        progress(
                            &format!(
                                "Dictée directe indisponible : transcription HTTP avec {model}."
                            ),
                            true,
                        );
                        let mut fallback = self.clone();
                        fallback.connections.stt_model = Some(model);
                        fallback.batch_transcribe(&pcm).await?
                    } else {
                        self.batch_transcribe(&pcm).await?
                    }
                }
            };
            result = crate::transcript::join(&result, &text);
            if result.len() > 6000 {
                return Err(failure(
                    "Cette prise de parole est trop longue pour le contexte choisi. Fais une pause et reprends avec une phrase plus courte.",
                ));
            }
            progress(result.trim(), false);
        }
        Ok(result.trim().into())
    }
}

pub fn pcm_bytes(pcm: &[i16]) -> Vec<u8> {
    pcm.iter().flat_map(|s| s.to_le_bytes()).collect()
}
pub fn wav(pcm: &[i16]) -> Vec<u8> {
    let data = pcm_bytes(pcm);
    let mut out = Vec::with_capacity(data.len() + 44);
    out.extend(b"RIFF");
    out.extend((data.len() as u32 + 36).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16_u32.to_le_bytes());
    out.extend(1_u16.to_le_bytes());
    out.extend(1_u16.to_le_bytes());
    out.extend(16000_u32.to_le_bytes());
    out.extend(32000_u32.to_le_bytes());
    out.extend(2_u16.to_le_bytes());
    out.extend(16_u16.to_le_bytes());
    out.extend(b"data");
    out.extend((data.len() as u32).to_le_bytes());
    out.extend(data);
    out
}

#[derive(Default)]
pub(crate) struct Sse {
    pending: Vec<u8>,
    data: String,
}
impl Sse {
    pub(crate) fn feed(&mut self, bytes: &[u8]) -> Result<Vec<String>> {
        self.pending.extend_from_slice(bytes);
        if self.pending.len() > 256_000 {
            return Err(failure("Événement SSE trop volumineux."));
        }
        let mut events = Vec::new();
        while let Some(end) = self.pending.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=end).collect();
            let line = std::str::from_utf8(&line)
                .map_err(|_| failure("Encodage SSE invalide."))?
                .trim_end_matches(['\n', '\r']);
            if line.is_empty() && !self.data.is_empty() {
                events.push(std::mem::take(&mut self.data));
            } else if let Some(data) = line.strip_prefix("data:") {
                if !self.data.is_empty() {
                    self.data.push('\n');
                }
                self.data.push_str(data.strip_prefix(' ').unwrap_or(data));
                if self.data.len() > 256_000 {
                    return Err(failure("Événement SSE trop volumineux."));
                }
            }
        }
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sse_handles_split_utf8_crlf_comments_and_multiple_events() {
        let raw = ": hello\r\ndata: été\r\n\r\ndata: [DONE]\n\n".as_bytes();
        let mut sse = Sse::default();
        let mut out = Vec::new();
        for b in raw {
            out.extend(sse.feed(&[*b]).unwrap());
        }
        assert_eq!(out, vec!["été", "[DONE]"]);
    }
}
