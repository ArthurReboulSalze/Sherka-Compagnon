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

#[cfg(test)]
mod tests {
    use super::*;
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
