use sherka_core::{
    Companion,
    catalogs::LanguageModel,
    conversation::{budget, prepare_context},
    memory::{Consolidation, Extract, MemoryAction, MemoryDraft, MemoryPlan},
    model::ProfileDraft,
    storage::{Delivery, MessageRole},
};
use tempfile::tempdir;

fn model(max: u64, loaded: u64) -> LanguageModel {
    LanguageModel {
        vision: None,
        api: None,
        id: "loaded-alias".into(),
        key: "model-key".into(),
        label: "Test".into(),
        loaded: Some(true),
        quantization: None,
        max_context_length: Some(max),
        loaded_context_length: Some(loaded),
        reasoning: None,
    }
}
fn draft(content: &str, kind: &str) -> MemoryDraft {
    MemoryDraft {
        content: content.into(),
        category: "Astronomie".into(),
        kind: kind.into(),
    }
}
fn record(app: &mut Companion, text: &str) -> sherka_core::memory::Source {
    let tag = app.session.begin_text_turn();
    app.reset_live(tag);
    app.record_message(tag, MessageRole::User, text, Delivery::TextOnly)
        .unwrap();
    app.store
        .source_for_turn(tag.profile_id, tag.turn_id)
        .unwrap()
}
#[test]
fn budget_uses_loaded_context_and_explicit_response_reserve() {
    let p = ProfileDraft::default();
    let b = budget(&p, &model(262144, 8192)).unwrap();
    assert_eq!(b.limit, 8192);
    assert_eq!(p.connections.response_tokens, 8192);
    assert_eq!(b.response_tokens, 2048); // The small loaded context still constrains the reserve.
    let small = prepare_context(&p, &[], "Bonjour", &Default::default(), b, None).unwrap();
    assert!(
        small.messages[0]
            .content
            .contains("without reciting your identity")
    );
    assert!(
        small.budget.input_estimate
            + small.budget.response_tokens as usize
            + small.budget.safety_tokens
            <= 8192
    );
    let text = "Lune ".repeat(5000);
    assert!(
        prepare_context(
            &p,
            &[],
            &text,
            &Default::default(),
            budget(&p, &model(8192, 8192)).unwrap(),
            None
        )
        .is_err()
    );
    let large = prepare_context(
        &p,
        &[],
        &text,
        &Default::default(),
        budget(&p, &model(262144, 200000)).unwrap(),
        None,
    )
    .unwrap();
    assert_eq!(large.messages.last().unwrap().content, text.trim());
    assert_eq!(large.budget.limit, 200000);
    let mut capped = p.clone();
    capped.connections.context_limit = Some(16000);
    assert_eq!(
        budget(&capped, &model(262144, 200000)).unwrap().limit,
        16000
    );
}
#[test]
fn remote_context_fallback_and_known_limits_keep_the_default_reply_budget_usable() {
    let mut p = ProfileDraft::default();
    p.connections.llm_provider = sherka_core::connections::LlmProvider::OpenRouter;
    let mut m = model(262144, 8192);
    m.max_context_length = None;
    m.loaded_context_length = None;
    let b = budget(&p, &m).unwrap();
    assert_eq!(b.limit, 64_000);
    assert_eq!(b.max_context, None); // A configured fallback is not provider metadata.
    assert_eq!(b.response_tokens, 8192);
    p.connections.context_limit = Some(100_000);
    assert_eq!(budget(&p, &m).unwrap().limit, 100_000);
    m.max_context_length = Some(32_768);
    assert_eq!(budget(&p, &m).unwrap().limit, 32_768);
    p.connections.context_limit = Some(16_000);
    assert_eq!(budget(&p, &m).unwrap().limit, 16_000);
}

#[test]
fn legacy_reply_default_is_upgraded_once_without_changing_history_or_other_choices() {
    for old_reserve in [1024, 3072] {
        let dir = tempdir().unwrap();
        let mut app = Companion::open(dir.path()).unwrap();
        let id = app.session.snapshot().profile_id;
        let old = app.store.profile(id).unwrap();
        let mut draft = old.details;
        draft.connections.response_tokens = old_reserve;
        draft.connections.llm_model = Some("chosen-model".into());
        draft.connections.tts_voice = Some("chosen-voice".into());
        let old = app.store.update(id, old.revision, draft).unwrap();
        record(&mut app, "Aujourd'hui j'ai fait une promenade.");
        app.store
            .write_memory(
                &app.store.memory_guard(id).unwrap(),
                None,
                &self::draft("J'aime les promenades.", "essential"),
            )
            .unwrap();
        let history = serde_json::to_value(app.store.history(id, 100).unwrap()).unwrap();
        let memories = app.store.export_memories(id).unwrap();
        let path = dir
            .path()
            .join("profiles")
            .join(id.to_string())
            .join("profile.sqlite3");
        drop(app);
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch("PRAGMA user_version = 4;").unwrap();
        drop(db);
        let app = Companion::open(dir.path()).unwrap();
        let upgraded = app.store.profile(id).unwrap();
        let mut expected = old.details;
        if old_reserve == 1024 {
            expected.connections.response_tokens = 8192;
        }
        assert_eq!(
            serde_json::to_value(&upgraded.details).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
        assert_eq!(
            upgraded.revision,
            old.revision + i64::from(old_reserve == 1024)
        );
        assert_eq!(
            serde_json::to_value(app.store.history(id, 100).unwrap()).unwrap(),
            history
        );
        assert_eq!(app.store.export_memories(id).unwrap(), memories);
        let mut edited = upgraded.details;
        edited.connections.response_tokens = 1024;
        app.store.update(id, upgraded.revision, edited).unwrap();
        drop(app);
        assert_eq!(
            Companion::open(dir.path())
                .unwrap()
                .store
                .profile(id)
                .unwrap()
                .details
                .connections
                .response_tokens,
            1024
        );
    }
}
#[test]
fn memory_is_profile_local_searchable_editable_and_forget_cannot_rehydrate_from_archives() {
    let dir = tempdir().unwrap();
    let mut app = Companion::open(dir.path()).unwrap();
    let a = app.session.snapshot().profile_id;
    let b = app.store.duplicate(a).unwrap().id;
    record(&mut app, "Retiens que mon télescope est un Newton.");
    let id = app
        .store
        .write_memory(
            &app.store.memory_guard(a).unwrap(),
            None,
            &draft("Mon télescope est un Newton.", "detail"),
        )
        .unwrap();
    let keep = app
        .store
        .write_memory(
            &app.store.memory_guard(a).unwrap(),
            None,
            &draft("Je préfère le tutoiement.", "essential"),
        )
        .unwrap();
    assert_eq!(
        app.store.recall(a, "Quel télescope ?").unwrap().details[0].id,
        id
    );
    assert!(app.store.recall(a, "Bonjour").unwrap().details.is_empty());
    assert_eq!(
        app.store.recall(a, "Bonjour").unwrap().essential[0].id,
        keep
    );
    assert!(app.store.memories(b, "").unwrap().entries.is_empty());
    let corrected = app
        .store
        .write_memory(
            &app.store.memory_guard(a).unwrap(),
            Some(id),
            &draft("Mon télescope est une lunette.", "detail"),
        )
        .unwrap();
    assert!(app.store.context_history(a).unwrap().is_empty());
    assert!(!app.store.history(a, 100).unwrap().is_empty());
    assert!(
        app.store.search_archive(a, "Newton").unwrap()[0]
            .content
            .contains("Newton")
    );
    assert!(app.store.memories(a, "Newton").unwrap().entries.is_empty());
    assert!(app.store.memories(a, "lunette").unwrap().entries[0].approved);
    app.store
        .forget_memory(&app.store.memory_guard(a).unwrap(), corrected)
        .unwrap();
    assert!(app.store.recall(a, "lunette").unwrap().details.is_empty());
    assert_eq!(app.store.memories(a, "").unwrap().entries.len(), 1);
    let export: serde_json::Value =
        serde_json::from_str(&app.store.export_memories(a).unwrap()).unwrap();
    assert_eq!(export["format"], "sherka-memory");
    assert!(
        export["memories"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["status"] == "deleted")
    );
}
#[test]
fn extraction_requires_quotes_user_sources_and_review_and_rejects_stale_jobs() {
    let dir = tempdir().unwrap();
    let mut app = Companion::open(dir.path()).unwrap();
    let id = app.session.snapshot().profile_id;
    for n in 0..6 {
        record(
            &mut app,
            &format!("Mon projet astronomie numéro {n} utilise une lunette."),
        );
    }
    let batch = app.store.extraction_batch(id).unwrap().unwrap();
    let quote = |text: &str| Extract {
        source_id: batch.sources[0].id.clone(),
        quote: text.into(),
        category: "Astronomie".into(),
        kind: "detail".into(),
    };
    assert!(
        app.store
            .consolidate(
                &batch,
                Consolidation {
                    memories: vec![quote("invention")],
                    summary: vec![]
                }
            )
            .is_err()
    );
    assert!(app.store.memories(id, "").unwrap().entries.is_empty());
    app.store
        .consolidate(
            &batch,
            Consolidation {
                memories: vec![quote("utilise une lunette")],
                summary: vec![quote("projet astronomie numéro 0")],
            },
        )
        .unwrap();
    let state = app.store.memories(id, "").unwrap();
    assert_eq!(state.entries.len(), 1);
    assert!(!state.entries[0].approved);
    assert!(app.store.recall(id, "lunette").unwrap().details.is_empty());
    assert_eq!(state.entries[0].sources[0].id, batch.sources[0].id);
    app.store
        .approve_memory(&app.store.memory_guard(id).unwrap(), state.entries[0].id)
        .unwrap();
    assert_eq!(app.store.recall(id, "lunette").unwrap().details.len(), 1);
    app.store
        .forget_memory(&app.store.memory_guard(id).unwrap(), state.entries[0].id)
        .unwrap();
    assert!(app.store.memories(id, "").unwrap().summary.is_empty());
    assert!(
        app.store
            .consolidate(
                &batch,
                Consolidation {
                    memories: vec![quote("utilise une lunette")],
                    summary: vec![]
                }
            )
            .is_err()
    );
    assert!(app.store.extraction_batch(id).unwrap().is_none());
}
#[test]
fn model_correction_is_atomic_proven_and_only_targets_visible_memories() {
    let dir = tempdir().unwrap();
    let mut app = Companion::open(dir.path()).unwrap();
    let id = app.session.snapshot().profile_id;
    let original = app
        .store
        .write_memory(
            &app.store.memory_guard(id).unwrap(),
            None,
            &draft("Mon chat s'appelle Milo.", "essential"),
        )
        .unwrap();
    let source = record(
        &mut app,
        "Corrige ta mémoire : mon chat s'appelle Miko, pas Milo.",
    );
    let visible = app.store.memories(id, "").unwrap().entries;
    let guard = app.store.memory_guard(id).unwrap();
    let action = |id| MemoryAction {
        action: "update".into(),
        id: Some(id),
        content: "Mon chat s'appelle Miko.".into(),
        category: "Animaux".into(),
        kind: "essential".into(),
    };
    assert!(
        app.store
            .apply_memory_plan(
                &guard,
                &source,
                &visible,
                MemoryPlan {
                    actions: vec![action(9999)]
                },
                "invalid"
            )
            .is_err()
    );
    assert_eq!(app.store.memories(id, "").unwrap().entries[0].id, original);
    let receipt = app
        .store
        .apply_memory_plan(
            &guard,
            &source,
            &visible,
            MemoryPlan {
                actions: vec![action(original)],
            },
            "valid",
        )
        .unwrap();
    assert!(receipt.contains("corrigé"));
    let state = app.store.memories(id, "").unwrap();
    assert_eq!(state.entries[0].draft.content, "Mon chat s'appelle Miko.");
    assert_eq!(state.entries[0].sources[0].id, source.id);
    assert!(
        app.store
            .apply_memory_plan(
                &guard,
                &source,
                &visible,
                MemoryPlan {
                    actions: vec![action(original)]
                },
                "valid"
            )
            .is_err()
    );
    assert_eq!(app.store.memories(id, "").unwrap().entries.len(), 1);
}
#[test]
fn old_profile_revisions_and_unloaded_contexts_are_rejected() {
    let dir = tempdir().unwrap();
    let app = Companion::open(dir.path()).unwrap();
    let p = app.store.list().unwrap().remove(0);
    let guard = app.store.memory_guard(p.id).unwrap();
    let mut changed = p.details;
    changed.name = "Autre".into();
    app.store.update(p.id, p.revision, changed).unwrap();
    assert!(
        app.store
            .write_memory(&guard, None, &draft("Ancien résultat", "essential"))
            .is_err()
    );
    let mut m = model(262144, 8192);
    m.loaded_context_length = None;
    assert!(budget(&ProfileDraft::default(), &m).is_err());
}
#[test]
fn migration_v2_keeps_messages_and_memory_provenance() {
    let dir = tempdir().unwrap();
    let app = Companion::open(dir.path()).unwrap();
    let p = app.store.list().unwrap().remove(0);
    let path = dir
        .path()
        .join("profiles")
        .join(p.id.to_string())
        .join("profile.sqlite3");
    let db = rusqlite::Connection::open(path).unwrap();
    // Reconstruct the previous shipped schema, not a version number alone.
    db.execute_batch("DROP TRIGGER message_insert; DROP TRIGGER message_update; DROP TRIGGER message_delete; DROP TABLE messages_fts; DROP TABLE summary_sources; DROP TABLE memory_sources; DROP TABLE memory_meta; ALTER TABLE memories DROP COLUMN kind; ALTER TABLE memories DROP COLUMN approved; ALTER TABLE memories DROP COLUMN origin; ALTER TABLE messages DROP COLUMN web_sources; PRAGMA user_version=2;").unwrap();
    db.execute("INSERT INTO memories(content,category,status,created_at,updated_at,operation_key) VALUES('astronomie','projet','active',1,1,'v2')",[]).unwrap();
    drop(db);
    let memory = app.store.memories(p.id, "").unwrap();
    assert_eq!(memory.entries[0].draft.content, "astronomie");
    assert_eq!(memory.entries[0].draft.kind, "detail");
}
