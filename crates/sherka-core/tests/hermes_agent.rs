use sherka_core::{
    Companion,
    connections::{Connections, LlmProvider, api_base_url},
    hermes::{profile_name, server_url},
    storage::{Delivery, MessageRole},
};
use tempfile::tempdir;

#[test]
fn hermes_addresses_and_profile_routes_are_scoped() {
    let config = Connections {
        llm_provider: LlmProvider::Hermes,
        llm_api_url: Some("http://192.168.1.50:8642/v1/".into()),
        hermes_profile: Some("maman".into()),
        ..Default::default()
    }
    .validated()
    .unwrap();
    assert_eq!(
        config.api_base().unwrap(),
        "http://192.168.1.50:8642/p/maman"
    );
    assert!(api_base_url("http://192.168.1.50:8642").is_err());
    for value in [
        "http://example.org:8642",
        "http://8.8.8.8",
        "https://x/p/maman",
        "http://user:password@localhost",
        "http://127.0.0.1?key=secret",
    ] {
        assert!(server_url(value).is_err(), "{value}");
    }
    for value in ["../maman", "maman/other", "a?key=b"] {
        assert!(profile_name(value).is_err());
    }
    assert!(server_url("https://hermes.example.org/service").is_ok());
}

#[test]
fn hermes_new_isolated_persistent_and_excluded_from_emotions() {
    let dir = tempdir().unwrap();
    let mut app = Companion::open(dir.path()).unwrap();
    let id = app.session.snapshot().profile_id;
    assert!(
        app.snapshot()
            .unwrap()
            .companion_capabilities
            .local_emotions
    );
    let p = app.store.profile(id).unwrap();
    let mut config = p.details.connections.clone();
    config.llm_provider = LlmProvider::Hermes;
    config.llm_api_url = Some("http://127.0.0.1:8642".into());
    config.hermes_profile = Some("maman".into());
    let mut draft = p.details;
    draft.connections = config.clone();
    app.update_profile(id, p.revision, draft).unwrap();
    let flags = app.snapshot().unwrap().companion_capabilities;
    assert!(!flags.local_emotions && !flags.local_memory && !flags.local_personality);
    let endpoint = config.api_base().unwrap();
    let first = app.store.hermes_session(id, &endpoint).unwrap();
    let tag = app.session.begin_text_turn();
    app.reset_live(tag);
    app.record_message(
        tag,
        MessageRole::User,
        "private previous conversation",
        Delivery::TextOnly,
    )
    .unwrap();
    assert_eq!(app.store.history(id, 100).unwrap().len(), 1);
    assert!(app.store.context_history(id).unwrap().is_empty());
    app.store
        .update_hermes_session_tip(&first, "compressed_remote_session")
        .unwrap();
    assert_eq!(
        app.store.hermes_session_tip(&first).unwrap(),
        "compressed_remote_session"
    );
    assert_eq!(app.store.history(id, 100).unwrap().len(), 1);

    let other = app
        .store
        .hermes_session(id, "http://127.0.0.1:8642")
        .unwrap();
    assert_ne!(first, other);
    let export = app.store.export_profile(id).unwrap();
    assert!(!export.contains(&first));
    assert!(!export.contains("private previous conversation"));
    app.stop_response().unwrap();
    drop(app);
    let app = Companion::open(dir.path()).unwrap();
    assert_eq!(app.store.hermes_session(id, &endpoint).unwrap(), first);
    app.store.new_hermes_session(id).unwrap();
    assert_ne!(app.store.hermes_session(id, &endpoint).unwrap(), first);
    assert!(app.store.history(id, 100).unwrap().is_empty());
    assert_eq!(
        app.store
            .hermes_session(id, "http://127.0.0.1:8642")
            .unwrap(),
        other
    );
    // New conversation hides the old local transcript, without deleting it.
    assert_eq!(app.store.profile(id).unwrap().message_count, 1);
}
