use sherka_core::{
    Companion,
    memory::{MemoryAction, MemoryDraft, MemoryMerge, MemoryPlan},
    storage::{Delivery, MessageRole},
};
fn fact(app: &mut Companion, text: &str, kind: &str) -> i64 {
    let tag = app.session.begin_text_turn();
    app.reset_live(tag);
    app.record_message(
        tag,
        MessageRole::User,
        &format!("Remember that {text}"),
        Delivery::TextOnly,
    )
    .unwrap();
    let source = app
        .store
        .source_for_turn(tag.profile_id, tag.turn_id)
        .unwrap();
    app.finish_response(tag, Delivery::TextOnly).unwrap();
    app.store
        .apply_memory_plan(
            &app.store.memory_guard(tag.profile_id).unwrap(),
            &source,
            &[],
            MemoryPlan {
                actions: vec![MemoryAction {
                    action: "create".into(),
                    id: None,
                    content: text.into(),
                    category: "Preferences".into(),
                    kind: kind.into(),
                }],
            },
            &source.id,
        )
        .unwrap();
    app.store
        .optimization_memories(tag.profile_id)
        .unwrap()
        .into_iter()
        .find(|m| m.draft.content == text)
        .unwrap()
        .id
}
#[test]
fn preview_waits_for_apply_preserves_sources_unique_facts_and_history_and_rejects_stale_edits() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = Companion::open(dir.path()).unwrap();
    let session = app.session.snapshot();
    let id = session.profile_id;
    let a = fact(
        &mut app,
        "I really enjoy tea, especially green tea, every morning.",
        "detail",
    );
    let b = fact(
        &mut app,
        "I like drinking green tea every morning, without any sugar.",
        "essential",
    );
    let unique = fact(
        &mut app,
        "My cat is named Pixel and was born on 2020-02-03.",
        "detail",
    );
    let before = app.store.export_memories(id).unwrap();
    let history = serde_json::to_value(app.store.history(id, 100).unwrap()).unwrap();
    let group = MemoryMerge {
        ids: vec![a, b],
        content: "I enjoy green tea every morning, without sugar.".into(),
    };
    let guard = app.store.memory_guard(id).unwrap();
    let preview = app
        .store
        .optimization_preview(&guard, session.session_id, vec![group.clone()], None)
        .unwrap();
    assert_eq!(app.store.export_memories(id).unwrap(), before);
    assert!(preview.after_chars < preview.before_chars);
    app.store.apply_optimization(&preview).unwrap();
    let entries = app.store.optimization_memories(id).unwrap();
    assert_eq!(entries.len(), 2);
    let merged = entries.iter().find(|m| m.id == a).unwrap();
    assert_eq!(merged.draft.content, group.content);
    assert_eq!(merged.sources.len(), 2);
    assert_eq!(merged.draft.kind, "essential");
    assert!(merged.approved);
    assert!(
        entries
            .iter()
            .any(|m| m.id == unique && m.draft.content.contains("2020-02-03"))
    );
    assert_eq!(
        serde_json::to_value(app.store.history(id, 100).unwrap()).unwrap(),
        history
    );
    assert!(app.store.apply_optimization(&preview).is_err());
    let guard = app.store.memory_guard(id).unwrap();
    let pending = app
        .store
        .optimization_preview(
            &guard,
            session.session_id,
            vec![MemoryMerge {
                ids: vec![a],
                content: "Green tea daily, no sugar.".into(),
            }],
            None,
        )
        .unwrap();
    app.store
        .write_memory(
            &guard,
            None,
            &MemoryDraft {
                content: "Another unique fact".into(),
                category: "Test".into(),
                kind: "detail".into(),
            },
        )
        .unwrap();
    assert!(app.store.apply_optimization(&pending).is_err());
    assert_eq!(
        app.store
            .optimization_memories(id)
            .unwrap()
            .iter()
            .find(|m| m.id == a)
            .unwrap()
            .draft
            .content,
        group.content
    );
}
#[test]
fn invalid_ids_duplicate_ids_and_approval_promotion_never_change_memory() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = Companion::open(dir.path()).unwrap();
    let session = app.session.snapshot();
    let id = session.profile_id;
    let a = fact(
        &mut app,
        "A saved detailed fact about green tea",
        "essential",
    );
    let b = fact(&mut app, "Another lengthy fact about green tea", "detail");
    let db = rusqlite::Connection::open(
        dir.path()
            .join("profiles")
            .join(id.to_string())
            .join("profile.sqlite3"),
    )
    .unwrap();
    db.execute("UPDATE memories SET approved=0 WHERE id=?1", [b])
        .unwrap();
    drop(db);
    let before = app.store.export_memories(id).unwrap();
    let guard = app.store.memory_guard(id).unwrap();
    for ids in [vec![], vec![a, a], vec![999999], vec![a, b]] {
        assert!(
            app.store
                .optimization_preview(
                    &guard,
                    session.session_id,
                    vec![MemoryMerge {
                        ids,
                        content: "Green tea".into()
                    }],
                    None
                )
                .is_err()
        );
        assert_eq!(app.store.export_memories(id).unwrap(), before);
    }
    let duplicate = vec![
        MemoryMerge {
            ids: vec![a],
            content: "Tea".into(),
        },
        MemoryMerge {
            ids: vec![a],
            content: "Tea".into(),
        },
    ];
    assert!(
        app.store
            .optimization_preview(&guard, session.session_id, duplicate, None)
            .is_err()
    );
}
