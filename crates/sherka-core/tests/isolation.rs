use sherka_core::{
    Companion, Error,
    model::ProfileDraft,
    session::MicrophoneState,
    storage::{Delivery, MessageRole},
};
use tempfile::tempdir;

#[test]
fn interrupted_response_keeps_only_confirmed_spoken_phrases_in_context_and_stays_in_its_profile() {
    let dir = tempdir().unwrap();
    let mut app = Companion::open(dir.path()).unwrap();
    let a = app.store.list().unwrap().remove(0);
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
    let tag = app.session.begin_text_turn();
    app.reset_live(tag);
    app.record_message(tag, MessageRole::User, "Bonjour", Delivery::TextOnly)
        .unwrap();
    app.live.reply = "Entendu. NON ENTENDU".into();
    app.live.heard = "Entendu.".into();
    app.switch_profile(b.id).unwrap();
    assert!(!app.session.accepts(tag));
    assert!(app.store.history(b.id, 10).unwrap().is_empty());
    let history = app.store.history(a.id, 10).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[1].delivery, Delivery::Interrupted);
    assert_eq!(history[1].heard_content, "Entendu.");
    assert!(app.finish_response(tag, Delivery::Completed).is_err());
    let messages = sherka_core::conversation::context(&a.details, &history, "Encore").unwrap();
    let json = serde_json::to_string(&messages).unwrap();
    assert!(json.contains("Entendu."));
    assert!(!json.contains("NON ENTENDU"));
}

#[test]
fn version_one_profiles_migrate_without_losing_history() {
    let dir = tempdir().unwrap();
    let app = Companion::open(dir.path()).unwrap();
    let profile = app.store.list().unwrap().remove(0);
    let path = dir
        .path()
        .join("profiles")
        .join(profile.id.to_string())
        .join("profile.sqlite3");
    let db = rusqlite::Connection::open(path).unwrap();
    db.execute_batch("DROP TRIGGER message_insert; DROP TRIGGER message_update; DROP TRIGGER message_delete; DROP TABLE messages_fts; DROP TABLE summary_sources; DROP TABLE memory_sources; DROP TABLE memory_meta; ALTER TABLE memories DROP COLUMN kind; ALTER TABLE memories DROP COLUMN approved; ALTER TABLE memories DROP COLUMN origin; ALTER TABLE messages DROP COLUMN heard_content; ALTER TABLE messages DROP COLUMN web_sources; PRAGMA user_version=1;")
        .unwrap();
    drop(db);
    let history = app.store.history(profile.id, 10).unwrap();
    assert!(history.is_empty());
    assert_eq!(
        app.store.profile(profile.id).unwrap().details,
        profile.details
    );
}

#[test]
fn connections_are_per_profile_persisted_and_cancel_old_generations() {
    let dir = tempdir().unwrap();
    let mut app = Companion::open(dir.path()).unwrap();
    let a = app.store.list().unwrap().remove(0);
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
    let tag = app.session.begin_text_turn();
    let mut draft = a.details;
    draft.connections.llm_model = Some("my-model".into());
    draft.connections.tts_engine = Some("omnivoice".into());
    draft.connections.tts_voice = Some("voice-a".into());
    app.update_profile(a.id, a.revision, draft.clone()).unwrap();
    assert!(!app.session.accepts(tag));
    assert!(
        app.store
            .profile(b.id)
            .unwrap()
            .details
            .connections
            .llm_model
            .is_none()
    );
    let copy = app.store.duplicate(a.id).unwrap();
    assert_eq!(copy.details.connections, draft.connections);
    let exported = app.store.export_profile(a.id).unwrap();
    assert_eq!(
        app.store
            .import_profile(&exported)
            .unwrap()
            .details
            .connections,
        draft.connections
    );
    drop(app);
    let app = Companion::open(dir.path()).unwrap();
    assert_eq!(
        app.store.profile(a.id).unwrap().details.connections,
        draft.connections
    );
}

#[test]
fn profile_history_survives_restart_without_cross_contamination() {
    let dir = tempdir().unwrap();
    let mut app = Companion::open(dir.path()).unwrap();
    let a = app.session.snapshot().profile_id;
    let tag = app.session.begin_text_turn();
    app.record_message(
        tag,
        MessageRole::User,
        "Mon projet est un télescope.",
        Delivery::TextOnly,
    )
    .unwrap();
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
    app.switch_profile(b.id).unwrap();
    assert!(app.store.history(b.id, 50).unwrap().is_empty());
    assert!(matches!(
        app.record_message(
            tag,
            MessageRole::Assistant,
            "résultat tardif",
            Delivery::Uncertain
        ),
        Err(Error::StaleOperation)
    ));
    assert_eq!(app.session.snapshot().microphone, MicrophoneState::Off);
    drop(app);
    let app = Companion::open(dir.path()).unwrap();
    assert_eq!(app.session.snapshot().profile_id, b.id);
    assert_eq!(
        app.store.history(a, 50).unwrap()[0].content,
        "Mon projet est un télescope."
    );
    assert!(app.store.history(b.id, 50).unwrap().is_empty());
    assert_eq!(app.store.profile(a).unwrap().message_count, 1);
}

#[test]
fn duplicates_and_exports_copy_configuration_but_not_history_or_identity() {
    let dir = tempdir().unwrap();
    let mut app = Companion::open(dir.path()).unwrap();
    let original = app.session.snapshot().profile_id;
    let tag = app.session.begin_text_turn();
    app.record_message(
        tag,
        MessageRole::User,
        "Un souvenir privé.",
        Delivery::TextOnly,
    )
    .unwrap();
    let copy = app.store.duplicate(original).unwrap();
    assert_ne!(copy.id, original);
    assert_eq!(copy.message_count, 0);
    assert_eq!(copy.memory_count, 0);
    let json = app.store.export_profile(original).unwrap();
    assert!(!json.contains("Un souvenir privé"));
    assert!(!json.contains(&original.to_string()));
    let imported = app.store.import_profile(&json).unwrap();
    assert_ne!(imported.id, original);
    assert_eq!(imported.message_count, 0);
    assert_eq!(
        imported.details,
        app.store.profile(original).unwrap().details
    );
}

#[test]
fn stale_edits_and_invalid_imports_cannot_overwrite_a_profile() {
    let dir = tempdir().unwrap();
    let app = Companion::open(dir.path()).unwrap();
    let initial_count = app.store.list().unwrap().len();
    let profile = app.store.list().unwrap().remove(0);
    let renamed = ProfileDraft {
        name: "Sherka II".into(),
        ..profile.details.clone()
    };
    app.store
        .update(profile.id, profile.revision, renamed.clone())
        .unwrap();
    assert!(matches!(
        app.store
            .update(profile.id, profile.revision, profile.details),
        Err(Error::Conflict)
    ));
    assert_eq!(
        app.store.profile(profile.id).unwrap().details.name,
        renamed.name
    );
    assert!(
        app.store
            .import_profile(r#"{"format":"sherka-profile","version":99,"profile":{}}"#)
            .is_err()
    );
    assert!(
        app.store
            .import_profile(
                r#"{"format":"sherka-profile","version":1,"profile":{},"path":"../../elsewhere"}"#
            )
            .is_err()
    );
    assert_eq!(app.store.list().unwrap().len(), initial_count);
}

#[test]
fn deletion_is_restricted_and_missing_switch_leaves_current_session_intact() {
    let dir = tempdir().unwrap();
    let mut app = Companion::open(dir.path()).unwrap();
    let active = app.session.snapshot().profile_id;
    for profile in app.store.list().unwrap() {
        if profile.id != active {
            app.store.delete(profile.id).unwrap();
        }
    }
    assert!(matches!(app.store.delete(active), Err(Error::LastProfile)));
    let b = app.store.duplicate(active).unwrap();
    assert!(app.store.delete(active).is_err());
    let tag = app.session.begin_text_turn();
    assert!(app.switch_profile(uuid::Uuid::new_v4()).is_err());
    assert!(app.session.accepts(tag));
    app.store.delete(b.id).unwrap();
    assert_eq!(app.store.list().unwrap().len(), 1);
}

#[test]
fn profile_database_identity_and_future_versions_fail_closed() {
    let dir = tempdir().unwrap();
    let app = Companion::open(dir.path()).unwrap();
    let id = app.session.snapshot().profile_id;
    let db = rusqlite::Connection::open(
        dir.path()
            .join("profiles")
            .join(id.to_string())
            .join("profile.sqlite3"),
    )
    .unwrap();
    db.execute_batch("PRAGMA user_version = 99;").unwrap();
    assert!(matches!(app.store.profile(id), Err(Error::FutureSchema)));
    db.execute_batch("PRAGMA user_version = 1; UPDATE profile SET id='other-profile';")
        .unwrap();
    assert!(app.store.profile(id).is_err());
}

#[test]
fn incompatible_startup_never_rewrites_settings_or_migrates_other_profiles() {
    let dir = tempdir().unwrap();
    let app = Companion::open(dir.path()).unwrap();
    let id = app.session.snapshot().profile_id;
    let other = app
        .store
        .create(
            ProfileDraft {
                name: "Vega".into(),
                ..Default::default()
            },
            None,
        )
        .unwrap();
    let settings = dir.path().join("settings.sqlite3");
    let profile = dir
        .path()
        .join("profiles")
        .join(id.to_string())
        .join("profile.sqlite3");
    drop(app);
    let older_profile = dir
        .path()
        .join("profiles")
        .join(other.id.to_string())
        .join("profile.sqlite3");
    let db = rusqlite::Connection::open(&older_profile).unwrap();
    db.execute_batch("ALTER TABLE messages DROP COLUMN web_sources; PRAGMA user_version=3;")
        .unwrap();
    drop(db);
    let older_before = std::fs::read(&older_profile).unwrap();
    let db = rusqlite::Connection::open(&settings).unwrap();
    let original: String = db
        .query_row("SELECT value FROM settings WHERE id=1", [], |r| r.get(0))
        .unwrap();
    let mut future: serde_json::Value = serde_json::from_str(&original).unwrap();
    future["future_test_flag"] = serde_json::json!(true);
    db.execute(
        "UPDATE settings SET value=?1 WHERE id=1",
        [future.to_string()],
    )
    .unwrap();
    drop(db);
    let before = std::fs::read(&settings).unwrap();
    assert!(Companion::open(dir.path()).is_err());
    assert_eq!(std::fs::read(&settings).unwrap(), before);
    assert_eq!(std::fs::read(&older_profile).unwrap(), older_before);
    let db = rusqlite::Connection::open(&settings).unwrap();
    db.execute("UPDATE settings SET value=?1 WHERE id=1", [&original])
        .unwrap();
    drop(db);
    let db = rusqlite::Connection::open(&profile).unwrap();
    db.execute_batch("PRAGMA user_version=99;").unwrap();
    drop(db);
    let before = std::fs::read(&settings).unwrap();
    assert!(matches!(
        Companion::open(dir.path()),
        Err(Error::FutureSchema)
    ));
    assert_eq!(std::fs::read(&settings).unwrap(), before);
    assert_eq!(std::fs::read(&older_profile).unwrap(), older_before);
}

#[test]
fn image_import_decodes_real_pixels_and_rejects_disguised_code() {
    use std::io::Cursor;
    let dir = tempdir().unwrap();
    let app = Companion::open(dir.path()).unwrap();
    let profile = app.store.list().unwrap().remove(0);
    assert!(
        app.store
            .set_avatar(
                profile.id,
                profile.revision,
                Some(b"<svg onload='attack()'></svg>")
            )
            .is_err()
    );
    let mut png = Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(8, 8, image::Rgba([150, 120, 70, 64]))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let updated = app
        .store
        .set_avatar(profile.id, profile.revision, Some(png.get_ref()))
        .unwrap();
    assert!(
        updated
            .avatar
            .unwrap()
            .starts_with("data:image/png;base64,")
    );
    let copy = app.store.duplicate(profile.id).unwrap();
    assert!(copy.avatar.is_some());
    assert_eq!(copy.message_count, 0);
}

#[test]
fn fts_is_available_in_each_profile_database() {
    let dir = tempdir().unwrap();
    let app = Companion::open(dir.path()).unwrap();
    let a = app.session.snapshot().profile_id;
    let b = app.store.duplicate(a).unwrap().id;
    let db = rusqlite::Connection::open(
        dir.path()
            .join("profiles")
            .join(a.to_string())
            .join("profile.sqlite3"),
    )
    .unwrap();
    db.execute("INSERT INTO memories(content,category,status,created_at,updated_at,operation_key) VALUES('astronomie','projets','active',1,1,'test-source-1')", []).unwrap();
    let hits: i64 = db
        .query_row(
            "SELECT count(*) FROM memories_fts WHERE memories_fts MATCH 'astronomie'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(hits, 1);
    assert_eq!(app.store.memory_count_for(a).unwrap(), 1);
    assert_eq!(app.store.memory_count_for(b).unwrap(), 0);
}
