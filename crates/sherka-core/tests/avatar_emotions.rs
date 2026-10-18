use sherka_core::{
    Companion, Error,
    avatar::{AvatarPreset, Emotion, ExpressionStream},
    model::ProfileDraft,
    storage::{Delivery, Store},
};
use std::io::Cursor;
use tempfile::tempdir;

fn png(color: u8) -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(8, 8, image::Rgba([color, 30, 90, 120]))
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    bytes.into_inner()
}

#[test]
fn expression_metadata_is_removed_at_every_unicode_stream_boundary() {
    for (name, expected) in [
        ("neutral", Emotion::Neutral),
        ("joy", Emotion::Joy),
        ("sadness", Emotion::Sadness),
        ("anger", Emotion::Anger),
        ("surprise", Emotion::Surprise),
        ("reflection", Emotion::Reflection),
        ("tenderness", Emotion::Tenderness),
    ] {
        let input = format!("\u{feff} \n[[emotion:{name}]]Bonjour ! 你好。こんにちは 🥰");
        for cut in input.char_indices().map(|(i, _)| i).chain([input.len()]) {
            let mut stream = ExpressionStream::default();
            let output =
                stream.push(&input[..cut]) + &stream.push(&input[cut..]) + &stream.finish();
            assert_eq!(output, "Bonjour ! 你好。こんにちは 🥰");
            assert_eq!(stream.emotion, expected);
        }
        let mut stream = ExpressionStream::default();
        let output: String = input.chars().map(|c| stream.push(&c.to_string())).collect();
        assert_eq!(output + &stream.finish(), "Bonjour ! 你好。こんにちは 🥰");
        assert_eq!(stream.emotion, expected);
    }
}

#[test]
fn absent_invalid_and_interrupted_headers_fall_back_without_leaking_metadata() {
    for (input, expected) in [
        (
            "Ordinary answer. [[emotion:joy]] is a quoted example.",
            "Ordinary answer. [[emotion:joy]] is a quoted example.",
        ),
        ("[[emotion:invalid]]Bonjour.", "Bonjour."),
        ("[[emotion:joy", ""),
        ("[[emot", ""),
        ("[normal text]", "[normal text]"),
        ("[[emotion:bad\nBonjour.", "Bonjour."),
    ] {
        let mut stream = ExpressionStream::default();
        assert_eq!(stream.push(input) + &stream.finish(), expected);
        assert_eq!(stream.emotion, Emotion::Neutral);
    }
    let mut stream = ExpressionStream::default();
    assert!(stream.push("[[emotion:").is_empty());
    for _ in 0..1000 {
        assert!(stream.push("invalid").is_empty());
    }
    assert_eq!(stream.push("]]Hello."), "Hello.");
    assert_eq!(stream.emotion, Emotion::Neutral);
}

#[test]
fn thinking_is_not_an_expression_and_small_model_aliases_and_fallbacks_work() {
    for (answer, emotion) in [
        ("[[Emotion:happy]]Bravo !", Emotion::Joy),
        ("[[emotion:colère]]Ça suffit.", Emotion::Anger),
        ("[[emotion:thinking]]Voyons cela.", Emotion::Reflection),
        (
            "[[emotion:neutral]]Quelle bonne nouvelle !",
            Emotion::Neutral,
        ),
        ("Quelle bonne nouvelle !", Emotion::Joy),
        ("Je suis désolée pour toi.", Emotion::Sadness),
        ("C'est inadmissible !", Emotion::Anger),
        ("Quelle surprise !", Emotion::Surprise),
        ("Je me demande comment faire.", Emotion::Reflection),
        ("Je t'aime.", Emotion::Tenderness),
        ("The Moon orbits Earth.", Emotion::Neutral),
    ] {
        let input = format!(
            "<think>Je suis désolée. [[emotion:sadness]] private 思考</think><analysis>more</analysis>{answer}"
        );
        for cut in input.char_indices().map(|(i, _)| i).chain([input.len()]) {
            let mut stream = ExpressionStream::default();
            let visible =
                stream.push(&input[..cut]) + &stream.push(&input[cut..]) + &stream.finish();
            assert!(!visible.contains("private"));
            assert!(!visible.contains("emotion:"));
            assert_eq!(stream.emotion, emotion, "{answer}");
        }
    }
    let mut incomplete = ExpressionStream::default();
    assert!(
        incomplete
            .push("<think>Unfinished private thought")
            .is_empty()
    );
    assert!(incomplete.finish().is_empty());
    assert_eq!(incomplete.emotion, Emotion::Neutral);
}

#[test]
fn images_are_isolated_validated_revisioned_and_travel_with_the_profile() {
    let dir = tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let mut a = store
        .create(
            ProfileDraft {
                avatar_preset: None,
                ..Default::default()
            },
            None,
        )
        .unwrap();
    let b = store
        .create(
            ProfileDraft {
                name: "Other".into(),
                avatar_preset: None,
                ..Default::default()
            },
            None,
        )
        .unwrap();
    for (i, emotion) in [
        Emotion::Neutral,
        Emotion::Joy,
        Emotion::Sadness,
        Emotion::Anger,
        Emotion::Surprise,
        Emotion::Reflection,
        Emotion::Tenderness,
    ]
    .into_iter()
    .enumerate()
    {
        a = store
            .set_emotion_avatar(a.id, a.revision, emotion, Some(&png(i as u8)))
            .unwrap();
    }
    assert_eq!(a.emotion_avatars.len(), 6);
    assert!(a.avatar.is_some());
    assert!(store.profile(b.id).unwrap().emotion_avatars.is_empty());
    assert!(matches!(
        store.set_emotion_avatar(a.id, a.revision - 1, Emotion::Joy, None),
        Err(Error::Conflict)
    ));
    assert!(
        store
            .set_emotion_avatar(
                a.id,
                a.revision,
                Emotion::Joy,
                Some(b"<svg onload='code'/>")
            )
            .is_err()
    );
    let unchanged = store.profile(a.id).unwrap();
    assert_eq!(unchanged.revision, a.revision);
    assert_eq!(unchanged.emotion_avatars, a.emotion_avatars);
    let duplicate = store.duplicate(a.id).unwrap();
    let exported = store.export_profile(a.id).unwrap();
    let imported = store.import_profile(&exported).unwrap();
    for profile in [duplicate, imported] {
        assert_ne!(profile.id, a.id);
        assert_eq!(profile.avatar, a.avatar);
        assert_eq!(profile.emotion_avatars, a.emotion_avatars);
        assert_eq!(profile.message_count, 0);
    }
    let mut invalid: serde_json::Value = serde_json::from_str(&exported).unwrap();
    invalid["emotion_avatars_png_base64"]["executable"] = "not-an-image".into();
    let count = store.list().unwrap().len();
    assert!(store.import_profile(&invalid.to_string()).is_err());
    assert_eq!(store.list().unwrap().len(), count);
    a = store
        .set_emotion_avatar(a.id, a.revision, Emotion::Joy, None)
        .unwrap();
    assert!(!a.emotion_avatars.contains_key(&Emotion::Joy));
    let reopened = Store::open(dir.path()).unwrap().profile(a.id).unwrap();
    assert_eq!(reopened.emotion_avatars, a.emotion_avatars);
    assert_eq!(reopened.avatar, a.avatar);
}

#[test]
fn sherka_pack_is_bundled_and_user_overrides_survive_copy_export_and_reopen() {
    let dir = tempdir().unwrap();
    let app = Companion::open(dir.path()).unwrap();
    let mut profile = app.snapshot().unwrap().profiles.remove(0);
    assert_eq!(profile.details.avatar_preset, Some(AvatarPreset::Sherka));
    for (emotion, path) in AvatarPreset::Sherka.images() {
        assert_eq!(profile.emotion_avatars[&emotion], path);
        let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../public")
            .join(path.trim_start_matches('/'));
        let image = image::open(file).unwrap().into_rgba8();
        assert!(image.pixels().any(|p| p.0[3] == 0));
    }
    profile = app
        .store
        .set_emotion_avatar(profile.id, profile.revision, Emotion::Joy, None)
        .unwrap();
    assert!(!profile.emotion_avatars.contains_key(&Emotion::Joy));
    profile = app
        .store
        .set_emotion_avatar(
            profile.id,
            profile.revision,
            Emotion::Tenderness,
            Some(&png(70)),
        )
        .unwrap();
    for copy in [
        app.store.duplicate(profile.id).unwrap(),
        app.store
            .import_profile(&app.store.export_profile(profile.id).unwrap())
            .unwrap(),
    ] {
        assert_eq!(copy.emotion_avatars, profile.emotion_avatars);
    }
    assert_eq!(
        Companion::open(dir.path())
            .unwrap()
            .store
            .profile(profile.id)
            .unwrap()
            .emotion_avatars,
        profile.emotion_avatars
    );
}

#[test]
fn schema_five_upgrades_sherka_once_and_preserves_custom_avatars() {
    for custom in [false, true] {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let bytes = png(25);
        let a = store
            .create(
                ProfileDraft {
                    avatar_preset: None,
                    ..Default::default()
                },
                custom.then_some(bytes.as_slice()),
            )
            .unwrap();
        let path = dir
            .path()
            .join("profiles")
            .join(a.id.to_string())
            .join("profile.sqlite3");
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch("DROP TABLE avatar_emotions; PRAGMA user_version=5;")
            .unwrap();
        drop(db);
        let migrated = Store::open(dir.path()).unwrap().profile(a.id).unwrap();
        assert_eq!(migrated.avatar, a.avatar);
        assert_eq!(migrated.details.avatar_preset.is_some(), !custom);
        let revision = migrated.revision;
        assert_eq!(store.profile(a.id).unwrap().revision, revision);
    }
    let dir = tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let legacy = serde_json::json!({"format":"sherka-profile","version":1,"profile":{"name":"Legacy","personality":"","instructions":"","accent":"amber"},"avatar_png_base64":null});
    let imported = store.import_profile(&legacy.to_string()).unwrap();
    assert!(imported.emotion_avatars.is_empty());
    assert!(imported.details.avatar_preset.is_none());
}

#[test]
fn expression_is_reset_on_completion_interruption_switch_and_restart() {
    let dir = tempdir().unwrap();
    let mut app = Companion::open(dir.path()).unwrap();
    let tag = app.session.begin_text_turn();
    app.reset_live(tag);
    app.live.emotion = Emotion::Joy;
    app.live.reply = "Hello".into();
    app.finish_response(tag, Delivery::TextOnly).unwrap();
    assert_eq!(app.live.emotion, Emotion::Neutral);
    let tag = app.session.begin_text_turn();
    app.reset_live(tag);
    app.live.emotion = Emotion::Anger;
    app.stop_response().unwrap();
    assert!(!app.session.accepts(tag));
    assert_eq!(app.live.emotion, Emotion::Neutral);
    let b = app
        .store
        .create(
            ProfileDraft {
                name: "Vega".into(),
                ..Default::default()
            },
            None,
        )
        .unwrap();
    app.live.emotion = Emotion::Tenderness;
    app.switch_profile(b.id).unwrap();
    assert_eq!(app.live.emotion, Emotion::Neutral);
    assert_eq!(
        Companion::open(dir.path()).unwrap().live.emotion,
        Emotion::Neutral
    );
}

#[test]
fn expression_prompt_respects_context_budget_and_agent_boundary() {
    use sherka_core::{
        catalogs::LanguageModel,
        conversation::{budget, prepare_context_with_avatar},
        i18n::Language,
        memory::Recall,
    };
    let model = LanguageModel {
        vision: None,
        api: None,
        id: "fixture".into(),
        key: "fixture".into(),
        label: "Fixture".into(),
        loaded: Some(true),
        quantization: None,
        max_context_length: Some(8192),
        loaded_context_length: Some(8192),
        reasoning: None,
    };
    for language in Language::ALL {
        let mut profile = sherka_core::presets::sherka(language);
        let base = budget(&profile, &model).unwrap();
        let plain = prepare_context_with_avatar(
            &profile,
            &[],
            "Hello",
            &Recall::default(),
            base.clone(),
            None,
            None,
            false,
        )
        .unwrap();
        let expressive = prepare_context_with_avatar(
            &profile,
            &[],
            "Hello",
            &Recall::default(),
            base.clone(),
            None,
            None,
            true,
        )
        .unwrap();
        assert!(expressive.avatar_emotions);
        assert!(expressive.messages[0].content.contains("[[emotion:joy]]"));
        assert!(expressive.budget.input_estimate > plain.budget.input_estimate);
        assert!(
            expressive.budget.input_estimate
                + expressive.budget.response_tokens as usize
                + expressive.budget.safety_tokens
                <= expressive.budget.limit as usize
        );
        let mut tight = base.clone();
        tight.limit = (plain.budget.input_estimate
            + tight.response_tokens as usize
            + tight.safety_tokens
            + 32) as u64;
        let fallback = prepare_context_with_avatar(
            &profile,
            &[],
            "Hello",
            &Recall::default(),
            tight,
            None,
            None,
            true,
        )
        .unwrap();
        assert!(!fallback.avatar_emotions);
        assert_eq!(fallback.messages[0].content, plain.messages[0].content);
        profile.connections.llm_provider = sherka_core::connections::LlmProvider::Hermes;
        let agent = prepare_context_with_avatar(
            &profile,
            &[],
            "Hello",
            &Recall::default(),
            base,
            None,
            None,
            true,
        )
        .unwrap();
        assert!(!agent.avatar_emotions);
        assert!(!agent.messages[0].content.contains("[[emotion:"));
    }
}
