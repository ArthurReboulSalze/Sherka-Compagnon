use crate::{connections::Connections, i18n::Language, model::ProfileDraft};
use serde::Deserialize;
use std::{collections::HashMap, sync::LazyLock};

#[derive(Deserialize)]
struct Character {
    personality: String,
    instructions: String,
}

static SHERKA: LazyLock<HashMap<String, Character>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../config/presets/sherka.json"))
        .expect("validated built-in Sherka presets")
});

pub fn sherka(language: Language) -> ProfileDraft {
    let character = &SHERKA[language.code()];
    ProfileDraft {
        name: "Sherka".into(),
        personality: character.personality.clone(),
        instructions: character.instructions.clone(),
        accent: "amber".into(),
        avatar_preset: Some(crate::avatar::AvatarPreset::Sherka),
        web_search: None,
        illustration: crate::illustration::Config::default(),
        search: crate::search_provider::Config::default(),
        connections: Connections {
            language: language.code().into(),
            ..Default::default()
        },
    }
}

static COMPANIONS: LazyLock<HashMap<String, HashMap<String, String>>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../config/presets/companions.json"))
        .expect("valid preset catalog")
});

pub fn companion(preset: crate::avatar::AvatarPreset, language: Language) -> ProfileDraft {
    use crate::avatar::AvatarPreset;
    let mut draft = sherka(language);
    if preset != AvatarPreset::Sherka {
        draft.name = preset.name().into();
        draft.personality = COMPANIONS[preset.name()][language.code()].clone();
        draft.accent = match preset {
            AvatarPreset::Botsy => "amber",
            AvatarPreset::Alba => "coral",
            AvatarPreset::Kit => "teal",
            _ => "violet",
        }
        .into();
        draft.avatar_preset = Some(preset);
    }
    draft
}

/// Update the shipped identity while retaining all personal settings. Edited
/// names/personality text are user-owned and do not match this migration.
pub(crate) fn upgrade_candis(draft: &mut ProfileDraft) -> bool {
    if draft.avatar_preset != Some(crate::avatar::AvatarPreset::Alba) || draft.name != "Candis" {
        return false;
    }
    let Some(personality) = COMPANIONS["Alba"].get(&draft.connections.language) else {
        return false;
    };
    if draft.personality != personality.replacen("Alba", "Candis", 1) {
        return false;
    }
    draft.name = "Alba".into();
    draft.personality.clone_from(personality);
    true
}

pub(crate) fn is_builtin_sherka(draft: &ProfileDraft) -> bool {
    draft.name == "Sherka"
        && SHERKA
            .get(&draft.connections.language)
            .is_some_and(|character| {
                draft.personality == character.personality
                    && draft.instructions == character.instructions
            })
}
