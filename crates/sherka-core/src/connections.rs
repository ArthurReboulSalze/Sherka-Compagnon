use crate::{Error, Result};
use serde::{Deserialize, Serialize};

pub const DEFAULT_RESPONSE_TOKENS: u32 = 8192;
pub const FALLBACK_CONTEXT_TOKENS: u64 = 64_000;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LlmProvider {
    #[default]
    LmStudio,
    OpenRouter,
    Zai,
    ZaiCoding,
    OpenAi,
    Anthropic,
    CustomOpenAi,
    CustomAnthropic,
    Hermes,
}
impl LlmProvider {
    pub fn is_agent(self) -> bool {
        self == Self::Hermes
    }
    pub fn is_local(&self) -> bool {
        *self == Self::LmStudio
    }
    pub fn is_anthropic(self) -> bool {
        matches!(self, Self::Anthropic | Self::CustomAnthropic)
    }
    pub fn is_zai(self) -> bool {
        matches!(self, Self::Zai | Self::ZaiCoding)
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::LmStudio => "LM Studio",
            Self::OpenRouter => "OpenRouter",
            Self::Zai => "Z.ai",
            Self::ZaiCoding => "Z.ai Coding Plan",
            Self::OpenAi => "OpenAI",
            Self::Anthropic => "Anthropic",
            Self::CustomOpenAi => "API OpenAI",
            Self::CustomAnthropic => "API Anthropic",
            Self::Hermes => "Hermes",
        }
    }
    pub fn default_url(self) -> Option<&'static str> {
        match self {
            Self::LmStudio => None,
            Self::OpenRouter => Some("https://openrouter.ai/api/v1"),
            Self::Zai => Some("https://api.z.ai/api/paas/v4"),
            Self::ZaiCoding => Some("https://api.z.ai/api/coding/paas/v4"),
            Self::OpenAi => Some("https://api.openai.com/v1"),
            Self::Anthropic => Some("https://api.anthropic.com/v1"),
            _ => None,
        }
    }
}

/// Model/voice references belong to the companion. No credentials are stored
/// here or included in profile exports.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Connections {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hermes_profile: Option<String>,
    #[serde(skip_serializing_if = "LlmProvider::is_local")]
    pub llm_provider: LlmProvider,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_api_url: Option<String>,
    pub lm_studio_url: String,
    pub voice_studio_url: String,
    pub llm_model: Option<String>,
    pub llm_model_key: Option<String>,
    pub tts_engine: Option<String>,
    pub tts_voice: Option<String>,
    pub stt_model: Option<String>,
    pub language: String,
    pub speed: f32,
    pub volume: f32,
    pub context_limit: Option<u32>,
    pub response_tokens: u32,
    pub reasoning: Option<String>,
}

impl Default for Connections {
    fn default() -> Self {
        Self {
            hermes_profile: None,
            llm_provider: LlmProvider::LmStudio,
            llm_api_url: None,
            lm_studio_url: "http://127.0.0.1:1234".into(),
            voice_studio_url: "http://127.0.0.1:3900".into(),
            llm_model: None,
            llm_model_key: None,
            tts_engine: None,
            tts_voice: None,
            stt_model: None,
            language: "fr".into(),
            speed: 1.0,
            volume: 0.8,
            context_limit: None,
            response_tokens: DEFAULT_RESPONSE_TOKENS,
            reasoning: None,
        }
    }
}

impl Connections {
    pub fn validated(mut self) -> Result<Self> {
        if self.reasoning.as_deref().is_some_and(|s| {
            ![
                "off", "on", "minimal", "low", "medium", "high", "xhigh", "max",
            ]
            .contains(&s)
        }) {
            return Err(Error::Validation(
                "Le réglage de pensée est invalide.".into(),
            ));
        }
        if self
            .context_limit
            .is_some_and(|n| !(2048..=2_000_000).contains(&n))
            || !(128..=8192).contains(&self.response_tokens)
        {
            return Err(Error::Validation(
                "Le plafond de contexte ou la réserve de réponse est invalide.".into(),
            ));
        }
        if !self.speed.is_finite()
            || !(0.5..=2.0).contains(&self.speed)
            || !self.volume.is_finite()
            || !(0.0..=1.0).contains(&self.volume)
            || !(2..=8).contains(&self.language.len())
            || !self
                .language
                .bytes()
                .all(|c| c.is_ascii_alphabetic() || c == b'-')
        {
            return Err(Error::Validation(
                "La langue, la vitesse ou le volume vocal est invalide.".into(),
            ));
        }
        self.lm_studio_url = base_url(&self.lm_studio_url)?;
        self.voice_studio_url = base_url(&self.voice_studio_url)?;
        if let Some(value) = self.llm_api_url.take().filter(|s| !s.trim().is_empty()) {
            self.llm_api_url = Some(if self.llm_provider.is_agent() {
                crate::hermes::server_url(&value)?
            } else {
                api_base_url(&value)?
            });
        }
        if self.llm_provider.is_agent() {
            self.hermes_profile = Some(crate::hermes::profile_name(
                self.hermes_profile.as_deref().unwrap_or("default"),
            )?);
        }
        if !self.llm_provider.is_local() {
            self.api_base()?;
        }
        for reference in [
            &mut self.llm_model,
            &mut self.llm_model_key,
            &mut self.tts_engine,
            &mut self.tts_voice,
            &mut self.stt_model,
        ] {
            if let Some(value) = reference {
                *value = value.trim().to_owned();
                if value.len() > 512 || value.chars().any(char::is_control) {
                    return Err(Error::Validation(
                        "L'identifiant de modèle ou de voix est invalide.".into(),
                    ));
                }
                if value.is_empty() {
                    *reference = None;
                }
            }
        }
        Ok(self)
    }

    pub fn validate_reasoning(&self, model: &crate::catalogs::LanguageModel) -> Result<()> {
        if let Some(choice) = &self.reasoning
            && !model
                .reasoning
                .as_ref()
                .is_some_and(|r| r.allowed_options.contains(choice))
        {
            return Err(Error::Validation("Ce modèle ne propose pas le réglage de pensée enregistré. Actualise ses options dans Réglages.".into()));
        }
        Ok(())
    }

    /// LM Studio 0.4.24 Chat Completions: none maps to off; medium maps
    /// to on for models advertising a binary thinking toggle. Leveled
    /// choices are only exposed when the native catalog advertises them.
    pub fn reasoning_effort(&self) -> Option<&str> {
        self.reasoning.as_deref().map(|s| match s {
            "off" => "none",
            "on" => "medium",
            other => other,
        })
    }

    pub fn api_base(&self) -> Result<String> {
        if self.llm_provider.is_agent() {
            return crate::hermes::profile_url(self);
        }
        if self.llm_provider.is_local() {
            return Ok(format!("{}/v1", self.lm_studio_url));
        }
        api_base_url(
            self.llm_api_url
                .as_deref()
                .or(self.llm_provider.default_url())
                .ok_or_else(|| {
                    Error::Validation("Indique l’adresse de base de l’API du fournisseur.".into())
                })?,
        )
    }
}

/// Unlike LM Studio's host normalization, preserve /api/v1 and /api/paas/v4.
/// Credentials must never travel over unencrypted public HTTP or URL userinfo.
pub fn api_base_url(value: &str) -> Result<String> {
    let invalid = || {
        Error::Validation("L’adresse de l’API doit utiliser HTTPS, sans clé ni paramètres ; HTTP est réservé au serveur local.".into())
    };
    if value.len() > 2048 {
        return Err(invalid());
    }
    let mut url = url::Url::parse(value.trim()).map_err(|_| invalid())?;
    let local = match url.host() {
        Some(url::Host::Domain("localhost")) => true,
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        _ => false,
    };
    if !(url.scheme() == "https" || url.scheme() == "http" && local)
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid());
    }
    let path = url.path().trim_end_matches('/').to_owned();
    url.set_path(&path);
    Ok(url.to_string().trim_end_matches('/').to_owned())
}

pub fn base_url(value: &str) -> Result<String> {
    let invalid = || {
        Error::Validation(
            "Indique l'adresse HTTP ou HTTPS du serveur, sans clé, mot de passe ni paramètres."
                .into(),
        )
    };
    if value.len() > 2048 {
        return Err(invalid());
    }
    let mut url = url::Url::parse(value.trim()).map_err(|_| invalid())?;
    if !["http", "https"].contains(&url.scheme())
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid());
    }
    let path = url.path().trim_end_matches('/');
    // Accept the usual API base URL pasted from a provider's settings.
    let path = ["/api/v1", "/api/v0", "/v1"]
        .iter()
        .find_map(|suffix| path.strip_suffix(suffix))
        .unwrap_or(path)
        .to_owned();
    url.set_path(&path);
    Ok(url.to_string().trim_end_matches('/').to_owned())
}
