//! Application events a companion reacts to with one short spoken remark: the
//! idle invitation, a guest joining. The remark is a normal assistant reply;
//! no user message is recorded and no memory is scheduled.
use crate::{Companion, Result, storage::Message};
use uuid::Uuid;

/// Longest description of a guest given to the master, in characters.
pub const DESCRIPTION_CHARS: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppEvent {
    /// The initiative timer fired: invite the user to chat.
    Idle,
    /// The user added `name` as the guest companion. `description` is `None`
    /// when the master already knows them.
    GuestJoined {
        id: Uuid,
        name: String,
        description: Option<String>,
    },
}

impl AppEvent {
    /// Stand-in for the user's message in logs and mood: never stored.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Idle => "Idle invitation",
            Self::GuestJoined { .. } => "Guest joined",
        }
    }
    /// In a three-way conversation, only the master reacts to this event.
    pub fn master_only(&self) -> bool {
        matches!(self, Self::GuestJoined { .. })
    }
    /// After this event's remark the companions wait for the user like after a
    /// user turn: the silence relay starts, with the remark as the last line.
    pub fn arms_relay(&self) -> bool {
        matches!(self, Self::GuestJoined { .. })
    }
    /// The final user-role instruction the reacting companion receives.
    pub fn prompt(&self) -> String {
        // i18n: data-start
        match self {
            Self::Idle => "[Application idle event, not a user message] Take the initiative with one brief, natural invitation to chat in your character's language. At most two short sentences and one question. You may refer to an established topic, but do not invent recent events, user activity or new facts. Do not mention this timer or repeat your identity. No tools, web search or memory changes. Wait for a reply afterwards.".into(),
            Self::GuestJoined { name, description, .. } => {
                let known = description
                    .as_deref()
                    .map(|d| format!(" What you know about {name}: {d}"))
                    .unwrap_or_default();
                format!(
                    "[Application event, not a user message] {name} has just joined the conversation at the user's request.{known} React with one brief, natural, in-character remark about {name} joining, in your character's language: at most two short sentences. You may greet or tease them according to your personality and what you know of them. Do not describe this event technically, do not mention the application, and do not speak for {name}."
                )
            }
        }
        // i18n: data-end
    }
}

/// True when the master's history holds a line the guest said.
pub fn knows_guest(history: &[Message], guest: Uuid) -> bool {
    history
        .iter()
        .any(|m| m.speaker.as_ref().is_some_and(|s| s.id == guest))
}

/// What the master is told about a guest it does not know yet: the personality
/// on one line, cut on a character boundary. `None` when it knows the guest or
/// there is nothing to tell.
pub fn guest_description(personality: &str, knows: bool) -> Option<String> {
    let text = personality.split_whitespace().collect::<Vec<_>>().join(" ");
    if knows || text.is_empty() {
        return None;
    }
    if text.chars().count() <= DESCRIPTION_CHARS {
        return Some(text);
    }
    let cut: String = text.chars().take(DESCRIPTION_CHARS).collect();
    Some(format!("{}…", cut.trim_end()))
}

/// The event for the guest that just joined, or `None` when there is no guest.
pub fn guest_joined(core: &Companion) -> Result<Option<AppEvent>> {
    let Some((master, guest)) = core.participants() else {
        return Ok(None);
    };
    let knows = knows_guest(&core.store.context_history(master.id)?, guest.id);
    let personality = core.profile(guest.id)?.details.personality;
    Ok(Some(AppEvent::GuestJoined {
        id: guest.id,
        name: guest.name,
        description: guest_description(&personality, knows),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{Delivery, MessageRole, Speaker};

    fn line(speaker: Option<Speaker>) -> Message {
        Message {
            web: None,
            speaker,
            id: Uuid::new_v4().to_string(),
            role: MessageRole::Assistant,
            content: "Hello".into(),
            delivery: Delivery::Completed,
            heard_content: "Hello".into(),
            created_at: 0,
        }
    }

    fn joined(description: Option<&str>) -> AppEvent {
        AppEvent::GuestJoined {
            id: Uuid::new_v4(),
            name: "Minerva".into(),
            description: description.map(Into::into),
        }
    }

    #[test]
    fn the_idle_prompt_is_the_initiative_invitation() {
        let prompt = AppEvent::Idle.prompt();
        assert!(
            prompt.starts_with("[Application idle event, not a user message] Take the initiative")
        );
        assert!(prompt.ends_with("Wait for a reply afterwards."));
        assert!(!AppEvent::Idle.master_only());
        assert_eq!(AppEvent::Idle.label(), "Idle invitation");
    }

    #[test]
    fn the_guest_prompt_names_the_guest_and_carries_a_description_only_when_given() {
        let known = joined(None).prompt();
        assert!(
            known.starts_with("[Application event, not a user message] Minerva has just joined")
        );
        assert!(!known.contains("What you know about"));
        assert!(known.contains("at most two short sentences"));
        assert!(known.contains("do not speak for Minerva"));
        let new = joined(Some("Irritable and sharp-tongued.")).prompt();
        assert!(new.contains(
            "request. What you know about Minerva: Irritable and sharp-tongued. React with"
        ));
        assert!(joined(None).master_only());
    }

    #[test]
    fn only_the_arrival_remark_starts_the_silence_relay() {
        assert!(joined(None).arms_relay());
        assert!(joined(Some("Sharp.")).arms_relay());
        assert!(!AppEvent::Idle.arms_relay());
    }

    #[test]
    fn the_master_knows_a_guest_it_has_heard_speak() {
        let guest = Uuid::new_v4();
        let said = Speaker {
            id: guest,
            name: "Minerva".into(),
        };
        let other = Speaker {
            id: Uuid::new_v4(),
            name: "Kit".into(),
        };
        assert!(!knows_guest(&[], guest));
        assert!(!knows_guest(&[line(None), line(Some(other))], guest));
        assert!(knows_guest(&[line(None), line(Some(said))], guest));
    }

    #[test]
    fn the_description_is_one_bounded_line_and_absent_for_a_known_guest() {
        assert_eq!(guest_description("Sharp.\n  Irritable.", true), None);
        assert_eq!(guest_description("  \n ", false), None);
        assert_eq!(
            guest_description("Sharp.\n  Irritable.", false).as_deref(),
            Some("Sharp. Irritable.")
        );
        let exact = "é".repeat(DESCRIPTION_CHARS);
        assert_eq!(guest_description(&exact, false), Some(exact));
        let long = format!("{} {}", "é".repeat(DESCRIPTION_CHARS - 1), "tail");
        let cut = guest_description(&long, false).unwrap();
        assert!(cut.ends_with('…'));
        assert!(cut.chars().count() <= DESCRIPTION_CHARS + 1);
        assert!(!cut.contains("tail"));
    }
}
