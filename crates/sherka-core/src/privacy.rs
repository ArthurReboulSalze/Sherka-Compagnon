//! Explicit user commands only. Never interpret model output or a substring
//! inside quoted, negated, conditional or ordinary conversational text.
pub fn clear_conversation_intent(text: &str, companion_name: &str) -> bool {
    let normalize = |s: &str| {
        s.to_lowercase()
            .replace('’', "'")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    let value = normalize(text);
    let mut s = value
        .trim_end_matches(['.', '!', '?', '。', '！', '？'])
        .trim();
    if s == "/clear" {
        return true;
    }
    let name = normalize(companion_name);
    for separator in [", ", ": ", " "] {
        if let Some(rest) = s.strip_prefix(&format!("{name}{separator}")) {
            s = rest;
            break;
        }
    }
    for prefix in [
        "s'il te plaît, ",
        "s'il te plaît ",
        "peux-tu ",
        "peux tu ",
        "please ",
        "can you ",
        "por favor, ",
        "por favor ",
        "per favore, ",
        "per favore ",
        "bitte ",
    ] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest;
            break;
        }
    }
    for suffix in [
        ", s'il te plaît",
        " s'il te plaît",
        ", please",
        " please",
        " en gardant les faits sauvegardés",
        " sans effacer les faits sauvegardés",
        " but keep saved facts",
    ] {
        if let Some(rest) = s.strip_suffix(suffix) {
            s = rest;
            break;
        }
    }
    let grammars: &[(&[&str], &[&str])] = &[
        (
            &[
                "efface",
                "effacer",
                "supprime",
                "supprimer",
                "oublie",
                "oublier",
            ],
            &[
                "notre conversation",
                "la conversation",
                "toute la conversation",
                "la discussion",
                "toute la discussion",
                "notre discussion",
                "toute notre discussion",
                "nos conversations",
                "toute notre conversation",
                "toutes nos conversations",
                "tous nos messages",
                "notre historique",
                "tout notre historique",
                "l'historique de nos conversations",
                "l'historique de notre conversation",
                "tout ce qu'on s'est dit",
                "tout ce que nous nous sommes dit",
                "toute trace de conversation avec toi",
                "toute trace de notre conversation",
            ],
        ),
        (
            &["clear", "delete", "erase", "forget"],
            &[
                "our conversation",
                "the conversation",
                "the whole conversation",
                "the discussion",
                "all the discussion",
                "our conversations",
                "all our messages",
                "our chat history",
                "all our conversation history",
                "everything we said",
                "all traces of our conversation",
            ],
        ),
        (
            &["borra", "elimina", "olvida"],
            &[
                "nuestra conversación",
                "nuestras conversaciones",
                "todos nuestros mensajes",
                "nuestro historial de conversación",
            ],
        ),
        (
            &["cancella", "elimina", "dimentica"],
            &[
                "la nostra conversazione",
                "le nostre conversazioni",
                "tutti i nostri messaggi",
                "la cronologia della nostra conversazione",
            ],
        ),
        (
            &["lösche", "vergiss"],
            &[
                "unser gespräch",
                "unsere gespräche",
                "alle unsere nachrichten",
                "unseren chatverlauf",
            ],
        ),
    ];
    grammars.iter().any(|(verbs, targets)| {
        verbs
            .iter()
            .any(|verb| targets.iter().any(|target| s == format!("{verb} {target}")))
    }) || [
        "清空我们的对话",
        "删除我们的对话",
        "清除我们的聊天记录",
        "删除所有聊天记录",
        "私たちの会話を削除して",
        "会話履歴を削除して",
        "会話履歴を消して",
    ]
    .contains(&s)
}

#[derive(Default)]
pub struct PendingClear(Option<(uuid::Uuid, uuid::Uuid, std::time::Instant)>);
impl PendingClear {
    pub fn request(&mut self, profile: uuid::Uuid, session: uuid::Uuid) {
        self.0 = Some((profile, session, std::time::Instant::now()));
    }
    pub fn valid(&self, profile: uuid::Uuid, session: uuid::Uuid) -> bool {
        self.0.is_some_and(|(p, s, time)| {
            p == profile && s == session && time.elapsed() < std::time::Duration::from_secs(180)
        })
    }
    pub fn take(&mut self, profile: uuid::Uuid, session: uuid::Uuid) -> bool {
        let valid = self.valid(profile, session);
        self.0 = None;
        valid
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    RequestClear,
    ConfirmClear,
    Cancel,
    Optimize,
    ApplyOptimization,
}
pub fn command(text: &str, name: &str) -> Option<Command> {
    if clear_conversation_intent(text, name) {
        return Some(Command::RequestClear);
    }
    let value = text.trim().to_lowercase().replace('’', "'");
    let value = value
        .trim_end_matches(['.', '!', '?', '。', '！', '？'])
        .trim();
    if [
        "/confirm-clear",
        "confirme la suppression",
        "je confirme la suppression",
        "confirm deletion",
        "confirm clear",
        "confirmar eliminación",
        "conferma eliminazione",
        "löschen bestätigen",
        "确认删除",
        "削除を確認",
    ]
    .contains(&value)
    {
        return Some(Command::ConfirmClear);
    }
    if [
        "/cancel",
        "annule",
        "annule la suppression",
        "annule l'optimisation",
        "cancel",
        "cancel deletion",
        "cancelar",
        "annulla",
        "abbrechen",
        "取消",
        "キャンセル",
    ]
    .contains(&value)
    {
        return Some(Command::Cancel);
    }
    if [
        "/optimize-memory",
        "optimise ta mémoire",
        "optimise la mémoire",
        "optimise tes souvenirs",
        "optimiser la mémoire",
        "peux-tu optimiser ta mémoire",
        "peux tu optimiser ta mémoire",
        "optimize your memory",
        "optimise your memory",
        "optimize memory",
        "optimiza tu memoria",
        "ottimizza la tua memoria",
        "optimiere dein gedächtnis",
        "优化你的记忆",
        "記憶を最適化して",
    ]
    .contains(&value)
    {
        return Some(Command::Optimize);
    }
    if [
        "/apply-memory",
        "confirme l'optimisation",
        "applique l'optimisation",
        "confirm memory optimization",
        "apply memory optimization",
        "confirma la optimización",
        "conferma ottimizzazione",
        "speicheroptimierung bestätigen",
        "确认记忆优化",
        "記憶の最適化を確認",
    ]
    .contains(&value)
    {
        return Some(Command::ApplyOptimization);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn confirmation_is_explicit_one_shot_and_scoped_to_the_current_profile_session() {
        use uuid::Uuid;
        let p = Uuid::new_v4();
        let s = Uuid::new_v4();
        let mut pending = PendingClear::default();
        assert!(!pending.take(p, s));
        pending.request(p, s);
        assert!(!pending.valid(Uuid::new_v4(), s));
        assert!(!pending.valid(p, Uuid::new_v4()));
        assert!(pending.take(p, s));
        assert!(!pending.take(p, s));
        pending.0 = Some((
            p,
            s,
            std::time::Instant::now() - std::time::Duration::from_secs(181),
        ));
        assert!(!pending.take(p, s));
        assert_eq!(
            command("Efface toute la discussion", "Kit"),
            Some(Command::RequestClear)
        );
        assert_eq!(
            command("confirme la suppression", "Kit"),
            Some(Command::ConfirmClear)
        );
        assert_eq!(
            command("optimise ta mémoire", "Kit"),
            Some(Command::Optimize)
        );
        assert_eq!(
            command("confirme l’optimisation", "Kit"),
            Some(Command::ApplyOptimization)
        );
        for text in [
            "oui",
            "Ne confirme pas la suppression",
            "Il dit : confirme la suppression",
            "Retiens le mot /clear",
            "Comment optimiser ta mémoire ?",
        ] {
            assert_eq!(command(text, "Kit"), None);
        }
    }
    #[test]
    fn only_explicit_whole_requests_clear_history_in_all_seven_languages() {
        for text in [
            "/clear",
            "Kit, efface toute trace de conversation avec toi.",
            "Peux-tu effacer notre conversation sans effacer les faits sauvegardés ?",
            "Please clear our conversation but keep saved facts.",
            "Borra nuestra conversación",
            "Cancella la nostra conversazione",
            "Bitte lösche unseren Chatverlauf",
            "清空我们的对话",
            "会話履歴を削除して",
        ] {
            assert!(clear_conversation_intent(text, "Kit"), "{text}");
        }
        for text in [
            "N'efface pas notre conversation",
            "Don't delete our conversation",
            "Comment effacer notre conversation ?",
            "Il a dit : efface notre conversation",
            "Si je dis efface notre conversation",
            "Translate: clear our conversation",
            "Retiens la phrase efface notre conversation",
            "Efface notre conversation et tous mes faits",
            "\"Efface notre conversation\"",
            "Mon message contient /clear",
            "Oublie ma ville",
        ] {
            assert!(!clear_conversation_intent(text, "Kit"), "{text}");
        }
    }
}
