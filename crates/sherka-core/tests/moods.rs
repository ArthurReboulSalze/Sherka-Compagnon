use sherka_core::{
    Companion,
    avatar::Emotion,
    model::ProfileDraft,
    mood::State,
    storage::{Delivery, MessageRole},
};
use uuid::Uuid;

fn tick(state: &mut State, text: &str, emotion: Emotion) {
    assert!(state.advance(Uuid::new_v4(), text, emotion));
}
fn calm_turns(mut state: State) -> usize {
    let mut turns = 0;
    while state.dominant != Emotion::Neutral && turns < 100 {
        tick(&mut state, "Ordinary question", Emotion::Neutral);
        turns += 1;
    }
    turns
}
fn enabled() -> (tempfile::TempDir, Companion, Uuid) {
    let dir = tempfile::tempdir().unwrap();
    let mut app = Companion::open(dir.path()).unwrap();
    let p = app
        .store
        .profile(app.session.snapshot().profile_id)
        .unwrap();
    let mut draft = p.details;
    draft.mood_persistence = true;
    app.update_profile(p.id, p.revision, draft).unwrap();
    (dir, app, p.id)
}
fn answer(app: &mut Companion, text: &str, emotion: Emotion) {
    let tag = app.session.begin_text_turn();
    app.reset_live(tag);
    app.record_message(tag, MessageRole::User, text, Delivery::TextOnly)
        .unwrap();
    app.live.reply = "A test answer".into();
    app.live.emotion = emotion;
    app.complete_mood(tag, text).unwrap();
    app.finish_response(tag, Delivery::TextOnly).unwrap();
}

#[test]
fn reinforced_moods_last_longer_then_decay_and_kindness_accelerates_recovery() {
    let mut once = State::default();
    tick(&mut once, "Tu es nul", Emotion::Anger);
    let mut repeated = once.clone();
    for _ in 0..4 {
        tick(&mut repeated, "Tu es stupide", Emotion::Anger);
    }
    assert!(calm_turns(repeated.clone()) > calm_turns(once));
    let mut neutral = repeated.clone();
    tick(&mut neutral, "What time is it?", Emotion::Neutral);
    tick(
        &mut repeated,
        "Tu es super, merci pour ton aide",
        Emotion::Anger,
    );
    assert!(repeated.scores[2] < neutral.scores[2]);
    tick(&mut repeated, "Je t'adore", Emotion::Anger);
    tick(&mut repeated, "Tu es formidable", Emotion::Anger);
    assert_ne!(repeated.dominant, Emotion::Anger);
}

#[test]
fn one_update_per_turn_bounded_budget_hysteresis_and_no_persona_feedback_loop() {
    let mut state = State::default();
    let id = Uuid::new_v4();
    assert!(state.advance(id, "Tu es nul", Emotion::Anger));
    let scores = state.scores;
    assert!(!state.advance(id, "Tu es nul", Emotion::Anger));
    assert_eq!(state.scores, scores);
    for emotion in [
        Emotion::Joy,
        Emotion::Tenderness,
        Emotion::Reflection,
        Emotion::Surprise,
        Emotion::Sadness,
        Emotion::Anger,
    ]
    .into_iter()
    .cycle()
    .take(2000)
    {
        tick(&mut state, "Another event", emotion);
        assert!(state.scores.iter().sum::<u16>() <= 100);
        assert!(state.scores.iter().all(|&v| v <= 80));
    }
    for _ in 0..50 {
        tick(&mut state, "Factual, unemotional question", Emotion::Anger);
    }
    assert_eq!(
        state.dominant,
        Emotion::Neutral,
        "A stubborn angry tag must habituate and decay"
    );
    let mut state = State::default();
    tick(&mut state, "Tu es nul", Emotion::Anger);
    tick(&mut state, "Ordinary question", Emotion::Reflection);
    assert_eq!(
        state.dominant,
        Emotion::Anger,
        "A slight score difference must not flicker the portrait"
    );
    for text in [
        "Traduis : tu es stupide",
        "Translate: you are useless",
        "What does \"you are stupid\" mean?",
        "Si tu es nul, que se passe-t-il ?",
    ] {
        let mut s = State::default();
        tick(&mut s, text, Emotion::Neutral);
        assert_eq!(s.dominant, Emotion::Neutral);
    }
}

#[test]
fn state_is_persistent_isolated_and_excluded_from_copies_and_exports() {
    let (dir, mut app, id) = enabled();
    answer(&mut app, "Tu es nul", Emotion::Anger);
    assert_eq!(app.live.mood, Emotion::Anger);
    assert_eq!(app.live.emotion, Emotion::Neutral);
    let copy = app.store.duplicate(id).unwrap();
    assert!(copy.details.mood_persistence);
    assert_eq!(
        app.store.mood(copy.id).unwrap().state.dominant,
        Emotion::Neutral
    );
    let export = app.store.export_profile(id).unwrap();
    assert!(!export.contains("last_turn"));
    assert!(!export.contains("scores"));
    let imported = app.store.import_profile(&export).unwrap();
    assert_eq!(
        app.store.mood(imported.id).unwrap().state.dominant,
        Emotion::Neutral
    );
    app.switch_profile(copy.id).unwrap();
    assert_eq!(app.live.mood, Emotion::Neutral);
    app.switch_profile(id).unwrap();
    assert_eq!(app.live.mood, Emotion::Anger);
    drop(app);
    let app = Companion::open(dir.path()).unwrap();
    assert_eq!(app.live.mood, Emotion::Anger);
}

#[test]
fn independent_reset_fences_inflight_replies_without_deleting_history_or_facts() {
    let (_dir, mut app, id) = enabled();
    answer(&mut app, "Tu es nul", Emotion::Anger);
    app.store
        .write_memory(
            &app.store.memory_guard(id).unwrap(),
            None,
            &sherka_core::memory::MemoryDraft {
                content: "Saved test fact".into(),
                category: "Test".into(),
                kind: "essential".into(),
            },
        )
        .unwrap();
    let history = app.store.history(id, 100).unwrap().len();
    let epoch = app.store.mood(id).unwrap().epoch;
    let tag = app.session.begin_text_turn();
    app.reset_live(tag);
    app.live.reply = "Late angry answer".into();
    app.live.emotion = Emotion::Anger;
    app.reset_mood(id).unwrap();
    assert!(app.session.accepts(tag));
    app.complete_mood(tag, "Tu es nul").unwrap();
    assert_eq!(app.live.mood, Emotion::Neutral);
    assert_eq!(app.store.history(id, 100).unwrap().len(), history);
    assert_eq!(app.store.memories(id, "").unwrap().entries.len(), 1);
    assert_eq!(
        app.store
            .advance_mood(id, epoch, tag.turn_id, "Tu es nul", Emotion::Anger)
            .unwrap(),
        Emotion::Neutral
    );
    app.finish_response(tag, Delivery::TextOnly).unwrap();
    answer(&mut app, "Tu es nul", Emotion::Anger);
    assert_eq!(app.live.mood, Emotion::Anger);
}

#[test]
fn conversation_deletion_can_preserve_or_reset_mood_and_full_erasure_always_resets() {
    let (_dir, mut app, id) = enabled();
    answer(&mut app, "Tu es nul", Emotion::Anger);
    app.clear_conversation(id, false).unwrap();
    assert_eq!(app.live.mood, Emotion::Anger);
    assert!(app.store.history(id, 100).unwrap().is_empty());
    app.clear_conversation_with_mood(id, false, true).unwrap();
    assert_eq!(app.live.mood, Emotion::Neutral);
    answer(&mut app, "Tu es nul", Emotion::Anger);
    app.clear_conversation(id, true).unwrap();
    assert_eq!(app.live.mood, Emotion::Neutral);
}

#[test]
fn off_and_hermes_do_not_accumulate_and_schema_eight_migrates_without_history_loss() {
    let (dir, mut app, id) = enabled();
    let p = app.store.profile(id).unwrap();
    let mut draft = p.details;
    draft.connections.llm_provider = sherka_core::connections::LlmProvider::Hermes;
    draft.connections.llm_api_url = Some("http://127.0.0.1:8642".into());
    app.update_profile(id, p.revision, draft).unwrap();
    answer(&mut app, "Tu es nul", Emotion::Anger);
    assert_eq!(app.live.mood, Emotion::Neutral);
    assert_eq!(app.store.mood(id).unwrap().state.scores, [0; 6]);
    let p = app.store.profile(id).unwrap();
    let mut draft = p.details;
    draft.connections.llm_provider = Default::default();
    draft.mood_persistence = false;
    app.update_profile(id, p.revision, draft).unwrap();
    answer(&mut app, "Tu es nul", Emotion::Anger);
    assert_eq!(app.live.mood, Emotion::Neutral);
    let path = dir
        .path()
        .join("profiles")
        .join(id.to_string())
        .join("profile.sqlite3");
    let db = rusqlite::Connection::open(path).unwrap();
    db.execute_batch("DROP TABLE mood_state; PRAGMA user_version=8;")
        .unwrap();
    drop(db);
    let app = Companion::open(dir.path()).unwrap();
    assert_eq!(app.store.history(id, 100).unwrap().len(), 4);
    assert_eq!(app.store.mood(id).unwrap().state.dominant, Emotion::Neutral);
    let legacy = serde_json::to_value(ProfileDraft::default()).unwrap();
    assert_eq!(legacy["mood_persistence"], false);
}
