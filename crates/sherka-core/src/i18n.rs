use serde::{Deserialize, Serialize};
use std::sync::{
    LazyLock,
    atomic::{AtomicU8, Ordering},
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    Fr,
    En,
    Es,
    It,
    De,
    Zh,
    Ja,
}

impl Language {
    pub const ALL: [Self; 7] = [
        Self::Fr,
        Self::En,
        Self::Es,
        Self::It,
        Self::De,
        Self::Zh,
        Self::Ja,
    ];
    pub fn code(self) -> &'static str {
        match self {
            Self::Fr => "fr",
            Self::En => "en",
            Self::Es => "es",
            Self::It => "it",
            Self::De => "de",
            Self::Zh => "zh",
            Self::Ja => "ja",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Fr => "French",
            Self::En => "English",
            Self::Es => "Spanish",
            Self::It => "Italian",
            Self::De => "German",
            Self::Zh => "Simplified Chinese",
            Self::Ja => "Japanese",
        }
    }
    pub fn from_code(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|language| language.code() == value)
    }
}

static UI_LANGUAGE: AtomicU8 = AtomicU8::new(0);
static CATALOG: LazyLock<std::collections::HashMap<String, Vec<String>>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../config/locales/ui.json"))
        .expect("validated UI translation catalog")
});
pub fn set_ui_language(language: Language) {
    UI_LANGUAGE.store(language as u8, Ordering::Relaxed);
}
pub fn ui_language() -> Language {
    Language::ALL[UI_LANGUAGE.load(Ordering::Relaxed) as usize]
}
pub fn text(source: &str) -> String {
    let language = ui_language() as usize;
    if language == 0 {
        return source.into();
    }
    CATALOG
        .get(source)
        .and_then(|values| values.get(language - 1))
        .cloned()
        .unwrap_or_else(|| source.into())
}
