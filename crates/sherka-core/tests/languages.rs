use sherka_core::{
    Companion, Error,
    conversation::{Phrases, context},
    i18n::Language,
    model::{ProfileDraft, Settings},
    storage::{Delivery, MessageRole},
};
use tempfile::tempdir;

#[test]
fn old_settings_default_to_french_and_all_ui_locales_round_trip() {
    let old: Settings =
        serde_json::from_str(r#"{"always_on_top":false,"widget_scale":110}"#).unwrap();
    assert_eq!(old.ui_language, Language::Fr);
    for language in Language::ALL {
        assert_eq!(
            serde_json::from_str::<Language>(&serde_json::to_string(&language).unwrap()).unwrap(),
            language
        );
        assert_eq!(Language::from_code(language.code()), Some(language));
    }
    assert!(serde_json::from_str::<Language>("\"xx\"").is_err());
}

#[test]
fn changing_interface_locale_preserves_custom_profiles_history_and_active_generation() {
    let dir = tempdir().unwrap();
    let mut app = Companion::open(dir.path()).unwrap();
    let original = app.snapshot().unwrap().profiles.remove(0);
    let mut customized = original.details.clone();
    customized.personality = "Mon propre caractère, conservé tel quel.".into();
    app.update_profile(original.id, original.revision, customized)
        .unwrap();
    let before = app.snapshot().unwrap().profiles;
    let profile = before[0].id;
    app.session.capture_started();
    let tag = app.session.begin_text_turn();
    app.record_message(
        tag,
        MessageRole::User,
        "Réglages 中文 日本語",
        Delivery::TextOnly,
    )
    .unwrap();
    app.live.reply = "Contenu opaque".into();
    let session = serde_json::to_value(app.session.snapshot()).unwrap();
    let prompt = context(&before[0].details, &[], "Bonjour").unwrap()[0]
        .content
        .clone();
    for language in Language::ALL {
        app.set_interface_language(language).unwrap();
        let snapshot = app.snapshot().unwrap();
        assert_eq!(snapshot.settings.ui_language, language);
        assert_eq!(serde_json::to_value(&snapshot.session).unwrap(), session);
        assert!(app.session.accepts(tag));
        assert_eq!(snapshot.live.reply, "Contenu opaque");
        assert_eq!(snapshot.profiles[0].revision, before[0].revision);
        assert_eq!(snapshot.profiles[0].details, before[0].details);
        assert_eq!(
            context(&snapshot.profiles[0].details, &[], "Bonjour").unwrap()[0].content,
            prompt
        );
    }
    drop(app);
    let app = Companion::open(dir.path()).unwrap();
    assert_eq!(app.snapshot().unwrap().settings.ui_language, Language::Ja);
    assert_eq!(
        app.store.history(profile, 10).unwrap()[0].content,
        "Réglages 中文 日本語"
    );
}

#[test]
fn companion_language_is_fixed_after_creation_and_survives_import() {
    let dir = tempdir().unwrap();
    let app = Companion::open(dir.path()).unwrap();
    for language in Language::ALL {
        let mut draft = ProfileDraft {
            personality: "Original opaque personality".into(),
            instructions: "Original opaque instructions".into(),
            ..Default::default()
        };
        draft.connections.language = language.code().into();
        let profile = app.store.create(draft, None).unwrap();
        let system = &context(&profile.details, &[], "Hello").unwrap()[0].content;
        assert!(system.contains(&format!("Respond naturally in {};", language.name())));
        assert!(system.contains("Original opaque personality"));
        let exported = app.store.export_profile(profile.id).unwrap();
        let imported = app.store.import_profile(&exported).unwrap();
        assert_eq!(imported.details.connections.language, language.code());
        let mut changed = profile.details.clone();
        changed.connections.language = if language == Language::Fr { "en" } else { "fr" }.into();
        assert!(matches!(
            app.store.update(profile.id, profile.revision, changed),
            Err(Error::Validation(_))
        ));
        assert_eq!(
            app.store.profile(profile.id).unwrap().details,
            profile.details
        );
    }
}

#[test]
fn cjk_speech_boundaries_respect_sentence_endings_without_losing_text() {
    for punctuation in ['。', '！', '？', '；', '：'] {
        let first = format!("{}{}", "你こんにちは".repeat(12), punctuation);
        let rest = "后面的内容です".repeat(35);
        let input = format!("{first}{rest}");
        let mut phrases = Phrases::default();
        phrases.push(&input);
        let spoken = phrases.next(false).unwrap();
        assert_eq!(spoken, first);
        let mut reconstructed = spoken;
        while let Some(chunk) = phrases.next(true) {
            reconstructed.push_str(&chunk);
        }
        assert_eq!(reconstructed, input);
    }
}
