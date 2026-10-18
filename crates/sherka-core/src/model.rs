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
    pub fn validated(mut self) -> Result<Self> {
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
