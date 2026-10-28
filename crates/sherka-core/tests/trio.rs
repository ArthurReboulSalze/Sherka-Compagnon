use sherka_core::{
    Companion, Error,
    avatar::Emotion,
    connections::LlmProvider,
    model::ProfileDraft,
    session::{OperationTag, ResponseState},
    storage::{Delivery, Message, MessageRole, Speaker},
    window::Bounds,
};
use tempfile::{TempDir, tempdir};
use uuid::Uuid;

const NO_VOICE: &str = "Choisis d’abord une voix pour ce compagnon.";
const HERMES: &str = "Les compagnons Hermes ne participent pas aux conversations à trois.";

fn draft(name: &str, voice: Option<&str>) -> ProfileDraft {
    let mut draft = ProfileDraft {
        name: name.into(),
        ..Default::default()
    };
    draft.connections.tts_voice = voice.map(Into::into);
    draft
}

fn hermes(name: &str) -> ProfileDraft {
    let mut draft = draft(name, Some("voice"));
    draft.connections.llm_provider = LlmProvider::Hermes;
    draft.connections.llm_api_url = Some("http://127.0.0.1:8642".into());
    draft.connections.hermes_profile = Some("maman".into());
    draft
}

fn guest_of(app: &Companion, name: &str) -> Uuid {
    app.store
        .create(draft(name, Some("voice")), None)
        .unwrap()
        .id
}

fn setup() -> (TempDir, Companion, Uuid, Uuid) {
    let dir = tempdir().unwrap();
    let mut app = Companion::open(dir.path()).unwrap();
    let master = app.session.snapshot().profile_id;
    let guest = guest_of(&app, "Minerva");
    app.set_guest(Some(guest)).unwrap();
    (dir, app, master, guest)
}

fn message(error: Error) -> String {
    error.to_string()
}

fn name(app: &Companion, id: Uuid) -> String {
    app.store.profile(id).unwrap().details.name
}

fn speaker(app: &Companion, id: Uuid) -> Speaker {
    Speaker {
        id,
        name: name(app, id),
    }
}

fn history(app: &Companion, id: Uuid) -> Vec<Message> {
    app.store.history(id, 100).unwrap()
}

fn enable_moods(app: &mut Companion, id: Uuid) {
    let p = app.store.profile(id).unwrap();
    let mut draft = p.details;
    draft.mood_persistence = true;
    app.update_profile(id, p.revision, draft).unwrap();
}

/// The user speaks and the master answers; the turn stays open for the guest.
fn master_reply(app: &mut Companion, user: &str, reply: &str) -> OperationTag {
    let tag = app.session.begin_text_turn();
    app.reset_live(tag);
    app.record_user_message(tag, user, Delivery::TextOnly)
        .unwrap();
    app.live.reply = reply.into();
    app.live.heard = reply.into();
    app.complete_mood(tag, user).unwrap();
    app.finish_generation(tag, Delivery::Completed).unwrap();
    tag
}

#[test]
fn set_guest_is_validated() {
    let dir = tempdir().unwrap();
    let mut app = Companion::open(dir.path()).unwrap();
    let master = app.session.snapshot().profile_id;
    let silent = app.store.create(draft("Muette", None), None).unwrap().id;
    let blank = app
        .store
        .create(draft("Blanche", Some("  ")), None)
        .unwrap()
        .id;
    let robot = app.store.create(hermes("Agent"), None).unwrap().id;
    assert!(matches!(
        app.set_guest(Some(Uuid::new_v4())),
        Err(Error::ProfileNotFound)
    ));
    assert_eq!(
        message(app.set_guest(Some(master)).unwrap_err()),
        "Ce compagnon est déjà ton compagnon principal."
    );
    assert_eq!(message(app.set_guest(Some(silent)).unwrap_err()), NO_VOICE);
    assert_eq!(message(app.set_guest(Some(blank)).unwrap_err()), NO_VOICE);
    assert_eq!(message(app.set_guest(Some(robot)).unwrap_err()), HERMES);
    assert_eq!(app.guest(), None);
    assert!(app.participants().is_none());
    // A Hermes master cannot invite anyone either.
    let guest = guest_of(&app, "Minerva");
    let p = app.store.profile(master).unwrap();
    let mut master_draft = p.details.clone();
    master_draft.connections = hermes("x").connections;
    app.update_profile(master, p.revision, master_draft)
        .unwrap();
    assert_eq!(message(app.set_guest(Some(guest)).unwrap_err()), HERMES);
    assert_eq!(app.guest(), None);
}

#[test]
fn a_guest_is_session_only_and_the_last_three_are_remembered_per_master() {
    let (dir, mut app, master, guest) = setup();
    let snapshot = app.snapshot().unwrap();
    assert_eq!(snapshot.session.guest_profile_id, Some(guest));
    assert_eq!(snapshot.live.guest.unwrap().profile_id, guest);
    assert_eq!(snapshot.live.guest.unwrap().mood, Emotion::Neutral);
    let (m, g) = app.participants().unwrap();
    assert_eq!((m.id, g.id), (master, guest));
    assert_eq!(g.name, "Minerva");
    // Inviting the same guest again changes nothing.
    app.set_guest(Some(guest)).unwrap();
    let others: Vec<Uuid> = ["A", "B", "C"].map(|n| guest_of(&app, n)).to_vec();
    for id in &others {
        app.set_guest(Some(*id)).unwrap();
    }
    let recent = |app: &Companion| app.snapshot().unwrap().settings.recent_guests[&master].clone();
    assert_eq!(recent(&app), [others[2], others[1], others[0]]);
    app.set_guest(Some(others[0])).unwrap();
    assert_eq!(recent(&app), [others[0], others[2], others[1]]);
    app.set_guest(Some(guest)).unwrap();
    assert_eq!(recent(&app), [guest, others[0], others[2]]);
    app.set_guest(None).unwrap();
    app.set_guest(None).unwrap();
    let snapshot = app.snapshot().unwrap();
    assert_eq!(snapshot.session.guest_profile_id, None);
    assert!(snapshot.live.guest.is_none());
    assert_eq!(app.guest(), None);
    // The list survives a restart, the guest does not.
    app.set_guest(Some(guest)).unwrap();
    drop(app);
    let app = Companion::open(dir.path()).unwrap();
    assert_eq!(app.guest(), None);
    let snapshot = app.snapshot().unwrap();
    assert_eq!(snapshot.session.guest_profile_id, None);
    assert!(snapshot.live.guest.is_none());
    assert_eq!(snapshot.settings.recent_guests[&master][0], guest);
}

#[test]
fn both_companions_keep_the_whole_three_way_thread() {
    let (_dir, mut app, master, guest) = setup();
    let first = master_reply(&mut app, "Bonjour vous deux", "Salut !");
    assert!(app.live.tag.is_none());
    let second = app.session.begin_peer(first, guest).unwrap();
    assert!(!app.session.accepts(first));
    app.reset_live(second);
    app.live.reply = "Enchantée.".into();
    app.live.heard = "Enchantée.".into();
    app.complete_mood(second, "Bonjour vous deux").unwrap();
    app.finish_response(second, Delivery::Completed).unwrap();
    assert_eq!(app.session.snapshot().response, ResponseState::Idle);
    assert!(!app.session.accepts(second));

    let mine = history(&app, master);
    let theirs = history(&app, guest);
    assert_eq!(mine.len(), 3);
    assert_eq!(theirs.len(), 3);
    let roles = |h: &[Message]| h.iter().map(|m| m.role.clone()).collect::<Vec<_>>();
    let expected = [
        MessageRole::User,
        MessageRole::Assistant,
        MessageRole::Assistant,
    ];
    assert_eq!(roles(&mine), expected);
    assert_eq!(roles(&theirs), expected);
    let contents = |h: &[Message]| h.iter().map(|m| m.content.clone()).collect::<Vec<_>>();
    let thread = ["Bonjour vous deux", "Salut !", "Enchantée."];
    assert_eq!(contents(&mine), thread);
    assert_eq!(contents(&theirs), thread);
    assert!(mine[0].speaker.is_none() && mine[1].speaker.is_none());
    assert_eq!(mine[2].speaker, Some(speaker(&app, guest)));
    assert!(theirs[0].speaker.is_none() && theirs[2].speaker.is_none());
    assert_eq!(theirs[1].speaker, Some(speaker(&app, master)));
    assert_eq!(mine[1].id, first.generation_id.to_string());
    assert_eq!(mine[2].id, second.generation_id.to_string());
    assert!(
        mine.iter()
            .chain(&theirs)
            .skip(1)
            .all(|m| m.delivery == Delivery::Completed || m.role == MessageRole::User)
    );
    // The user message carries the turn on both sides.
    assert_eq!(app.store.history(master, 1).unwrap().len(), 1);
}

#[test]
fn a_barge_in_saves_the_heard_part_on_both_sides_and_not_the_finished_reply_again() {
    let (_dir, mut app, master, guest) = setup();
    let first = master_reply(&mut app, "Raconte", "Il était une fois.");
    let second = app.session.begin_peer(first, guest).unwrap();
    app.reset_live(second);
    app.live.reply = "Un roi vivait là-bas".into();
    app.live.heard = "Un roi".into();
    app.stop_response().unwrap();
    assert!(!app.session.accepts(second));
    assert_eq!(app.session.snapshot().response, ResponseState::Idle);

    let mine = history(&app, master);
    let theirs = history(&app, guest);
    assert_eq!((mine.len(), theirs.len()), (3, 3));
    assert_eq!(mine[1].delivery, Delivery::Completed);
    assert_eq!(mine[1].content, "Il était une fois.");
    assert_eq!(mine[2].delivery, Delivery::Interrupted);
    assert_eq!(mine[2].heard_content, "Un roi");
    assert_eq!(mine[2].speaker, Some(speaker(&app, guest)));
    assert_eq!(theirs[1].delivery, Delivery::Completed);
    assert_eq!(theirs[2].delivery, Delivery::Interrupted);
    assert_eq!(theirs[2].heard_content, "Un roi");
    assert_eq!(theirs[2].content, "Un roi vivait là-bas");
    assert!(theirs[2].speaker.is_none());

    // Interrupting the first speaker copies its partial reply to the guest.
    let tag = app.session.begin_text_turn();
    app.reset_live(tag);
    app.record_user_message(tag, "Encore", Delivery::TextOnly)
        .unwrap();
    app.live.reply = "Alors voilà".into();
    app.live.heard = "Alors".into();
    app.stop_response().unwrap();
    let theirs = history(&app, guest);
    assert_eq!(theirs.len(), 5);
    assert_eq!(theirs[4].heard_content, "Alors");
    assert_eq!(theirs[4].delivery, Delivery::Interrupted);
    assert_eq!(theirs[4].speaker, Some(speaker(&app, master)));
}

#[test]
fn a_second_speaker_who_passes_saves_nothing() {
    let (_dir, mut app, master, guest) = setup();
    let first = master_reply(&mut app, "Coucou", "Coucou toi !");
    let second = app.session.begin_peer(first, guest).unwrap();
    app.reset_live(second);
    app.live.reply = "[[pass]]".into();
    app.discard_generation(second).unwrap();
    assert!(!app.session.accepts(second));
    assert_eq!(app.session.snapshot().response, ResponseState::Idle);
    assert!(app.live.tag.is_none() && app.live.reply.is_empty());
    assert!(matches!(
        app.discard_generation(second),
        Err(Error::StaleOperation)
    ));
    assert_eq!(history(&app, master).len(), 2);
    let theirs = history(&app, guest);
    assert_eq!(theirs.len(), 2);
    assert_eq!(theirs[1].speaker, Some(speaker(&app, master)));
}

#[test]
fn each_companion_has_its_own_mood() {
    let (_dir, mut app, master, guest) = setup();
    enable_moods(&mut app, master);
    enable_moods(&mut app, guest);
    assert_eq!(app.guest(), Some(guest));
    let first = app.session.begin_text_turn();
    app.reset_live(first);
    app.record_user_message(first, "Quelle heure est-il ?", Delivery::TextOnly)
        .unwrap();
    app.live.reply = "Pardon.".into();
    app.complete_mood(first, "Quelle heure est-il ?").unwrap();
    app.finish_generation(first, Delivery::Completed).unwrap();
    assert_eq!(
        app.store.mood(guest).unwrap().state.dominant,
        Emotion::Neutral
    );
    let second = app.session.begin_peer(first, guest).unwrap();
    app.reset_live(second);
    app.live.reply = "Tu m'as vexée !".into();
    app.live.emotion = Emotion::Anger;
    app.complete_mood(second, "Quelle heure est-il ?").unwrap();
    app.finish_response(second, Delivery::Completed).unwrap();
    assert_eq!(app.live.guest.unwrap().mood, Emotion::Anger);
    assert_eq!(
        app.store.mood(guest).unwrap().state.dominant,
        Emotion::Anger
    );
    assert_eq!(app.live.mood, Emotion::Neutral);
    assert_eq!(
        app.store.mood(master).unwrap().state.dominant,
        Emotion::Neutral
    );
    // The mood survives the next turn's live reset, and a reset empties it.
    let next = app.session.begin_text_turn();
    app.reset_live(next);
    assert_eq!(app.live.guest.unwrap().mood, Emotion::Anger);
    app.reset_mood(guest).unwrap();
    assert_eq!(app.live.guest.unwrap().mood, Emotion::Neutral);
    assert_eq!(
        app.store.mood(guest).unwrap().state.dominant,
        Emotion::Neutral
    );
}

#[test]
fn the_guest_speaks_with_the_masters_model_and_its_own_voice() {
    let dir = tempdir().unwrap();
    let mut app = Companion::open(dir.path()).unwrap();
    let master = app.session.snapshot().profile_id;
    let p = app.store.profile(master).unwrap();
    let mut master_draft = p.details.clone();
    master_draft.connections.llm_model = Some("master-model".into());
    master_draft.connections.tts_voice = Some("master-voice".into());
    app.update_profile(master, p.revision, master_draft)
        .unwrap();
    let mut guest_draft = draft("Minerva", Some("guest-voice"));
    guest_draft.connections.llm_model = Some("guest-model".into());
    let guest = app.store.create(guest_draft, None).unwrap().id;
    // Before the invitation the guest is just a profile.
    let before = app.speaking_profile(guest).unwrap();
    assert_eq!(
        before.details.connections.llm_model.as_deref(),
        Some("guest-model")
    );
    app.set_guest(Some(guest)).unwrap();
    let guest_profile = app.speaking_profile(guest).unwrap();
    let c = &guest_profile.details.connections;
    assert_eq!(c.llm_model.as_deref(), Some("master-model"));
    assert_eq!(c.tts_voice.as_deref(), Some("guest-voice"));
    assert_eq!(guest_profile.details.name, "Minerva");
    let master_profile = app.speaking_profile(master).unwrap();
    assert_eq!(
        master_profile.details.connections.tts_voice.as_deref(),
        Some("master-voice")
    );
    assert_eq!(
        app.store
            .profile(guest)
            .unwrap()
            .details
            .connections
            .llm_model
            .as_deref(),
        Some("guest-model")
    );
}

#[test]
fn switching_deleting_or_breaking_the_guest_ends_the_trio() {
    let (_dir, mut app, master, guest) = setup();
    let other = guest_of(&app, "Autre");
    app.switch_profile(other).unwrap();
    assert_eq!(app.guest(), None);
    let snapshot = app.snapshot().unwrap();
    assert_eq!(snapshot.session.guest_profile_id, None);
    assert!(snapshot.live.guest.is_none());
    app.switch_profile(master).unwrap();

    app.set_guest(Some(guest)).unwrap();
    assert!(app.delete_profile(master).is_err());
    assert_eq!(app.guest(), Some(guest));
    app.delete_profile(guest).unwrap();
    assert_eq!(app.guest(), None);
    assert_eq!(app.snapshot().unwrap().session.guest_profile_id, None);
    assert!(app.store.profile(guest).is_err());
    assert!(!app.snapshot().unwrap().settings.recent_guests[&master].contains(&guest));

    // Taking the voice away, or turning someone into a Hermes agent, ends it.
    let guest = guest_of(&app, "Voix");
    app.set_guest(Some(guest)).unwrap();
    let p = app.store.profile(guest).unwrap();
    let mut silent = p.details.clone();
    silent.connections.tts_voice = None;
    app.update_profile(guest, p.revision, silent).unwrap();
    assert_eq!(app.guest(), None);
    app.set_guest(Some(other)).unwrap();
    let p = app.store.profile(master).unwrap();
    let mut agent = p.details.clone();
    agent.connections = hermes("x").connections;
    app.update_profile(master, p.revision, agent).unwrap();
    assert_eq!(app.guest(), None);
}

#[test]
fn clearing_a_conversation_keeps_the_guest_and_a_cleared_guest_stops_talking() {
    let (_dir, mut app, master, guest) = setup();
    app.clear_conversation(master, false).unwrap();
    assert_eq!(app.guest(), Some(guest));
    assert_eq!(
        app.snapshot().unwrap().session.guest_profile_id,
        Some(guest)
    );
    assert_eq!(app.live.guest.unwrap().profile_id, guest);
    let first = master_reply(&mut app, "Salut", "Hello");
    let second = app.session.begin_peer(first, guest).unwrap();
    app.reset_live(second);
    app.live.reply = "Bonjour".into();
    app.clear_conversation(guest, false).unwrap();
    assert!(!app.session.accepts(second));
    assert!(history(&app, guest).is_empty());
    assert_eq!(app.guest(), Some(guest));
    assert_eq!(history(&app, master).len(), 3);
}

#[test]
fn the_guest_window_position_is_stored_apart_from_the_masters() {
    let (_dir, mut app, master, guest) = setup();
    let bounds = Bounds {
        x: -300,
        y: 40,
        width: 420,
        height: 600,
    };
    assert!(app.guest_bounds(guest).is_none());
    let before = app.snapshot().unwrap().settings.widget_bounds;
    app.save_guest_bounds(guest, bounds).unwrap();
    assert!(matches!(
        app.save_guest_bounds(master, bounds),
        Err(Error::StaleOperation)
    ));
    assert_eq!(app.guest_bounds(guest), Some(bounds));
    let settings = app.snapshot().unwrap().settings;
    assert_eq!(settings.widget_bounds, before);
    assert_eq!(settings.desktop_layouts[&guest].bounds, Some(bounds));
    assert_ne!(settings.desktop_layouts[&master].bounds, Some(bounds));
    // Empty bounds are ignored, like for the master.
    app.save_guest_bounds(guest, Bounds { width: 0, ..bounds })
        .unwrap();
    assert_eq!(app.guest_bounds(guest), Some(bounds));
}

#[test]
fn web_search_mode_can_be_read_for_any_companion() {
    let (_dir, app, master, guest) = setup();
    assert_eq!(
        app.web_search_mode_for(guest).unwrap(),
        app.web_search_mode().unwrap()
    );
    assert_eq!(
        app.web_search_mode_for(master).unwrap(),
        app.web_search_mode().unwrap()
    );
    assert!(app.web_search_mode_for(Uuid::new_v4()).is_err());
}

#[test]
fn trio_settings_default_validate_and_persist() {
    use sherka_core::model::TrioSettings;
    let dir = tempdir().unwrap();
    let mut app = Companion::open(dir.path()).unwrap();
    assert_eq!(
        app.trio_settings(),
        TrioSettings {
            relay_seconds: 8,
            quick_replies: false,
            user_name: String::new()
        }
    );
    assert_eq!(app.snapshot().unwrap().settings.trio, app.trio_settings());
    let invalid = "Le délai de relance doit être désactivé ou compris entre 3 et 30 secondes.";
    for seconds in [1, 2, 31, 600] {
        let error = app
            .set_trio_settings(TrioSettings {
                relay_seconds: seconds,
                quick_replies: true,
                user_name: String::new(),
            })
            .unwrap_err();
        assert_eq!(message(error), invalid);
    }
    // A rejected change leaves the settings untouched.
    assert_eq!(app.trio_settings(), TrioSettings::default());
    for seconds in [0, 3, 30] {
        app.set_trio_settings(TrioSettings {
            relay_seconds: seconds,
            quick_replies: true,
            user_name: String::new(),
        })
        .unwrap();
    }
    drop(app);
    let reopened = Companion::open(dir.path()).unwrap();
    assert_eq!(
        reopened.trio_settings(),
        TrioSettings {
            relay_seconds: 30,
            quick_replies: true,
            user_name: String::new()
        }
    );
    // Stored settings written before the feature still load.
    let json = serde_json::to_value(TrioSettings::default()).unwrap();
    assert_eq!(
        json,
        serde_json::json!({"relay_seconds": 8, "quick_replies": false, "user_name": ""})
    );
    assert_eq!(
        serde_json::from_str::<TrioSettings>("{}").unwrap(),
        TrioSettings::default()
    );
    assert!(serde_json::from_str::<TrioSettings>(r#"{"other":1}"#).is_err());
}

#[test]
fn the_users_name_is_trimmed_and_cannot_break_the_line_label() {
    use sherka_core::model::TrioSettings;
    let dir = tempdir().unwrap();
    let mut app = Companion::open(dir.path()).unwrap();
    let with = |user_name: &str| TrioSettings {
        user_name: user_name.into(),
        ..Default::default()
    };
    app.set_trio_settings(with("  Arthur  ")).unwrap();
    assert_eq!(app.trio_settings().user_name, "Arthur");
    let invalid = "Ton prénom doit faire au plus 40 caractères, sans deux-points.";
    for name in ["Art: hur", "Ar\nthur", "Ar\tthur", &"a".repeat(41)] {
        assert_eq!(
            message(app.set_trio_settings(with(name)).unwrap_err()),
            invalid
        );
    }
    // A rejected name leaves the previous one in place; 40 characters pass.
    assert_eq!(app.trio_settings().user_name, "Arthur");
    app.set_trio_settings(with(&"é".repeat(40))).unwrap();
    assert_eq!(app.trio_settings().user_name.chars().count(), 40);
    // Blank means no name.
    app.set_trio_settings(with("   ")).unwrap();
    assert_eq!(app.trio_settings(), TrioSettings::default());
}

#[test]
fn the_relay_is_cancelled_by_every_change_of_the_conversation() {
    let (_dir, mut app, master, guest) = setup();
    // A new arm replaces the previous relay.
    let first = app.arm_relay();
    let second = app.arm_relay();
    assert!(first.is_cancelled() && !second.is_cancelled());
    app.cancel_relay();
    assert!(second.is_cancelled());

    let tag = app.session.begin_text_turn();
    let token = app.arm_relay();
    app.reset_live(tag);
    assert!(token.is_cancelled(), "a new turn");

    let token = app.arm_relay();
    app.stop_response().unwrap();
    assert!(token.is_cancelled(), "stop");

    let token = app.arm_relay();
    app.pause_voice().unwrap();
    assert!(token.is_cancelled(), "pause");

    let token = app.arm_relay();
    app.set_guest(None).unwrap();
    assert!(token.is_cancelled(), "guest removed");
    app.set_guest(Some(guest)).unwrap();

    let token = app.arm_relay();
    app.switch_profile(guest).unwrap();
    assert!(token.is_cancelled(), "profile switch");
    assert_eq!(app.session.snapshot().profile_id, guest);
    let _ = master;
}

#[test]
fn a_reply_produced_by_the_relay_does_not_cancel_the_relay_that_runs_it() {
    let (_dir, mut app, master, guest) = setup();
    let token = app.arm_relay();
    let turn = app.session.begin_text_turn();
    let tag = app.session.begin_peer(turn, guest).unwrap();
    app.reset_live_keeping_relay(tag);
    assert!(!token.is_cancelled());
    assert_eq!(app.live.tag, Some(tag));
    // The next speaker of the chain, in the same turn.
    let next = app.session.begin_peer(tag, master).unwrap();
    app.reset_live_keeping_relay(next);
    assert!(!token.is_cancelled());
    // The user still cuts it in every usual way.
    app.stop_response().unwrap();
    assert!(token.is_cancelled());
    let token = app.arm_relay();
    let tag = app.session.begin_text_turn();
    app.reset_live(tag);
    assert!(token.is_cancelled(), "a new turn of the user");
}

#[test]
fn the_answer_closes_the_turn_but_not_the_relay_prepared_during_it() {
    let (_dir, mut app, _master, guest) = setup();
    let tag = app.session.begin_text_turn();
    app.reset_live(tag);
    app.record_user_message(tag, "Salut", Delivery::TextOnly)
        .unwrap();
    // The relay is armed once the answerer is known, before it speaks.
    let relay = app.arm_relay();
    assert!(app.relay_active());
    app.live.reply = "Coucou !".into();
    app.live.heard = "Coucou !".into();
    app.finish_response(tag, Delivery::Completed).unwrap();
    // The turn is over and the session idle, so the user may speak ...
    assert_eq!(app.session.snapshot().response, ResponseState::Idle);
    assert!(!app.session.accepts(tag));
    // ... and the reaction prepared meanwhile is still wanted.
    assert!(!relay.is_cancelled() && app.relay_active());
    // The cooldown elapses: the reaction runs in a turn of its own and keeps the relay.
    let turn = app.session.begin_text_turn();
    let reaction = app.session.begin_peer(turn, guest).unwrap();
    app.reset_live_keeping_relay(reaction);
    assert!(!relay.is_cancelled() && app.live.tag == Some(reaction));
    // Anything the user does ends it.
    let user = app.session.begin_text_turn();
    app.reset_live(user);
    assert!(relay.is_cancelled() && !app.relay_active());
}

#[test]
fn only_a_turn_of_the_user_cancels_the_pending_memory_extraction() {
    let (_dir, mut app, _master, guest) = setup();
    let tag = app.session.begin_text_turn();
    app.reset_live(tag);
    let job = app.memory_job.clone();
    // The relay's replies, a chained speaker and a remark of the companions' own
    // are not the user: the extraction scheduled for the user's message lives.
    let turn = app.session.begin_text_turn();
    let reaction = app.session.begin_peer(turn, guest).unwrap();
    app.reset_live_keeping_relay(reaction);
    assert!(!job.is_cancelled() && !app.memory_job.is_cancelled());
    let remark = app.session.begin_text_turn();
    let relay = app.arm_relay();
    app.reset_live_for_event(remark);
    assert!(relay.is_cancelled() && !job.is_cancelled());
    // Stopping, or a new turn of the user (which schedules its own), cancels it.
    let user = app.session.begin_text_turn();
    app.reset_live(user);
    assert!(job.is_cancelled() && !app.memory_job.is_cancelled());
    let job = app.memory_job.clone();
    app.stop_response().unwrap();
    assert!(job.is_cancelled());
}
