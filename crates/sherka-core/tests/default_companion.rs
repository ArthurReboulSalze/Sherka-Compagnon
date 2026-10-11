use sherka_core::{
    Companion, Error,
    conversation::context,
    i18n::Language,
    model::ProfileDraft,
    presets,
    session::MicrophoneState,
    storage::{Delivery, MessageRole, Store},
};
use tempfile::tempdir;

#[test]
fn builtin_sherka_has_seven_complete_personalities_and_prompts() {
    for language in Language::ALL {
        let draft = presets::sherka(language).validated().unwrap();
        assert_eq!(draft.name, "Sherka");
        assert_eq!(draft.connections.language, language.code());
        assert!(!draft.personality.is_empty());
        assert!(!draft.instructions.is_empty());
        if language != Language::Fr {
            assert_ne!(draft.personality, ProfileDraft::default().personality);
            assert_ne!(draft.instructions, ProfileDraft::default().instructions);
        }
        let system = &context(&draft, &[], "Hello").unwrap()[0].content;
        assert!(system.contains(&draft.personality));
        assert!(system.contains(&draft.instructions));
        assert!(system.contains(&format!("Respond naturally in {};", language.name())));
        assert!(system.contains("Introduce yourself only when asked"));
    }
}

#[test]
fn only_the_registered_default_follows_the_interface_and_keeps_its_models_and_voice() {
    let dir = tempdir().unwrap();
    let mut app = Companion::open(dir.path()).unwrap();
    let original = app.snapshot().unwrap().profiles.remove(0);
    let mut configured = original.details.clone();
    configured.connections.llm_model = Some("chosen-model".into());
    configured.connections.tts_engine = Some("chosen-engine".into());
    configured.connections.tts_voice = Some("chosen-voice".into());
    configured.connections.stt_model = Some("chosen-stt".into());
    configured.connections.speed = 1.1;
    app.update_profile(original.id, original.revision, configured.clone())
        .unwrap();
    let copy = app.store.duplicate(original.id).unwrap();
    let imported = app
        .store
        .import_profile(&app.store.export_profile(original.id).unwrap())
        .unwrap();
    let custom = app
        .store
        .create(
            ProfileDraft {
                name: "Orion".into(),
                personality: "My own character".into(),
                ..Default::default()
            },
            None,
        )
        .unwrap();
    let tag = app.session.begin_text_turn();
    app.record_message(
        tag,
        MessageRole::User,
        "Original history",
        Delivery::TextOnly,
    )
    .unwrap();
    app.session.cancel();
    for language in Language::ALL {
        app.set_interface_language(language).unwrap();
        let profile = app.store.profile(original.id).unwrap();
        let mut expected = configured.clone();
        let preset = presets::sherka(language);
        expected.personality = preset.personality;
        expected.instructions = preset.instructions;
        expected.connections.language = language.code().into();
        assert_eq!(profile.details, expected);
        assert_eq!(
            app.snapshot().unwrap().settings.default_companion,
            Some(original.id)
        );
        assert_eq!(app.store.profile(copy.id).unwrap().details, copy.details);
        assert_eq!(
            app.store.profile(imported.id).unwrap().details,
            imported.details
        );
        assert_eq!(
            app.store.profile(custom.id).unwrap().details,
            custom.details
        );
        let mut manual_language = profile.details.clone();
        manual_language.connections.language =
            if language == Language::Fr { "en" } else { "fr" }.into();
        assert!(matches!(
            app.update_profile(profile.id, profile.revision, manual_language),
            Err(Error::Validation(_))
        ));
    }
    drop(app);
    let app = Companion::open(dir.path()).unwrap();
    assert_eq!(
        app.store
            .profile(original.id)
            .unwrap()
            .details
            .connections
            .language,
        "ja"
    );
    assert_eq!(
        app.store.history(original.id, 10).unwrap()[0].content,
        "Original history"
    );
    assert_eq!(
        app.snapshot().unwrap().settings.default_companion,
        Some(original.id)
    );
}

#[test]
fn personalizing_the_default_detaches_it_permanently_and_inactive_defaults_do_not_interrupt() {
    let dir = tempdir().unwrap();
    let mut app = Companion::open(dir.path()).unwrap();
    let original = app.snapshot().unwrap().profiles.remove(0);
    let other = app
        .store
        .create(
            ProfileDraft {
                name: "Orion".into(),
                ..Default::default()
            },
            None,
        )
        .unwrap();
    app.switch_profile(other.id).unwrap();
    let tag = app.session.begin_text_turn();
    app.set_interface_language(Language::En).unwrap();
    assert!(app.session.accepts(tag));
    let localized = app.store.profile(original.id).unwrap();
    assert_eq!(localized.details.connections.language, "en");
    let mut customized = localized.details.clone();
    customized.personality = "My own Sherka".into();
    app.update_profile(original.id, localized.revision, customized.clone())
        .unwrap();
    assert_eq!(app.snapshot().unwrap().settings.default_companion, None);
    app.set_interface_language(Language::Ja).unwrap();
    assert_eq!(app.store.profile(original.id).unwrap().details, customized);
    assert!(app.session.accepts(tag));
    // Even restoring the stock text is an explicit custom edit, not a new preset.
    let mut restored = presets::sherka(Language::En);
    restored.connections = customized.connections;
    app.store
        .update(original.id, localized.revision + 1, restored)
        .unwrap();
    drop(app);
    let mut app = Companion::open(dir.path()).unwrap();
    app.set_interface_language(Language::De).unwrap();
    assert_eq!(
        app.store
            .profile(original.id)
            .unwrap()
            .details
            .connections
            .language,
        "en"
    );
    assert_eq!(app.snapshot().unwrap().settings.default_companion, None);
}

#[test]
fn changing_the_active_default_language_cancels_old_speech_but_same_locale_does_not() {
    let dir = tempdir().unwrap();
    let mut app = Companion::open(dir.path()).unwrap();
    app.session.capture_started();
    let tag = app.session.begin_text_turn();
    app.reset_live(tag);
    app.live.reply = "Entendu. Non entendu.".into();
    app.live.heard = "Entendu.".into();
    app.set_interface_language(Language::Fr).unwrap();
    assert!(app.session.accepts(tag));
    app.set_interface_language(Language::En).unwrap();
    assert!(!app.session.accepts(tag));
    assert_eq!(app.session.snapshot().microphone, MicrophoneState::Off);
    let history = app.store.history(tag.profile_id, 10).unwrap();
    assert_eq!(history[0].delivery, Delivery::Interrupted);
    assert_eq!(history[0].heard_content, "Entendu.");
    assert!(app.finish_response(tag, Delivery::Completed).is_err());
}

#[test]
fn an_existing_unmodified_default_is_recognized_once_on_upgrade() {
    let dir = tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let original = store.create(ProfileDraft::default(), None).unwrap();
    let settings = rusqlite::Connection::open(dir.path().join("settings.sqlite3")).unwrap();
    settings.execute("UPDATE settings SET value=?1 WHERE id=1", [r#"{"ui_language":"de","active_profile":null,"always_on_top":true,"widget_scale":100}"#]).unwrap();
    drop(settings);
    drop(store);
    let mut app = Companion::open(dir.path()).unwrap();
    assert_eq!(
        app.snapshot().unwrap().settings.default_companion,
        Some(original.id)
    );
    assert_eq!(
        app.store.profile(original.id).unwrap().details,
        presets::sherka(Language::De)
    );
    app.set_interface_language(Language::Ja).unwrap();
    assert_eq!(
        app.store.profile(original.id).unwrap().details,
        presets::sherka(Language::Ja)
    );
}
