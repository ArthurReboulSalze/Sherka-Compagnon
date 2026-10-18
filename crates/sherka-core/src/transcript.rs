//! Conservative ASR cleanup and joining of segment/cumulative final events.
//! Never remove a phrase merely because it is short or unfamiliar.
pub fn clean(value: &str) -> String {
    let text = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let folded: String = text
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    let subtitle_credit = ["soustitrage", "soustitrages"].iter().any(|prefix| {
        folded.strip_prefix(prefix).is_some_and(|tail| {
            tail.strip_prefix("st").is_some_and(|digits| {
                !digits.is_empty()
                    && digits.len() <= 5
                    && digits.chars().all(|c| c.is_ascii_digit())
            })
        })
    });
    let marker = [
        "[blank_audio]",
        "[silence]",
        "[music]",
        "[musique]",
        "[inaudible]",
        "(inaudible)",
        "[cough]",
        "[toux]",
        "[applause]",
    ]
    .contains(&text.to_lowercase().as_str());
    if subtitle_credit || marker {
        String::new()
    } else {
        text
    }
}

/// Providers differ: a final can be one segment, the entire capture, or a
/// repeated last segment. Preserve earlier confirmed words in each case.
pub fn join(previous: &str, next: &str) -> String {
    let previous = previous.trim();
    let next = clean(next);
    if previous.is_empty() {
        return next;
    }
    if next.is_empty() || previous.ends_with(&next) {
        return previous.into();
    }
    if next.starts_with(previous) {
        return next;
    }
    let a: Vec<_> = previous.split_whitespace().collect();
    let b: Vec<_> = next.split_whitespace().collect();
    let overlap = (1..=a.len().min(b.len()))
        .rev()
        .find(|n| a[a.len() - n..] == b[..*n])
        .unwrap_or(0);
    if overlap == b.len() {
        previous.into()
    } else {
        format!("{previous} {}", b[overlap..].join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn noise_credits_are_ignored_but_real_short_words_and_discussion_are_kept() {
        for value in [
            "Sous-titrage ST’501",
            "SOUS-TITRAGE ST' 501",
            "Sous titrage ST 501",
            "[BLANK_AUDIO]",
            "[toux]",
        ] {
            assert_eq!(clean(value), "");
        }
        for value in [
            "oui",
            "non",
            "coucou",
            "Je voudrais modifier le sous-titrage ST 501.",
            "Merci d’avoir regardé cette vidéo.",
        ] {
            assert_eq!(clean(value), value);
        }
    }
    #[test]
    fn segment_and_summary_variants_preserve_all_words_once() {
        let a = join("", "Je prépare le repas.");
        let b = join(&a, "Ensuite je promène le chien.");
        assert_eq!(join(&b, "Ensuite je promène le chien."), b);
        assert_eq!(join(&a, &b), b);
        assert_eq!(
            join("je vais au", "au marché demain"),
            "je vais au marché demain"
        );
        assert_eq!(join(&b, "Sous-titrage ST’501"), b);
    }
}
