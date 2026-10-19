use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProfileDraft {
    pub name: String,
    pub personality: String,
    pub instructions: String,
    pub accent: String,
    #[serde(default)]
    pub mood_persistence: bool,
    #[serde(default)]
    pub initiative: InitiativeConfig,
    #[serde(default)]
    pub avatar_preset: Option<crate::avatar::AvatarPreset>,
    #[serde(default)]
    pub web_search: Option<crate::web::SearchMode>,
    #[serde(default)]
    pub connections: crate::connections::Connections,
    #[serde(default)]
    pub illustration: crate::illustration::Config,
    #[serde(default)]
    pub search: crate::search_provider::Config,
}

impl Default for ProfileDraft {
    fn default() -> Self {
        crate::presets::sherka(crate::i18n::Language::Fr)
    }
}

impl ProfileDraft {
    pub fn for_export(mut self) -> Self {
        // Legacy fields remain readable for older packs, but device services
        // are configured by the receiving installation, never by an avatar.
        self.connections.voice_studio_url =
            crate::connections::Connections::default().voice_studio_url;
        self.connections.stt_model = None;
        self
    }

    pub fn validated(mut self) -> Result<Self> {
        self.initiative.validate()?;
        self.connections = self.connections.validated()?;
        self.illustration = self.illustration.validated()?;
        self.search = self.search.validated()?;
        self.name = self.name.trim().to_string();
        self.personality = self.personality.trim().to_string();
        self.instructions = self.instructions.trim().to_string();
        if self.name.is_empty()
            || self.name.chars().count() > 60
            || self.name.chars().any(char::is_control)
        {
            return Err(Error::Validation(
                "Choisis un nom de 1 à 60 caractères, sur une ligne.".into(),
            ));
        }
        if self.personality.chars().count() > 12_000 || self.instructions.chars().count() > 12_000 {
            return Err(Error::Validation(
                "La personnalité et les instructions sont limitées à 12 000 caractères chacune."
                    .into(),
            ));
        }
        if !["amber", "teal", "violet", "coral"].contains(&self.accent.as_str()) {
            return Err(Error::Validation(
                "Cette couleur de profil n'est pas disponible.".into(),
            ));
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: Uuid,
    pub revision: i64,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(flatten)]
    pub details: ProfileDraft,
    /// Only normalized PNG bytes from the profile's own database are exposed.
    pub avatar: Option<String>,
    /// Neutral remains in `avatar` for existing profiles. Other slots fall back
    /// to it when absent. Paths from the user's computer are never persisted.
    pub emotion_avatars: std::collections::BTreeMap<crate::avatar::Emotion, String>,
    pub message_count: i64,
    pub memory_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub theme: Theme,
    pub comfyui_startup: ComfyStartupConfig,
    pub voice_service: Option<VoiceServiceConfig>,
    pub desktop_layouts: std::collections::BTreeMap<Uuid, DesktopLayout>,
    pub companion_pack_initialized: bool,
    pub web_search: crate::web::SearchMode,
    pub ui_language: crate::i18n::Language,
    pub default_companion: Option<Uuid>,
    pub default_companion_initialized: bool,
    pub active_profile: Option<Uuid>,
    pub always_on_top: bool,
    pub widget_scale: u16,
    pub widget_bounds: Option<crate::window::Bounds>,
    pub audio: crate::voice::AudioSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            comfyui_startup: Default::default(),
            voice_service: None,
            desktop_layouts: Default::default(),
            companion_pack_initialized: false,
            web_search: Default::default(),
            ui_language: Default::default(),
            default_companion: None,
            default_companion_initialized: false,
            active_profile: None,
            always_on_top: true,
            widget_scale: 100,
            widget_bounds: None,
            audio: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct ComfyStartupConfig {
    pub disabled: bool,
    pub directory: String,
    pub python: String,
}
impl ComfyStartupConfig {
    pub fn validated(mut self) -> Result<Self> {
        self.directory = self.directory.trim().to_owned();
        self.python = self.python.trim().to_owned();
        if [&self.directory, &self.python]
            .iter()
            .any(|path| path.len() > 4096 || path.chars().any(char::is_control))
        {
            return Err(Error::Validation(
                "Le chemin de ComfyUI ou de Python est invalide.".into(),
            ));
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    Gold,
    Sage,
    Ocean,
    Iris,
    Rose,
    Mono,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preferences {
    pub always_on_top: bool,
    pub widget_scale: u16,
}

impl Preferences {
    pub fn validate(&self) -> Result<()> {
        if !(65..=160).contains(&self.widget_scale) {
            return Err(Error::Validation(
                "La taille du widget doit être comprise entre 65 et 160 %.".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileExport {
    pub format: String,
    pub version: u32,
    pub profile: ProfileDraft,
    pub avatar_png_base64: Option<String>,
    #[serde(default)]
    pub emotion_avatars_png_base64:
        std::collections::BTreeMap<crate::avatar::Emotion, Option<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct VoiceServiceConfig {
    pub url: String,
    pub stt_model: Option<String>,
    pub autostart: bool,
    pub directory: String,
    pub python: String,
}
impl Default for VoiceServiceConfig {
    fn default() -> Self {
        Self {
            url: "http://127.0.0.1:3900".into(),
            stt_model: None,
            autostart: true,
            directory: String::new(),
            python: String::new(),
        }
    }
}
impl VoiceServiceConfig {
    pub fn validated(mut self) -> Result<Self> {
        self.url = crate::connections::base_url(&self.url)?;
        let paths = ComfyStartupConfig {
            directory: self.directory,
            python: self.python,
            ..Default::default()
        }
        .validated()?;
        self.directory = paths.directory;
        self.python = paths.python;
        if self
            .stt_model
            .as_ref()
            .is_some_and(|s| s.len() > 512 || s.chars().any(char::is_control))
        {
            return Err(Error::Validation(
                "Modèle de transcription invalide.".into(),
            ));
        }
        self.stt_model = self.stt_model.filter(|s| !s.trim().is_empty());
        Ok(self)
    }
}
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct DesktopLayout {
    pub bounds: Option<crate::window::Bounds>,
    pub image: Option<ImagePlacement>,
    pub text_height: Option<f64>,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ImagePlacement {
    pub x: f64,
    pub y: f64,
    pub size: f64,
    pub moved: bool,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct InitiativeConfig {
    pub enabled: bool,
    pub min_minutes: u16,
    pub max_minutes: u16,
}
impl Default for InitiativeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            min_minutes: 5,
            max_minutes: 15,
        }
    }
}
impl InitiativeConfig {
    pub fn validate(&self) -> Result<()> {
        if self.min_minutes < 1 || self.min_minutes > self.max_minutes || self.max_minutes > 1440 {
            return Err(Error::Validation("Choisis un délai de 1 à 1440 minutes, avec un maximum supérieur ou égal au minimum.".into()));
        }
        Ok(())
    }
}
