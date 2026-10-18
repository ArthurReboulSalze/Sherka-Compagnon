use sherka_core::{
    Companion,
    memory::{Consolidation, Extract, MemoryAction, MemoryDraft, MemoryPlan},
    storage::{Delivery, MessageRole},
};

fn record(app: &mut Companion, text: &str) {
    let tag = app.session.begin_text_turn();
    app.reset_live(tag);
    app.record_message(tag, MessageRole::User, text, Delivery::TextOnly)
        .unwrap();
    app.finish_response(tag, Delivery::TextOnly).unwrap();
}
fn draft(text: &str) -> MemoryDraft {
    MemoryDraft {
        content: text.into(),
        category: "Preferences".into(),
        kind: "essential".into(),
    }
}

#[test]
fn clearing_history_keeps_saved_facts_but_removes_sources_summaries_indexes_and_late_jobs() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = Companion::open(dir.path()).unwrap();
    let a = app.session.snapshot().profile_id;
    let b = app.store.duplicate(a).unwrap().id;
    let manual = app
        .store
        .write_memory(
            &app.store.memory_guard(a).unwrap(),
            None,
            &draft("Saved manual preference"),
        )
        .unwrap();
    record(&mut app, "Retiens que mon animal favori est le renard.");
    let source = app.store.memories(a, "").unwrap();
    assert_eq!(source.entries.len(), 1);
    let source = app.store.search_archive(a, "renard").unwrap().remove(0);
    app.store
        .apply_memory_plan(
            &app.store.memory_guard(a).unwrap(),
            &source,
            &[],
            MemoryPlan {
                actions: vec![MemoryAction {
                    action: "create".into(),
                    id: None,
                    content: "Saved explicit preference".into(),
                    category: "Preferences".into(),
                    kind: "essential".into(),
                }],
            },
            "privacy-fixture",
        )
        .unwrap();
    for i in 0..6 {
        record(
            &mut app,
            &format!("ERASE_UNIQUE_DIALOGUE_MARKER_{i} temporary conversation."),
        );
    }
    let batch = app.store.extraction_batch(a).unwrap().unwrap();
    let quote = |index: usize| Extract {
        source_id: batch.sources[index].id.clone(),
        quote: batch.sources[index].content.clone(),
        category: "Temporary".into(),
        kind: "detail".into(),
    };
    app.store
        .consolidate(
            &batch,
            Consolidation {
                memories: vec![quote(1), quote(2)],
                summary: vec![quote(3)],
            },
        )
        .unwrap();
    let pending = app
        .store
        .memories(a, "")
        .unwrap()
        .entries
        .into_iter()
        .find(|m| !m.approved)
        .unwrap()
        .id;
    app.store
        .approve_memory(&app.store.memory_guard(a).unwrap(), pending)
        .unwrap();
    let old_guard = app.store.memory_guard(a).unwrap();
    let other = app.store.export_memories(b).unwrap();
    let mut profile = serde_json::to_value(app.store.profile(a).unwrap()).unwrap();
    profile["message_count"] = serde_json::json!(0);
    app.session.capture_started();
    let old_tag = app.session.begin_text_turn();
    app.reset_live(old_tag);
    app.live.reply = "ERASE_UNIQUE_LIVE_MARKER".into();
    let old_token = app.session.cancellation();
    app.clear_conversation(a, false).unwrap();
    assert!(old_token.is_cancelled());
    assert!(app.finish_response(old_tag, Delivery::Completed).is_err());
    assert!(app.live.reply.is_empty());
    assert_eq!(
        app.session.snapshot().microphone,
        sherka_core::session::MicrophoneState::Listening
    );
    assert!(app.store.history(a, 100).unwrap().is_empty());
    assert!(
        app.store
            .search_archive(a, "ERASE_UNIQUE_DIALOGUE_MARKER")
            .unwrap()
            .is_empty()
    );
    let memory = app.store.memories(a, "").unwrap();
    assert_eq!(memory.entries.len(), 3); // manual, explicitly requested, approved suggestion
    assert!(
        memory
            .entries
            .iter()
            .all(|m| m.approved && m.sources.is_empty())
    );
    assert_eq!(memory.archive_count, 0);
    assert_eq!(memory.excluded_count, 0);
    assert!(memory.summary.is_empty());
    assert!(
        app.store
            .write_memory(&old_guard, None, &draft("late fact"))
            .is_err()
    );
    assert!(
        app.store
            .consolidate(
                &batch,
                Consolidation {
                    memories: vec![],
                    summary: vec![]
                }
            )
            .is_err()
    );
    assert_eq!(app.store.export_memories(b).unwrap(), other);
    assert_eq!(
        serde_json::to_value(app.store.profile(a).unwrap()).unwrap(),
        profile
    );
    let path = dir
        .path()
        .join("profiles")
        .join(a.to_string())
        .join("profile.sqlite3");
    let db = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        0
    );
    for table in [
        "messages",
        "memory_sources",
        "summary_sources",
        "summaries",
        "extraction_exclusions",
    ] {
        assert_eq!(
            db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    drop(db);
    app.store
        .delete_memory(&app.store.memory_guard(a).unwrap(), manual)
        .unwrap();
    assert!(
        !app.store
            .export_memories(a)
            .unwrap()
            .contains("Saved manual preference")
    );
    assert_eq!(app.store.memories(a, "").unwrap().entries.len(), 2);
    app.clear_conversation(a, true).unwrap();
    assert!(app.store.memories(a, "").unwrap().entries.is_empty());
    let bytes = std::fs::read(path).unwrap();
    assert!(
        !bytes
            .windows(b"ERASE_UNIQUE".len())
            .any(|w| w == b"ERASE_UNIQUE")
    );
    drop(app);
    let app = Companion::open(dir.path()).unwrap();
    assert!(app.store.history(a, 100).unwrap().is_empty());
    assert!(app.store.memories(a, "").unwrap().entries.is_empty());
    assert_eq!(app.store.export_memories(b).unwrap(), other);
}

#[test]
fn clearing_hermes_is_local_and_retires_all_old_session_references() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = Companion::open(dir.path()).unwrap();
    let id = app.session.snapshot().profile_id;
    let old = app
        .store
        .hermes_session(id, "http://127.0.0.1:9999")
        .unwrap();
    app.store
        .update_hermes_session_tip(&old, "old-tip")
        .unwrap();
    app.clear_conversation(id, false).unwrap();
    assert_ne!(
        app.store
            .hermes_session(id, "http://127.0.0.1:9999")
            .unwrap(),
        old
    );
    assert_eq!(app.store.hermes_session_tip(&old).unwrap(), old);
}
