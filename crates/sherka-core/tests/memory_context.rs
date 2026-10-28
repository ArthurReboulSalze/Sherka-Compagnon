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
fn explicit_forgetting_also_removes_unapproved_extracted_memories() {
    let dir = tempdir().unwrap();
    let mut app = Companion::open(dir.path()).unwrap();
    let p = app.session.snapshot().profile_id;
    let saved = app
        .store
        .write_memory(
            &app.store.memory_guard(p).unwrap(),
            None,
            &draft("Mon instrument favori est le piano.", "essential"),
        )
        .unwrap();
    for n in 0..6 {
        record(
            &mut app,
            &format!("Mon projet astronomie numéro {n} utilise une lunette."),
        );
    }
    let batch = app.store.extraction_batch(p).unwrap().unwrap();
    app.store
        .consolidate(
            &batch,
            Consolidation {
                memories: vec![Extract {
                    source_id: batch.sources[0].id.clone(),
                    quote: "utilise une lunette".into(),
                    category: "Astronomie".into(),
                    kind: "detail".into(),
                }],
                summary: vec![],
            },
        )
        .unwrap();
    let auto = app
        .store
        .memories(p, "")
        .unwrap()
        .entries
        .into_iter()
        .find(|m| !m.approved)
        .unwrap();
    let source = record(&mut app, "Peux-tu supprimer le souvenir sur ma lunette ?");
    let receipt = app
        .store
        .apply_memory_plan(
            &app.store.memory_guard(p).unwrap(),
            &source,
            std::slice::from_ref(&auto),
            MemoryPlan {
                actions: vec![MemoryAction {
                    action: "forget".into(),
                    id: Some(auto.id),
                    content: String::new(),
                    category: String::new(),
                    kind: String::new(),
                }],
            },
            "forget-auto",
        )
        .unwrap();
    assert!(receipt.contains("oublié"));
    let active = app.store.memories(p, "").unwrap().entries;
    assert!(active.iter().any(|m| m.id == saved));
    assert!(!active.iter().any(|m| m.id == auto.id));
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

fn row(
    n: usize,
    role: MessageRole,
    speaker: Option<&str>,
    content: &str,
) -> sherka_core::storage::Message {
    sherka_core::storage::Message {
        web: None,
        speaker: speaker.map(|name| sherka_core::storage::Speaker {
            id: uuid::Uuid::new_v4(),
            name: name.into(),
        }),
        id: n.to_string(),
        role,
        content: content.into(),
        delivery: Delivery::Completed,
        heard_content: content.into(),
        created_at: n as i64,
    }
}

fn minerva() -> ProfileDraft {
    ProfileDraft {
        name: "Minerva".into(),
        ..Default::default()
    }
}

#[test]
fn peer_rows_become_named_user_turns_and_adjacent_users_merge() {
    let p = minerva();
    let history = [
        row(1, MessageRole::User, None, "Q1"),
        row(2, MessageRole::Assistant, None, "A1"),
        row(3, MessageRole::Assistant, Some("Sherka"), "P1"),
        row(4, MessageRole::User, None, "Q2"),
    ];
    let prepared = prepare_context(
        &p,
        &history,
        "Q3",
        &Default::default(),
        budget(&p, &model(8192, 8192)).unwrap(),
        None,
    )
    .unwrap();
    let roles: Vec<_> = prepared.messages.iter().map(|m| m.role).collect();
    assert_eq!(roles, ["system", "user", "assistant", "user"]);
    // The departed companion's line makes the prompt multi-party: the user's
    // lines are labelled.
    assert_eq!(prepared.messages[1].content, "User: Q1");
    assert_eq!(prepared.messages[2].content, "A1");
    assert_eq!(
        prepared.messages[3].content,
        "Sherka: P1\n\nUser: Q2\n\nUser: Q3"
    );
    assert_eq!(prepared.budget.history_kept, 4);
    assert_eq!(prepared.budget.history_omitted, 0);
    // The prompt is readable in solo mode too, without a peer paragraph.
    assert!(!prepared.messages[0].content.contains("Three-way"));
}

#[test]
fn interrupted_peer_rows_keep_only_what_was_heard() {
    let p = minerva();
    let mut cut = row(2, MessageRole::Assistant, Some("Sherka"), "HEARD UNHEARD");
    cut.delivery = Delivery::Interrupted;
    cut.heard_content = "HEARD".into();
    let mut silent = row(3, MessageRole::Assistant, Some("Sherka"), "NEVERHEARD");
    silent.delivery = Delivery::Uncertain;
    silent.heard_content = String::new();
    let history = [row(1, MessageRole::User, None, "Q1"), cut, silent];
    let prepared = prepare_context(
        &p,
        &history,
        "Q2",
        &Default::default(),
        budget(&p, &model(8192, 8192)).unwrap(),
        None,
    )
    .unwrap();
    let json = serde_json::to_string(&prepared.messages).unwrap();
    assert!(!json.contains("UNHEARD") && !json.contains("NEVERHEARD"));
    assert_eq!(
        prepared.messages[1].content,
        "User: Q1\n\nSherka: HEARD [Suite interrompue, non entendue.]\n\nUser: Q2"
    );
}

#[test]
fn peer_paragraph_and_pass_rule_depend_on_the_turn_order() {
    use sherka_core::{
        conversation::prepare_context_with_avatar,
        trio::{Participant, Peer, follow_message},
    };
    let p = minerva();
    let run = |user: &str, peer: Option<&Peer>| {
        prepare_context_with_avatar(
            &p,
            &[row(1, MessageRole::User, None, "Salut")],
            user,
            &Default::default(),
            budget(&p, &model(8192, 8192)).unwrap(),
            None,
            None,
            false,
            peer,
            "",
            if peer.is_some_and(|p| p.second) {
                sherka_core::trio::Last::Attributed
            } else {
                sherka_core::trio::Last::User
            },
            None,
        )
        .unwrap()
    };
    let solo = run("Hello", None);
    assert!(!solo.messages[0].content.contains("Three-way"));
    assert!(!solo.messages[0].content.contains("[[pass]]"));
    let first = run(
        "Hello",
        Some(&Peer {
            name: "Sherka".into(),
            second: false,
            quick: false,
        }),
    );
    let system = &first.messages[0].content;
    assert!(system.contains("Three-way conversation"));
    assert!(system.contains("Speak only as Minerva"));
    assert!(system.contains("another companion named Sherka"));
    assert!(!system.contains("[[pass]]"));
    assert!(system.contains("Character: Minerva."));
    let sherka = Participant {
        id: uuid::Uuid::new_v4(),
        name: "Sherka".into(),
    };
    let follow = follow_message(Some("Hello"), "", &sherka, "Hi, I am Sherka.");
    let second = run(
        &follow,
        Some(&Peer {
            name: "Sherka".into(),
            second: true,
            quick: false,
        }),
    );
    assert!(second.messages[0].content.contains("reply only [[pass]]"));
    assert_eq!(second.messages.len(), 2);
    assert_eq!(
        second.messages[1].content,
        "User: Salut\n\nUser: Hello\n\nSherka: Hi, I am Sherka."
    );
    // The extra paragraph is mandatory text, so it is part of the estimate.
    assert!(second.budget.input_estimate > solo.budget.input_estimate);
}

#[test]
fn merged_three_way_prompts_stay_inside_the_budget() {
    use sherka_core::trio::Peer;
    let p = minerva();
    let mut history = Vec::new();
    for n in 0..60 {
        let who = match n % 3 {
            0 => (MessageRole::User, None),
            1 => (MessageRole::Assistant, None),
            _ => (MessageRole::Assistant, Some("Sherka")),
        };
        history.push(row(
            n,
            who.0,
            who.1,
            &format!("ligne {n} {}", "mot ".repeat(60)),
        ));
    }
    let budget = budget(&p, &model(8192, 8192)).unwrap();
    let limit = budget.limit as usize;
    let prepared = sherka_core::conversation::prepare_context_with_avatar(
        &p,
        &history,
        "Et toi ?",
        &Default::default(),
        budget,
        None,
        None,
        false,
        Some(&Peer {
            name: "Sherka".into(),
            second: true,
            quick: false,
        }),
        "",
        sherka_core::trio::Last::User,
        None,
    )
    .unwrap();
    let b = &prepared.budget;
    assert!(b.input_estimate + b.response_tokens as usize + b.safety_tokens <= limit);
    assert!(b.history_omitted > 0 && b.history_kept > 0);
    assert_eq!(prepared.messages[0].role, "system");
    assert!(
        prepared
            .messages
            .windows(2)
            .all(|w| w[0].role != w[1].role || w[0].role == "system")
    );
    assert_eq!(prepared.messages[1].role, "user");
    assert!(
        prepared
            .messages
            .last()
            .unwrap()
            .content
            .ends_with("Et toi ?")
    );
}

#[test]
fn a_departed_guest_is_explained_to_the_master() {
    use sherka_core::{conversation::prepare_context_with_avatar, trio::Peer};
    let p = minerva();
    let history = [
        row(1, MessageRole::User, None, "Q1"),
        row(2, MessageRole::Assistant, Some("Kit"), "Salut, c'est Kit."),
        row(3, MessageRole::Assistant, None, "Et moi, Minerva."),
    ];
    let run = |history: &[sherka_core::storage::Message], peer: Option<&Peer>| {
        prepare_context_with_avatar(
            &p,
            history,
            "Q2",
            &Default::default(),
            budget(&p, &model(8192, 8192)).unwrap(),
            None,
            None,
            false,
            peer,
            "",
            sherka_core::trio::Last::User,
            None,
        )
        .unwrap()
        .messages[0]
            .content
            .clone()
    };
    // Kit left: no peer, but his lines remain.
    let system = run(&history, None);
    assert!(system.contains("Earlier in this conversation, Kit (another companion) took part"));
    assert!(system.contains("Kit has left the conversation and can no longer hear you."));
    assert!(system.contains("not by the user"));
    assert!(!system.contains("Three-way conversation"));
    // Nobody ever spoke: nothing to explain.
    let alone = run(&history[..1], None);
    assert!(!alone.contains("Earlier in this conversation"));
    // Kit is the current peer: not departed.
    let peer = |name: &str| Peer {
        name: name.into(),
        second: false,
        quick: false,
    };
    let current = run(&history, Some(&peer("Kit")));
    assert!(current.contains("Three-way conversation"));
    assert!(!current.contains("Earlier in this conversation"));
    assert!(!run(&history, Some(&peer(" kit "))).contains("Earlier in this conversation"));
    // A new guest joined while the old one's lines remain.
    let replaced = run(&history, Some(&peer("Sherka")));
    assert!(replaced.contains("another companion named Sherka"));
    assert!(replaced.contains("Earlier in this conversation, Kit (another companion)"));
    // Several former guests are listed once each.
    let mut crowd = history.to_vec();
    crowd.push(row(4, MessageRole::Assistant, Some("Alba"), "Coucou."));
    crowd.push(row(5, MessageRole::Assistant, Some("Kit"), "Encore moi."));
    crowd.push(row(6, MessageRole::Assistant, Some("Botsy"), "Bip."));
    let system = run(&crowd, None);
    assert!(system.contains("Kit, Alba and Botsy (other companions) took part"));
    assert!(system.contains("those names were said by them"));
    assert!(system.contains("They have left the conversation"));
    assert_eq!(system.matches("Kit").count(), 1);
}

#[test]
fn the_users_lines_are_labelled_whenever_another_companion_is_in_the_prompt() {
    use sherka_core::{
        conversation::prepare_context_with_avatar,
        trio::{Last, Peer},
    };
    let p = minerva();
    let run = |history: &[sherka_core::storage::Message],
               user: &str,
               peer: Option<&Peer>,
               user_name: &str,
               last: Last| {
        prepare_context_with_avatar(
            &p,
            history,
            user,
            &Default::default(),
            budget(&p, &model(8192, 8192)).unwrap(),
            None,
            None,
            false,
            peer,
            user_name,
            last,
            None,
        )
        .unwrap()
    };
    let sherka = Peer {
        name: "Sherka".into(),
        second: false,
        quick: false,
    };
    let history = [
        row(1, MessageRole::User, None, "Salut"),
        row(
            2,
            MessageRole::Assistant,
            Some("Sherka"),
            "Coucou, moi c'est Sherka.",
        ),
        row(3, MessageRole::User, None, "Stop, écoute-moi."),
        row(4, MessageRole::Assistant, None, "Je t'écoute."),
    ];
    // Trio: the user's words stay apart from the other companion's line.
    let trio = run(&history, "Et toi ?", Some(&sherka), "", Last::User);
    assert_eq!(trio.messages.len(), 4);
    assert_eq!(
        trio.messages[1].content,
        "User: Salut\n\nSherka: Coucou, moi c'est Sherka.\n\nUser: Stop, écoute-moi."
    );
    assert!(trio.messages[1].content.contains("\n\nUser: "));
    assert_eq!(trio.messages[2].content, "Je t'écoute.");
    assert_eq!(trio.messages[3].content, "User: Et toi ?");
    assert!(trio.messages[0].content.contains("\"User:\""));
    // The label costs context: the estimate follows.
    let bare = run(&history, "Et toi ?", None, "", Last::User);
    assert!(trio.budget.input_estimate > bare.budget.input_estimate);
    // A named user.
    let named = run(&history, "Et toi ?", Some(&sherka), " Arthur ", Last::User);
    assert_eq!(
        named.messages[1].content,
        "Arthur: Salut\n\nSherka: Coucou, moi c'est Sherka.\n\nArthur: Stop, écoute-moi."
    );
    assert_eq!(named.messages[3].content, "Arthur: Et toi ?");
    assert!(named.messages[0].content.contains("\"Arthur:\""));
    assert!(!named.messages[0].content.contains("User:"));
    // The second speaker's follow message already carries the label.
    let second = Peer {
        second: true,
        ..sherka.clone()
    };
    let who = sherka_core::trio::Participant {
        id: uuid::Uuid::new_v4(),
        name: "Sherka".into(),
    };
    let follow = sherka_core::trio::follow_message(Some("Et toi ?"), "", &who, "Bonjour.");
    let reacting = run(&history[..1], &follow, Some(&second), "", Last::Attributed);
    assert_eq!(
        reacting.messages[1].content,
        "User: Salut\n\nUser: Et toi ?\n\nSherka: Bonjour."
    );
    // Departed companion only: the user's lines are labelled all the same.
    let departed = run(&history, "Et toi ?", None, "", Last::User);
    assert_eq!(departed.messages[3].content, "User: Et toi ?");
    assert!(
        departed.messages[1]
            .content
            .starts_with("User: Salut\n\nSherka: ")
    );
    let note = &departed.messages[0].content;
    assert!(note.contains("Earlier in this conversation, Sherka (another companion) took part"));
    assert!(note.contains("The user's lines start with \"User:\"."));
    assert!(!note.contains("Three-way conversation"));
    let named = run(&history, "Et toi ?", None, "Arthur", Last::User);
    assert!(
        named.messages[0]
            .content
            .contains("The user's lines start with \"Arthur:\".")
    );
    assert_eq!(named.messages[3].content, "Arthur: Et toi ?");
    // With a peer present, its paragraph says it; the note does not repeat it.
    assert!(
        !trio.messages[0]
            .content
            .contains("The user's lines start with")
    );
    // An application event is not a line of the user.
    let event = sherka_core::event::AppEvent::GuestJoined {
        id: uuid::Uuid::new_v4(),
        name: "Sherka".into(),
        description: None,
    }
    .prompt();
    let remark = run(&history[..1], &event, Some(&sherka), "", Last::Attributed);
    let last = &remark.messages.last().unwrap().content;
    assert!(last.ends_with(&event) && !last.contains("User: [Application"));
    assert!(remark.messages[1].content.starts_with("User: Salut"));
    // Solo, with no other companion anywhere: nothing is labelled.
    let solo_history = [
        row(1, MessageRole::User, None, "Salut"),
        row(2, MessageRole::Assistant, None, "Bonjour."),
    ];
    let solo = run(&solo_history, "Et toi ?", None, "Arthur", Last::User);
    assert_eq!(solo.messages.len(), 4);
    assert_eq!(solo.messages[1].content, "Salut");
    assert_eq!(solo.messages[3].content, "Et toi ?");
    assert!(
        solo.messages
            .iter()
            .all(|m| !m.content.contains("User:") && !m.content.contains("Arthur:"))
    );
    assert!(!solo.messages[0].content.contains("lines start with"));
}
