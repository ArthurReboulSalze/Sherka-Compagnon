//! Three-way conversation: the user talks with the active companion (master)
//! and a guest companion. Pure helpers: addressing, turn order, prompt text and
//! the filter that detects a companion choosing to stay silent.
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Participant {
    pub id: Uuid,
    pub name: String,
}

/// The other companion, as seen from the one who is about to speak.
#[derive(Debug, Clone)]
pub struct Peer {
    pub name: String,
    /// This companion reacts to the peer's line (spontaneous exchange, relay or
    /// chain), so it may pass.
    pub second: bool,
    /// Quick replies are on: one or two short spoken sentences.
    pub quick: bool,
}

/// Marker a second speaker replies with to stay silent.
pub const PASS: &str = "[[pass]]";

/// Lowercase and strip the accents of Latin-1 and Latin Extended-A letters.
fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => out.push('a'),
            'æ' => out.push_str("ae"),
            'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => out.push('c'),
            'ď' | 'đ' => out.push('d'),
            'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => out.push('e'),
            'ĝ' | 'ğ' | 'ġ' | 'ģ' => out.push('g'),
            'ĥ' | 'ħ' => out.push('h'),
            'ì' | 'í' | 'î' | 'ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' => out.push('i'),
            'ĵ' => out.push('j'),
            'ķ' => out.push('k'),
            'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => out.push('l'),
            'ñ' | 'ń' | 'ņ' | 'ň' => out.push('n'),
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ŏ' | 'ő' => out.push('o'),
            'œ' => out.push_str("oe"),
            'ŕ' | 'ŗ' | 'ř' => out.push('r'),
            'ś' | 'ŝ' | 'ş' | 'š' => out.push('s'),
            'ß' => out.push_str("ss"),
            'ţ' | 'ť' | 'ŧ' => out.push('t'),
            'ù' | 'ú' | 'û' | 'ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => out.push('u'),
            'ŵ' => out.push('w'),
            'ý' | 'ÿ' | 'ŷ' => out.push('y'),
            'ź' | 'ż' | 'ž' => out.push('z'),
            c => out.push(c),
        }
    }
    out
}

/// Words split on whitespace and punctuation (apostrophes and hyphens too).
fn words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(fold)
}

fn levenshtein(a: &[char], b: &[char]) -> usize {
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (diagonal + usize::from(ca != cb))
                .min(row[j] + 1)
                .min(above + 1);
            diagonal = above;
        }
    }
    row[b.len()]
}

/// Distance between a spoken word and a name, when close enough to count.
/// Names of 5 characters or more tolerate one edit, which absorbs
/// speech-recognition slips such as "Cherka" for "Sherka".
fn closeness(word: &str, name: &[char]) -> Option<usize> {
    let word: Vec<char> = word.chars().collect();
    let allowed = usize::from(name.len() >= 5);
    Some(levenshtein(&word, name)).filter(|d| *d <= allowed)
}

/// The participant addressed by name within the first three words of `text`.
pub fn addressee(text: &str, participants: &[Participant]) -> Option<Uuid> {
    let names: Vec<(Uuid, Vec<char>)> = participants
        .iter()
        .filter_map(|p| Some((p.id, words(&p.name).next()?.chars().collect())))
        .collect();
    words(text).take(3).find_map(|word| {
        names
            .iter()
            .filter_map(|(id, name)| Some((closeness(&word, name)?, *id)))
            .min_by_key(|(distance, _)| *distance)
            .map(|(_, id)| id)
    })
}

/// Who answers a message of the user: the addressed companion, otherwise the
/// coin picks one. Never both: the other one waits for the cooldown (see
/// `RelayState`).
pub fn answerer(text: &str, master: &Participant, guest: &Participant, coin: bool) -> Uuid {
    addressee(text, &[master.clone(), guest.clone()]).unwrap_or(if coin {
        master.id
    } else {
        guest.id
    })
}

/// A fair coin, from the same randomness source as the spontaneous initiative.
pub fn coin() -> bool {
    Uuid::new_v4().as_bytes()[0] & 1 == 1
}

/// Label of the user's lines in a multi-party prompt when no name is set.
pub const USER_LABEL: &str = "User";

/// The label of the user's lines: the configured name, else "User".
pub fn user_label(user_name: &str) -> &str {
    Some(user_name.trim())
        .filter(|name| !name.is_empty())
        .unwrap_or(USER_LABEL)
}

/// A line of the user in a multi-party prompt.
pub fn user_line(user_name: &str, text: &str) -> String {
    format!("{}: {text}", user_label(user_name))
}

/// How the final message of a prompt is attributed in a multi-party context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Last {
    /// The user's own message: it gets the user's label.
    User,
    /// Attributed already: a follow message or an application event.
    Attributed,
}

/// The final user-role message for the companion who speaks second. The
/// user's part, if any, carries the user's label.
pub fn follow_message(
    user: Option<&str>,
    user_name: &str,
    first: &Participant,
    reply: &str,
) -> String {
    let line = format!("{}: {}", first.name.trim(), reply.trim());
    match user.map(str::trim).filter(|u| !u.is_empty()) {
        Some(user) => format!("{}\n\n{line}", user_line(user_name, user)),
        None => line,
    }
}

/// Appended to the paragraph when quick replies are on.
pub const QUICK: &str = " Answer in one or two short spoken sentences; no lists.";

/// Response budget of a quick trio reply, in tokens.
pub const QUICK_TOKENS: u32 = 160;

/// Response budget of a trio reply: capped when quick replies are on.
pub fn response_tokens(current: u32, quick: bool) -> u32 {
    if quick {
        current.min(QUICK_TOKENS)
    } else {
        current
    }
}

/// Silences of the user before the companions stop waiting for them and chain
/// replies: the cooldown (`relay_seconds`) after the answer, then `QUICK_SILENCE`.
pub const SILENCES_BEFORE_CHAIN: u32 = 2;

/// The second silence: the user already had the whole cooldown to speak, so
/// the companions only leave them a moment before going on.
pub const QUICK_SILENCE: std::time::Duration = std::time::Duration::from_secs(1);

/// Most replies the companions produce in a row (relays and chained replies)
/// after the user's last message.
pub const CHAIN_LIMIT: u32 = 20;

/// Consecutive passes after which the companions have nothing left to say.
pub const PASS_LIMIT: u32 = 2;

/// Natural pause between two chained replies.
pub const CHAIN_PAUSE: std::time::Duration = std::time::Duration::from_millis(600);

/// Where the companions stand since the user's last message, once an exchange
/// ended normally and they wait for the user.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RelayState {
    /// Silences of the user that have elapsed.
    pub silences: u32,
    /// Replies produced since the user's message (relays and chained replies).
    pub replies: u32,
    /// Passes in a row, the last reaction included.
    pub passes: u32,
}

/// What happens after the latest step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayNext {
    /// Wait for the user this long, then let the other companion react: the
    /// cooldown (`relay_seconds`) or, after the first reaction, `QUICK_SILENCE`.
    Wait(std::time::Duration),
    /// The user stayed silent twice: the other companion reacts after a short pause.
    Chain(std::time::Duration),
    /// Back to waiting for the user, for good.
    Stop,
}

impl RelayState {
    /// The next move. `relay_seconds` 0 turns the whole mechanism off. The
    /// first silence is the cooldown after the answer to the user, the second
    /// one lasts `QUICK_SILENCE`; a companion that just passed is asked again
    /// only after a further cooldown.
    pub fn next(self, relay_seconds: u32) -> RelayNext {
        let cooldown = std::time::Duration::from_secs(relay_seconds.into());
        if relay_seconds == 0 || self.replies >= CHAIN_LIMIT || self.passes >= PASS_LIMIT {
            RelayNext::Stop
        } else if self.passes > 0 || self.silences == 0 {
            RelayNext::Wait(cooldown)
        } else if self.silences < SILENCES_BEFORE_CHAIN {
            RelayNext::Wait(QUICK_SILENCE)
        } else {
            RelayNext::Chain(CHAIN_PAUSE)
        }
    }
    /// A silence elapsed without the user doing anything.
    pub fn silence(&mut self) {
        self.silences = self.silences.saturating_add(1);
    }
    /// A companion spoke.
    pub fn replied(&mut self) {
        self.replies = self.replies.saturating_add(1);
        self.passes = 0;
    }
    /// A companion chose to stay silent.
    pub fn passed(&mut self) {
        self.passes = self.passes.saturating_add(1);
    }
    /// Another reply would follow the one being produced at once: the turn
    /// stays open and the next speaker may prepare while this one talks.
    pub fn chains_after_reply(self, relay_seconds: u32) -> bool {
        let mut next = self;
        next.replied();
        matches!(next.next(relay_seconds), RelayNext::Chain(_))
    }
    /// Another reply would follow the one being produced, after a wait: the
    /// turn closes, but the next speaker may prepare detached from it.
    pub fn waits_after_reply(self, relay_seconds: u32) -> bool {
        let mut next = self;
        next.replied();
        matches!(next.next(relay_seconds), RelayNext::Wait(_))
    }
}

/// The companion who reacts to `last`'s line: the other participant.
pub fn reactor<'a>(last: Uuid, a: &'a Participant, b: &'a Participant) -> &'a Participant {
    if last == a.id { b } else { a }
}

/// System prompt paragraph describing the other companion.
pub fn peer_instructions(own_name: &str, peer: &Peer, user_name: &str) -> String {
    let (own, other, label) = (own_name.trim(), peer.name.trim(), user_label(user_name));
    // Unnamed, the user is "the user" in prose and "User:" on their lines.
    let (user, user_intro) = match user_name.trim() {
        "" => ("the user".to_owned(), "the user".to_owned()),
        name => (name.to_owned(), format!("the user ({name})")),
    };
    let mut text = format!(
        "\nThree-way conversation: there are three participants: {user_intro}, you ({own}) and another companion named {other}. Lines starting with \"{label}:\" were said by {user}; lines starting with \"{other}:\" were said by {other}. Treat them as different people: never attribute {user}'s words to {other} or {other}'s words to {user}. Speak only as {own}, in the first person; never write {other}'s or {user}'s lines or speak on their behalf, and do not prefix your reply with your name or with \"{label}:\". Keep your reply short, a few sentences, so the conversation flows."
    );
    if peer.second {
        text.push_str(&format!(" {other} has just answered. React to {user} and to {other} without repeating what was said. If you have nothing worthwhile to add, reply only {PASS}."));
    }
    if peer.quick {
        text.push_str(QUICK);
    }
    text
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PassOutput {
    /// Not decided yet: nothing to show.
    Pending,
    /// Visible text to forward (the buffered start of the reply, then chunks).
    Text(String),
    /// The companion chose silence.
    Passed,
}

#[derive(Debug, Default)]
enum PassState {
    #[default]
    Buffering,
    Streaming,
    Silent,
}

/// Detects `[[pass]]` at the start of a streamed reply. Leading text is held
/// back only while it could still turn out to be the marker.
#[derive(Debug, Default)]
pub struct PassFilter {
    state: PassState,
    buffer: String,
}

impl PassFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, chunk: &str) -> PassOutput {
        match self.state {
            PassState::Silent => return PassOutput::Passed,
            PassState::Streaming => return PassOutput::Text(chunk.to_owned()),
            PassState::Buffering => {}
        }
        self.buffer.push_str(chunk);
        let lead = self.buffer.trim_start().to_ascii_lowercase();
        if lead.is_empty() {
            return PassOutput::Pending;
        }
        if lead.starts_with("[[pass") {
            self.state = PassState::Silent;
            self.buffer.clear();
            return PassOutput::Passed;
        }
        if PASS.starts_with(&lead) {
            return PassOutput::Pending;
        }
        self.state = PassState::Streaming;
        PassOutput::Text(std::mem::take(&mut self.buffer))
    }

    /// End of the stream. An empty reply counts as silence; text still held
    /// back (a lone "[") is released. `Pending` means nothing remains.
    pub fn finish(&mut self) -> PassOutput {
        match self.state {
            PassState::Silent => PassOutput::Passed,
            PassState::Streaming => PassOutput::Pending,
            PassState::Buffering if self.buffer.trim().is_empty() => {
                self.state = PassState::Silent;
                self.buffer.clear();
                PassOutput::Passed
            }
            PassState::Buffering => {
                self.state = PassState::Streaming;
                PassOutput::Text(std::mem::take(&mut self.buffer))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn who(name: &str) -> Participant {
        Participant {
            id: Uuid::new_v4(),
            name: name.into(),
        }
    }

    #[test]
    fn addressee_matches_names_in_the_first_three_words() {
        let (minerva, kit) = (who("Minerva"), who("Kit"));
        let all = [minerva.clone(), kit.clone()];
        let find = |text: &str| addressee(text, &all);
        assert_eq!(find("Minerva, raconte-moi une blague"), Some(minerva.id));
        assert_eq!(find("Hé Kit, tu dors ?"), Some(kit.id));
        assert_eq!(find("MINÉRVA ?"), Some(minerva.id));
        assert_eq!(find("Kit et Minerva, venez"), Some(kit.id));
        assert_eq!(find("Minerva et Kit, venez"), Some(minerva.id));
        assert_eq!(find("Raconte une blague à Minerva"), None);
        assert_eq!(find(""), None);
        assert_eq!(find("Bonjour tout le monde"), None);
    }

    #[test]
    fn addressee_tolerates_recognition_slips_on_long_names() {
        let (sherka, kit) = (who("Sherka"), who("Kit"));
        let all = [sherka.clone(), kit.clone()];
        assert_eq!(addressee("Dis Sherka, tu es là", &all), Some(sherka.id));
        assert_eq!(addressee("cherka tu es là", &all), Some(sherka.id));
        assert_eq!(addressee("Shirka ?", &all), Some(sherka.id));
        assert_eq!(addressee("Sherkas, hein", &all), Some(sherka.id));
        // Two edits is too far.
        assert_eq!(addressee("Chirka tu es là", &all), None);
        // Short names must match exactly.
        assert_eq!(addressee("Kitty, viens", &all), None);
        assert_eq!(addressee("Kat, viens", &all), None);
        assert_eq!(addressee("kit viens", &all), Some(kit.id));
    }

    #[test]
    fn addressee_ignores_accents_punctuation_and_extra_name_words() {
        let (helene, ana) = (who("Hélène Dupont"), who("Aña"));
        let all = [helene.clone(), ana.clone()];
        assert_eq!(addressee("Helene! écoute", &all), Some(helene.id));
        assert_eq!(addressee("« Hélène »", &all), Some(helene.id));
        assert_eq!(addressee("Dis-moi Ana", &all), Some(ana.id));
        assert_eq!(addressee("Dupont, écoute", &all), None);
        assert_eq!(addressee("C'est Ana", &all), Some(ana.id));
        assert_eq!(addressee("Ça va, Ana ?", &all), Some(ana.id));
        assert_eq!(addressee("Ça va bien Ana ?", &all), None);
    }

    #[test]
    fn levenshtein_counts_single_edits() {
        let d = |a: &str, b: &str| {
            levenshtein(
                &a.chars().collect::<Vec<_>>(),
                &b.chars().collect::<Vec<_>>(),
            )
        };
        assert_eq!(d("", ""), 0);
        assert_eq!(d("abc", "abc"), 0);
        assert_eq!(d("abc", "abd"), 1);
        assert_eq!(d("abc", "abcd"), 1);
        assert_eq!(d("sherka", "cherka"), 1);
        assert_eq!(d("kitten", "sitting"), 3);
        assert_eq!(d("", "abc"), 3);
    }

    #[test]
    fn fold_removes_latin_accents() {
        assert_eq!(fold("ÀÉîõüÇñŒß"), "aeioucnoess");
        assert_eq!(fold("Zażółć"), "zazolc");
    }

    #[test]
    fn one_companion_answers_the_addressee_or_the_coin() {
        let (master, guest) = (who("Sherka"), who("Minerva"));
        let answer = |text, coin| answerer(text, &master, &guest, coin);
        // A name wins over the coin, in any message.
        assert_eq!(answer("Minerva, ça va ?", true), guest.id);
        assert_eq!(answer("Sherka, ça va ?", false), master.id);
        // Otherwise the coin picks one of them, never both.
        assert_eq!(answer("Ça va ?", true), master.id);
        assert_eq!(answer("Ça va ?", false), guest.id);
        assert_eq!(answer("", true), master.id);
        // Naming both: the first name in the message answers.
        assert_eq!(answer("Minerva et Sherka, venez", true), guest.id);
        assert_eq!(answer("Sherka et Minerva, venez", false), master.id);
    }

    #[test]
    fn coin_gives_both_sides() {
        let flips: Vec<bool> = (0..200).map(|_| coin()).collect();
        assert!(flips.contains(&true) && flips.contains(&false));
    }

    #[test]
    fn follow_message_quotes_the_first_reply() {
        let first = who("Sherka");
        assert_eq!(
            follow_message(Some(" Salut "), "", &first, "  Coucou !\n"),
            "User: Salut\n\nSherka: Coucou !"
        );
        assert_eq!(
            follow_message(Some("Salut"), " Arthur ", &first, "Coucou !"),
            "Arthur: Salut\n\nSherka: Coucou !"
        );
        assert_eq!(follow_message(None, "", &first, " Hé "), "Sherka: Hé");
        assert_eq!(follow_message(None, "Arthur", &first, "Hé"), "Sherka: Hé");
        assert_eq!(follow_message(Some("  "), "", &first, "Hé"), "Sherka: Hé");
    }

    #[test]
    fn the_user_label_is_the_name_or_user() {
        assert_eq!(user_label(""), "User");
        assert_eq!(user_label("  "), "User");
        assert_eq!(user_label(" Arthur "), "Arthur");
        assert_eq!(user_line("", "Hi"), "User: Hi");
        assert_eq!(user_line("Arthur", "Hi"), "Arthur: Hi");
    }

    #[test]
    fn instructions_name_the_three_participants() {
        let peer = Peer {
            name: "Sherka".into(),
            second: false,
            quick: false,
        };
        let plain = peer_instructions("Minerva", &peer, "");
        assert!(plain.contains(
            "there are three participants: the user, you (Minerva) and another companion named Sherka"
        ));
        assert!(plain.contains("Lines starting with \"User:\" were said by the user"));
        assert!(plain.contains("lines starting with \"Sherka:\" were said by Sherka"));
        assert!(
            plain.contains(
                "never attribute the user's words to Sherka or Sherka's words to the user"
            )
        );
        assert!(plain.contains("do not prefix your reply with your name or with \"User:\""));
        let named = peer_instructions("Minerva", &peer, " Arthur ");
        assert!(named.contains(
            "three participants: the user (Arthur), you (Minerva) and another companion named Sherka"
        ));
        assert!(named.contains("Lines starting with \"Arthur:\" were said by Arthur"));
        assert!(
            named.contains("never attribute Arthur's words to Sherka or Sherka's words to Arthur")
        );
        assert!(named.contains("or with \"Arthur:\""));
        assert!(!named.contains("User:"));
        let second = peer_instructions(
            "Minerva",
            &Peer {
                second: true,
                ..peer
            },
            "Arthur",
        );
        assert!(second.contains("React to Arthur and to Sherka without"));
    }

    #[test]
    fn instructions_mention_pass_only_for_the_second_speaker() {
        let first = peer_instructions(
            "Minerva",
            &Peer {
                name: "Sherka".into(),
                second: false,
                quick: false,
            },
            "",
        );
        assert!(first.contains("another companion named Sherka"));
        assert!(first.contains("Speak only as Minerva"));
        assert!(first.contains("\"Sherka:\""));
        assert!(!first.contains(PASS));
        let second = peer_instructions(
            "Minerva",
            &Peer {
                name: "Sherka".into(),
                second: true,
                quick: false,
            },
            "",
        );
        assert!(second.starts_with(&first));
        assert!(second.contains("Sherka has just answered"));
        assert!(second.ends_with("reply only [[pass]]."));
    }

    #[test]
    fn quick_replies_add_one_short_sentence_to_either_speaker() {
        for second in [false, true] {
            let slow = peer_instructions(
                "Minerva",
                &Peer {
                    name: "Sherka".into(),
                    second,
                    quick: false,
                },
                "",
            );
            let quick = peer_instructions(
                "Minerva",
                &Peer {
                    name: "Sherka".into(),
                    second,
                    quick: true,
                },
                "",
            );
            assert!(!slow.contains("one or two short spoken sentences"));
            assert!(quick.starts_with(&slow));
            assert!(quick.ends_with(" Answer in one or two short spoken sentences; no lists."));
            assert!(quick.contains("Keep your reply short"));
        }
    }

    #[test]
    fn quick_replies_cap_the_budget_and_never_raise_it() {
        assert_eq!(response_tokens(2048, true), 160);
        assert_eq!(response_tokens(128, true), 128);
        assert_eq!(response_tokens(2048, false), 2048);
    }

    #[test]
    fn the_cooldown_comes_first_then_a_quick_silence_then_the_chain() {
        let secs = |n| RelayNext::Wait(std::time::Duration::from_secs(n));
        let mut state = RelayState::default();
        // The user spoke and one companion answered: the cooldown.
        assert_eq!(state.next(5), secs(5));
        state.silence();
        state.replied();
        // The other one reacted: only a quick silence before they go on.
        assert_eq!(state.next(5), RelayNext::Wait(QUICK_SILENCE));
        assert_eq!(QUICK_SILENCE, std::time::Duration::from_secs(1));
        state.silence();
        state.replied();
        assert_eq!(state.silences, 2);
        // From now on they no longer wait.
        for _ in state.replies..CHAIN_LIMIT {
            assert_eq!(state.next(5), RelayNext::Chain(CHAIN_PAUSE));
            state.replied();
        }
        assert_eq!(state.replies, CHAIN_LIMIT);
        assert_eq!(state.next(5), RelayNext::Stop);
        // The cooldown follows the setting, the quick silence and the pause do not.
        let mut state = RelayState::default();
        assert_eq!(state.next(30), secs(30));
        state.silences = 1;
        assert_eq!(state.next(30), RelayNext::Wait(QUICK_SILENCE));
        state.silences = 2;
        assert_eq!(state.next(30), RelayNext::Chain(CHAIN_PAUSE));
    }

    #[test]
    fn the_relay_is_off_with_zero_seconds_at_any_point() {
        for silences in 0..4 {
            for replies in [0, 1, 5] {
                let state = RelayState {
                    silences,
                    replies,
                    passes: 0,
                };
                assert_eq!(state.next(0), RelayNext::Stop);
                assert!(!state.chains_after_reply(0) && !state.waits_after_reply(0));
            }
        }
    }

    #[test]
    fn a_pass_waits_for_a_cooldown_and_two_in_a_row_stop() {
        let cooldown = RelayNext::Wait(std::time::Duration::from_secs(8));
        // The first reaction passes: asked again only after a further cooldown.
        let mut first = RelayState {
            silences: 1,
            ..Default::default()
        };
        first.passed();
        assert_eq!(first.next(8), cooldown);
        first.silence();
        first.passed();
        assert_eq!(first.next(8), RelayNext::Stop);
        // In the chain a pass also waits, a spoken reply clears the streak.
        let mut chaining = RelayState {
            silences: 2,
            replies: 3,
            passes: 0,
        };
        assert_eq!(chaining.next(8), RelayNext::Chain(CHAIN_PAUSE));
        chaining.passed();
        assert_eq!(chaining.next(8), cooldown);
        let mut replied = chaining;
        replied.replied();
        assert_eq!(replied.passes, 0);
        assert_eq!(replied.next(8), RelayNext::Chain(CHAIN_PAUSE));
        chaining.passed();
        assert_eq!(chaining.next(8), RelayNext::Stop);
    }

    #[test]
    fn a_reply_keeps_the_turn_open_only_when_another_follows_at_once() {
        let mut state = RelayState::default();
        // The answer to the user: the cooldown follows, nothing at once.
        state.silence();
        assert!(!state.chains_after_reply(8), "the quick silence follows");
        assert!(state.waits_after_reply(8));
        state.replied();
        state.silence();
        assert!(state.chains_after_reply(8), "the chain opens");
        assert!(!state.waits_after_reply(8));
        assert!(!state.chains_after_reply(0), "off");
        state.replies = CHAIN_LIMIT - 1;
        assert!(!state.chains_after_reply(8), "the last allowed reply");
        assert!(!state.waits_after_reply(8));
    }

    #[test]
    fn the_reactor_is_the_participant_who_did_not_speak_last() {
        let (a, b) = (who("Sherka"), who("Kit"));
        assert_eq!(reactor(a.id, &a, &b).id, b.id);
        assert_eq!(reactor(b.id, &a, &b).id, a.id);
    }

    fn run(chunks: &[&str]) -> (Vec<PassOutput>, PassOutput) {
        let mut filter = PassFilter::new();
        let pushed = chunks.iter().map(|c| filter.push(c)).collect();
        (pushed, filter.finish())
    }

    #[test]
    fn pass_filter_detects_the_marker_across_chunks() {
        let (pushed, end) = run(&["[", "[pa", "ss]]"]);
        assert_eq!(
            pushed,
            [PassOutput::Pending, PassOutput::Pending, PassOutput::Passed]
        );
        assert_eq!(end, PassOutput::Passed);
        let (pushed, _) = run(&["\n  ", " [[Pass]]", " ignored"]);
        assert_eq!(
            pushed,
            [PassOutput::Pending, PassOutput::Passed, PassOutput::Passed]
        );
        assert_eq!(run(&["[[PASS]]"]).0, [PassOutput::Passed]);
        assert_eq!(run(&["[[pass"]).0, [PassOutput::Passed]);
    }

    #[test]
    fn pass_filter_releases_normal_text() {
        let (pushed, end) = run(&["Bon", "jour", " !"]);
        assert_eq!(
            pushed,
            [
                PassOutput::Text("Bon".into()),
                PassOutput::Text("jour".into()),
                PassOutput::Text(" !".into())
            ]
        );
        assert_eq!(end, PassOutput::Pending);
        let (pushed, _) = run(&[" \n", "Salut", " toi"]);
        assert_eq!(
            pushed,
            [
                PassOutput::Pending,
                PassOutput::Text(" \nSalut".into()),
                PassOutput::Text(" toi".into())
            ]
        );
    }

    #[test]
    fn pass_filter_releases_a_bracket_that_is_not_the_marker() {
        let (pushed, _) = run(&["[", "note] hi", " there"]);
        assert_eq!(
            pushed,
            [
                PassOutput::Pending,
                PassOutput::Text("[note] hi".into()),
                PassOutput::Text(" there".into())
            ]
        );
        let (pushed, _) = run(&["[[pa", "ge]] x"]);
        assert_eq!(pushed[0], PassOutput::Pending);
        assert_eq!(pushed[1], PassOutput::Text("[[page]] x".into()));
    }

    #[test]
    fn pass_filter_finish_decides_ambiguous_endings() {
        assert_eq!(run(&[]).1, PassOutput::Passed);
        assert_eq!(run(&[" \n "]).1, PassOutput::Passed);
        assert_eq!(run(&["["]).1, PassOutput::Text("[".into()));
        assert_eq!(run(&[" [[pa"]).1, PassOutput::Text(" [[pa".into()));
        let mut filter = PassFilter::new();
        assert_eq!(filter.push("[["), PassOutput::Pending);
        assert_eq!(filter.finish(), PassOutput::Text("[[".into()));
        assert_eq!(filter.push("x"), PassOutput::Text("x".into()));
    }
}
