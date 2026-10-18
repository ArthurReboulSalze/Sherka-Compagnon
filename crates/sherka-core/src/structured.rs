//! Parse final answers only. Reasoning is never used as a fallback JSON result.
use crate::{Error, Result};
use serde::de::DeserializeOwned;

pub fn parse<T: DeserializeOwned>(text: &str) -> Result<T> {
    let mut text = text.trim().trim_start_matches('\u{feff}');
    for _ in 0..3 {
        let pair = [("<think>", "</think>"), ("<analysis>", "</analysis>")]
            .into_iter()
            .find(|(open, _)| text.starts_with(open));
        let Some((open, close)) = pair else { break };
        let Some(end) = text[open.len()..].find(close) else {
            return Err(invalid());
        };
        text = text[open.len() + end + close.len()..].trim();
    }
    if let Some(code) = text
        .strip_prefix("```json")
        .or_else(|| text.strip_prefix("```"))
    {
        text = code.trim().strip_suffix("```").ok_or_else(invalid)?.trim();
    }
    // Reject trailing prose, multiple objects and incomplete JSON. Never repair
    // or execute guessed memory actions from a truncated answer.
    serde_json::from_str(text).map_err(|_| invalid())
}
fn invalid() -> Error {
    Error::Validation(
        "La proposition du fournisseur est invalide ; aucune mémoire n’a été modifiée.".into(),
    )
}

/// Chat templates such as NeoHorse require all system instructions at the
/// beginning. Keep user data separate and consolidate trusted instructions.
pub(crate) fn request(
    messages: &mut Vec<crate::conversation::ChatMessage>,
    schema: &serde_json::Value,
) {
    let mut system = messages
        .iter()
        .filter(|m| m.role == "system")
        .map(|m| m.content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    let instruction = format!(
        "Return one JSON object only, matching this schema. No commentary or markdown. Schema: {schema}"
    );
    if !system.contains(&instruction) {
        system.push_str("\n\n");
        system.push_str(&instruction);
    }
    messages.retain(|m| m.role != "system");
    messages.insert(
        0,
        crate::conversation::ChatMessage {
            role: "system",
            content: system.trim().into(),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    #[test]
    fn final_json_survives_wrappers_but_never_uses_reasoning_or_partial_answers() {
        assert_eq!(
            parse::<Value>("<think>{\"wrong\":true}</think>\n```json\n{\"ok\":true}\n```").unwrap(),
            json!({"ok":true})
        );
        for bad in [
            "<think>{\"ok\":true}",
            "{\"ok\":",
            "{} {}",
            "Here is {}",
            "<think>{}</think>",
        ] {
            assert!(parse::<Value>(bad).is_err(), "{bad}");
        }
    }
}
