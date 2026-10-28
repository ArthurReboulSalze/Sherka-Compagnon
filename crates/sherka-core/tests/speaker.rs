use sherka_core::{
    Companion,
    model::ProfileDraft,
    session::OperationTag,
    storage::{Delivery, MessageRole, Speaker},
};
use tempfile::tempdir;
use uuid::Uuid;

fn draft(name: &str) -> ProfileDraft {
    ProfileDraft {
        name: name.into(),
        ..Default::default()
    }
}

fn db_path(root: &std::path::Path, id: Uuid) -> std::path::PathBuf {
    root.join("profiles")
        .join(id.to_string())
        .join("profile.sqlite3")
}

fn version(path: &std::path::Path) -> i64 {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap()
}

fn reply_tag(tag: OperationTag) -> OperationTag {
    OperationTag {
        generation_id: Uuid::new_v4(),
        ..tag
    }
}

#[test]
fn new_databases_are_at_version_ten_and_nine_migrates_with_null_speakers() {
    let dir = tempdir().unwrap();
    let mut app = Companion::open(dir.path()).unwrap();
    let id = app.session.snapshot().profile_id;
    assert_eq!(version(&dir.path().join("settings.sqlite3")), 10);
    let tag = app.session.begin_text_turn();
    app.reset_live(tag);
    app.record_message(tag, MessageRole::User, "Bonjour", Delivery::TextOnly)
        .unwrap();
    app.live.reply = "Salut".into();
    app.finish_response(tag, Delivery::TextOnly).unwrap();
    assert_eq!(version(&db_path(dir.path(), id)), 10);
    drop(app);
    let db = rusqlite::Connection::open(db_path(dir.path(), id)).unwrap();
    db.execute_batch(
        "ALTER TABLE messages DROP COLUMN speaker_id; ALTER TABLE messages DROP COLUMN speaker_name; PRAGMA user_version=9;",
    )
    .unwrap();
    drop(db);
    let app = Companion::open(dir.path()).unwrap();
    let history = app.store.history(id, 100).unwrap();
    assert_eq!(history.len(), 2);
    assert!(history.iter().all(|m| m.speaker.is_none()));
    assert_eq!(history[1].content, "Salut");
    assert_eq!(version(&db_path(dir.path(), id)), 10);
    // A version-9 database that already has the columns migrates as well.
    let db = rusqlite::Connection::open(db_path(dir.path(), id)).unwrap();
    db.execute_batch("PRAGMA user_version=9;").unwrap();
    drop(db);
    let app = Companion::open(dir.path()).unwrap();
    assert_eq!(app.store.history(id, 100).unwrap().len(), 2);
    assert_eq!(version(&db_path(dir.path(), id)), 10);
}

#[test]
fn peer_replies_and_user_copies_land_in_the_given_profile_with_attribution() {
    let dir = tempdir().unwrap();
    let mut app = Companion::open(dir.path()).unwrap();
    let master = app.session.snapshot().profile_id;
    let guest = app.store.create(draft("Minerva"), None).unwrap().id;
    let tag = app.session.begin_text_turn();
    app.reset_live(tag);
    app.record_message(
        tag,
        MessageRole::User,
        "Salut vous deux",
        Delivery::TextOnly,
    )
    .unwrap();
    app.store
        .append_user_message_to(guest, tag, "Salut vous deux", Delivery::TextOnly)
        .unwrap();
    let master_speaker = Speaker {
        id: master,
        name: "Sherka".into(),
    };
    let guest_speaker = Speaker {
        id: guest,
        name: "Minerva".into(),
    };
    app.live.reply = "Coucou !".into();
    app.finish_response(tag, Delivery::Completed).unwrap();
    app.store
        .append_peer_reply(
            guest,
            tag,
            &master_speaker,
            "Coucou !",
            "Coucou !",
            Delivery::Completed,
        )
        .unwrap();
    let second = reply_tag(tag);
    app.store
        .append_peer_reply(
            master,
            second,
            &guest_speaker,
            "Bonjour, je réponds.",
            "Bonjour,",
            Delivery::Interrupted,
        )
        .unwrap();

    let mine = app.store.history(master, 100).unwrap();
    assert_eq!(mine.len(), 3);
    assert!(mine[0].speaker.is_none() && mine[1].speaker.is_none());
    assert_eq!(mine[2].speaker.as_ref(), Some(&guest_speaker));
    assert_eq!(mine[2].role, MessageRole::Assistant);
    assert_eq!(mine[2].delivery, Delivery::Interrupted);
    assert_eq!(mine[2].heard_content, "Bonjour,");
    assert_eq!(mine[2].id, second.generation_id.to_string());
    let theirs = app.store.history(guest, 100).unwrap();
    assert_eq!(theirs.len(), 2);
    assert_eq!(theirs[0].role, MessageRole::User);
    assert!(theirs[0].speaker.is_none());
    assert_eq!(theirs[0].content, "Salut vous deux");
    assert_eq!(theirs[1].speaker.as_ref(), Some(&master_speaker));
    assert_eq!(theirs[1].id, tag.generation_id.to_string());
    assert_eq!(
        serde_json::to_value(&mine[2]).unwrap()["speaker"]["name"],
        "Minerva"
    );

    // Copying the same reply again replaces it instead of duplicating it.
    app.store
        .append_peer_reply(
            master,
            second,
            &guest_speaker,
            "Bonjour, je réponds mieux.",
            "Bonjour, je réponds mieux.",
            Delivery::Completed,
        )
        .unwrap();
    let mine = app.store.history(master, 100).unwrap();
    assert_eq!(mine.len(), 3);
    assert_eq!(mine[2].content, "Bonjour, je réponds mieux.");
    assert_eq!(mine[2].delivery, Delivery::Completed);
    // Silence leaves nothing behind.
    app.store
        .append_peer_reply(
            master,
            reply_tag(tag),
            &guest_speaker,
            "  ",
            "",
            Delivery::Completed,
        )
        .unwrap();
    assert_eq!(app.store.history(master, 100).unwrap().len(), 3);
    // Peer rows stay available to the prompt.
    assert_eq!(app.store.context_history(master).unwrap().len(), 3);
}

#[test]
fn memory_extraction_reads_only_the_users_own_words() {
    let dir = tempdir().unwrap();
    let mut app = Companion::open(dir.path()).unwrap();
    let master = app.session.snapshot().profile_id;
    let guest = app.store.create(draft("Minerva"), None).unwrap().id;
    let speaker = Speaker {
        id: guest,
        name: "Minerva".into(),
    };
    for n in 0..8 {
        let tag = app.session.begin_text_turn();
        app.reset_live(tag);
        app.store
            .append_peer_reply(
                master,
                tag,
                &speaker,
                &format!("PEERWORDS {n}"),
                "",
                Delivery::Completed,
            )
            .unwrap();
    }
    assert!(app.store.extraction_batch(master).unwrap().is_none());
    for n in 0..6 {
        let tag = app.session.begin_text_turn();
        app.reset_live(tag);
        app.record_message(
            tag,
            MessageRole::User,
            &format!("Mon message numéro {n}"),
            Delivery::TextOnly,
        )
        .unwrap();
        app.store
            .append_peer_reply(
                master,
                reply_tag(tag),
                &speaker,
                &format!("PEERWORDS suite {n}"),
                "",
                Delivery::Completed,
            )
            .unwrap();
    }
    let batch = app.store.extraction_batch(master).unwrap().unwrap();
    assert_eq!(batch.sources.len(), 6);
    assert!(batch.sources.iter().all(|s| !s.content.contains("PEER")));
    // A peer reply is never a memory source for a turn either.
    let tag = app.session.begin_text_turn();
    app.store
        .append_peer_reply(master, tag, &speaker, "PEERWORDS", "", Delivery::Completed)
        .unwrap();
    assert!(app.store.source_for_turn(master, tag.turn_id).is_err());
}

#[test]
fn profile_export_still_contains_no_history_or_speakers() {
    let dir = tempdir().unwrap();
    let mut app = Companion::open(dir.path()).unwrap();
    let master = app.session.snapshot().profile_id;
    let guest = app.store.create(draft("Minerva"), None).unwrap().id;
    let tag = app.session.begin_text_turn();
    app.reset_live(tag);
    app.record_message(tag, MessageRole::User, "SECRETTEXT", Delivery::TextOnly)
        .unwrap();
    app.store
        .append_peer_reply(
            master,
            reply_tag(tag),
            &Speaker {
                id: guest,
                name: "Minerva".into(),
            },
            "PEERSECRET",
            "",
            Delivery::Completed,
        )
        .unwrap();
    let exported = app.store.export_profile(master).unwrap();
    assert!(!exported.contains("SECRETTEXT"));
    assert!(!exported.contains("PEERSECRET"));
    assert!(!exported.contains("speaker"));
}
