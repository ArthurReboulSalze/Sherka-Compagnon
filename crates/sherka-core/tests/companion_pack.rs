use sherka_core::{
    Companion, avatar::AvatarPreset, connections::LlmProvider, i18n::Language, presets,
    web::SearchMode,
};
use tempfile::tempdir;

#[test]
fn starter_pack_is_complete_localized_and_never_recreates_deleted_characters() {
    let root = tempdir().unwrap();
    let app = Companion::open(root.path()).unwrap();
    let profiles = app.store.list().unwrap();
    assert_eq!(profiles.len(), 5);
    assert_eq!(profiles[0].details.name, "Sherka");
    assert!(!profiles.iter().any(|p| p.details.name == "Orion"));
    for preset in [
        AvatarPreset::Sherka,
        AvatarPreset::Botsy,
        AvatarPreset::Alba,
        AvatarPreset::Kit,
        AvatarPreset::Minerva,
    ] {
        for language in Language::ALL {
            let draft = presets::companion(preset, language).validated().unwrap();
            assert_eq!(draft.name, preset.name());
            assert_eq!(draft.connections.language, language.code());
            assert!(!draft.personality.trim().is_empty());
            assert!(draft.connections.llm_model.is_none());
            assert!(draft.connections.tts_voice.is_none());
        }
        for path in std::iter::once(preset.neutral().to_owned())
            .chain(preset.images().into_iter().map(|(_, p)| p))
        {
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../public");
            let image = image::open(root.join(path.trim_start_matches('/')))
                .unwrap()
                .to_rgba8();
            assert!(image.pixels().any(|p| p[3] == 0));
            assert!(image.pixels().any(|p| p[3] > 0));
        }
    }
    let minerva = profiles
        .iter()
        .find(|p| p.details.name == "Minerva")
        .unwrap();
    assert!(minerva.details.personality.contains("colérique"));
    let kit = profiles.iter().find(|p| p.details.name == "Kit").unwrap();
    assert!(kit.details.personality.contains("pince-sans-rire"));
    app.store.delete(kit.id).unwrap();
    drop(app);
    let app = Companion::open(root.path()).unwrap();
    assert_eq!(app.store.list().unwrap().len(), 4);
    assert!(
        !app.store
            .list()
            .unwrap()
            .iter()
            .any(|p| p.details.name == "Kit")
    );
}

#[test]
fn web_mode_is_per_companion_and_agent_mode_always_bypasses_it() {
    let root = tempdir().unwrap();
    let mut app = Companion::open(root.path()).unwrap();
    let a = app.session.snapshot().profile_id;
    app.set_web_search_mode(SearchMode::Auto).unwrap();
    assert_eq!(app.web_search_mode().unwrap(), SearchMode::Auto);
    let b = app
        .store
        .list()
        .unwrap()
        .into_iter()
        .find(|p| p.id != a)
        .unwrap();
    app.set_profile_web_search(b.id, b.revision, SearchMode::Off)
        .unwrap();
    assert_eq!(app.web_search_mode().unwrap(), SearchMode::Auto);
    app.switch_profile(b.id).unwrap();
    assert_eq!(app.web_search_mode().unwrap(), SearchMode::Off);
    let mut a = app.store.profile(a).unwrap();
    a.details.connections.llm_provider = LlmProvider::Hermes;
    a.details.connections.llm_api_url = Some("http://127.0.0.1:8642".into());
    app.update_profile(a.id, a.revision, a.details).unwrap();
    app.switch_profile(a.id).unwrap();
    assert_eq!(app.web_search_mode().unwrap(), SearchMode::Off);
    let copied = app
        .store
        .import_profile(&app.store.export_profile(b.id).unwrap())
        .unwrap();
    assert_eq!(copied.details.web_search, Some(SearchMode::Off));
    drop(app);
    assert_eq!(
        Companion::open(root.path())
            .unwrap()
            .web_search_mode()
            .unwrap(),
        SearchMode::Off
    );
}

#[test]
fn legacy_candis_upgrades_to_alba_without_losing_personal_data_or_overwriting_edits() {
    use sherka_core::{
        avatar::Emotion,
        memory::MemoryDraft,
        storage::{Delivery, MessageRole},
    };
    assert_eq!(
        serde_json::from_str::<AvatarPreset>("\"candis\"").unwrap(),
        AvatarPreset::Alba
    );
    assert_eq!(
        serde_json::to_string(&AvatarPreset::Alba).unwrap(),
        "\"candis\""
    );
    for language in Language::ALL {
        let root = tempdir().unwrap();
        let mut app = Companion::open(root.path()).unwrap();
        let mut expected = presets::companion(AvatarPreset::Alba, language);
        expected.connections.llm_model = Some("personal-model".into());
        expected.connections.tts_voice = Some("personal-voice".into());
        expected.instructions = "Personal instructions remain mine.".into();
        let mut legacy = expected.clone();
        legacy.name = "Candis".into();
        legacy.personality = legacy.personality.replacen("Alba", "Candis", 1);
        let profile = app.store.create(legacy.clone(), None).unwrap();
        let png = include_bytes!("../../../tests/fixtures/botsy-neutral.png");
        let profile = app
            .store
            .set_avatar(profile.id, profile.revision, Some(png))
            .unwrap();
        let profile = app
            .store
            .set_emotion_avatar(profile.id, profile.revision, Emotion::Joy, Some(png))
            .unwrap();
        app.switch_profile(profile.id).unwrap();
        let tag = app.session.begin_text_turn();
        app.store
            .append_message(
                tag,
                MessageRole::User,
                "Keep this conversation.",
                Delivery::TextOnly,
            )
            .unwrap();
        app.store
            .write_memory(
                &app.store.memory_guard(profile.id).unwrap(),
                None,
                &MemoryDraft {
                    content: "Keep this memory.".into(),
                    category: "Personal".into(),
                    kind: "essential".into(),
                },
            )
            .unwrap();
        let memories = app.store.export_memories(profile.id).unwrap();
        let count = app.store.list().unwrap().len();
        let mut edited = legacy.clone();
        edited.personality.push_str(" My own character detail.");
        let edited = app.store.create(edited, None).unwrap();
        let mut named = legacy;
        named.name = "My companion".into();
        let named = app.store.create(named, None).unwrap();
        drop(app);
        let app = Companion::open(root.path()).unwrap();
        let migrated = app.store.profile(profile.id).unwrap();
        assert_eq!(migrated.details, expected);
        assert_eq!(migrated.avatar, profile.avatar);
        assert_eq!(migrated.emotion_avatars, profile.emotion_avatars);
        assert_eq!(
            app.store.history(profile.id, 10).unwrap()[0].content,
            "Keep this conversation."
        );
        assert_eq!(app.store.export_memories(profile.id).unwrap(), memories);
        assert_eq!(
            app.store.profile(edited.id).unwrap().details,
            edited.details
        );
        assert_eq!(app.store.profile(named.id).unwrap().details, named.details);
        assert_eq!(app.store.list().unwrap().len(), count + 2);
        let revision = migrated.revision;
        drop(app);
        assert_eq!(
            Companion::open(root.path())
                .unwrap()
                .store
                .profile(profile.id)
                .unwrap()
                .revision,
            revision
        );
    }
}
