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
        AvatarPreset::Candis,
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
