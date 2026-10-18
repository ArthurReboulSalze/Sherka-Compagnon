use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AvatarPreset {
    Sherka,
    Botsy,
    // Keep the 0.8.0 preset identifier readable by existing profiles/exports.
    #[serde(rename = "candis", alias = "alba")]
    Alba,
    Kit,
    Minerva,
}

impl AvatarPreset {
    pub fn name(self) -> &'static str {
        match self {
            Self::Sherka => "Sherka",
            Self::Botsy => "Botsy",
            Self::Alba => "Alba",
            Self::Kit => "Kit",
            Self::Minerva => "Minerva",
        }
    }
    pub fn neutral(self) -> &'static str {
        match self {
            Self::Sherka => "/assets/sherka.png",
            Self::Botsy => "/assets/botsy/neutral.png",
            Self::Alba => "/assets/alba/neutral.png",
            Self::Kit => "/assets/kit/neutral.png",
            Self::Minerva => "/assets/minerva-v2/neutral.png",
        }
    }
    pub fn images(self) -> [(Emotion, String); 6] {
        [
            Emotion::Joy,
            Emotion::Sadness,
            Emotion::Anger,
            Emotion::Surprise,
            Emotion::Reflection,
            Emotion::Tenderness,
        ]
        .map(|emotion| {
            (
                emotion,
                format!(
                    "/assets/{}/{}.png",
                    if self == Self::Minerva {
                        "minerva-v2".into()
                    } else {
                        self.name().to_ascii_lowercase()
                    },
                    emotion.as_str()
                ),
            )
        })
    }
}

/// Stable identifiers shared by stored images and the conversation protocol.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Emotion {
    #[default]
    Neutral,
    Joy,
    Sadness,
    Anger,
    Surprise,
    Reflection,
    Tenderness,
}

impl Emotion {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Neutral => "neutral",
            Self::Joy => "joy",
            Self::Sadness => "sadness",
            Self::Anger => "anger",
            Self::Surprise => "surprise",
            Self::Reflection => "reflection",
            Self::Tenderness => "tenderness",
        }
    }
}

pub const PROMPT: &str = "\nAvatar expression: start your reply with exactly one hidden marker [[emotion:STATE]], then your normal spoken answer. STATE must be neutral (everyday discussion/information), joy (happiness/enthusiasm/amusement/satisfaction), sadness (sorrow/melancholy/empathy), anger (annoyance/indignation/frustration), surprise (unexpected discovery/astonishment), reflection (doubt/curiosity/hesitation/perplexity), or tenderness (love/affection). Choose the tone of YOUR reply, not merely a word in the user's message. Prefer neutral when unsure. Use these exact English identifiers regardless of the conversation language. Never explain the marker or repeat it in the answer.";

/// Recognizes only leading metadata, across arbitrary streaming boundaries.
/// No marker is required: ordinary text is released as soon as it cannot match.
/// Malformed reserved headers are discarded with a bounded buffer; prose and
/// quoted markers inside the answer are otherwise left untouched.
#[derive(Default)]
pub struct ExpressionStream {
    pending: String,
    body: bool,
    header: bool,
    oversized: bool,
    closing: bool,
    pub emotion: Emotion,
}

impl ExpressionStream {
    pub fn push(&mut self, text: &str) -> String {
        const PREFIX: &str = "[[emotion:";
        if self.body {
            return text.into();
        }
        let mut output = String::new();
        for c in text.chars() {
            if self.body {
                output.push(c);
                continue;
            }
            if !self.header {
                if self.pending.is_empty() && (c.is_whitespace() || c == '\u{feff}') {
                    continue;
                }
                self.pending.push(c);
                if self.pending == PREFIX {
                    self.pending.clear();
                    self.header = true;
                } else if !PREFIX.starts_with(&self.pending) {
                    output.push_str(&std::mem::take(&mut self.pending));
                    self.body = true;
                }
            } else if c == '\n' || c == '\r' || (c == ']' && self.closing) {
                if c == ']' && !self.oversized {
                    let value = self
                        .pending
                        .trim_end_matches(']')
                        .trim()
                        .to_ascii_lowercase();
                    self.emotion = serde_json::from_value(serde_json::Value::String(value))
                        .unwrap_or_default();
                }
                self.pending.clear();
                self.body = true;
            } else {
                self.closing = c == ']';
                if self.pending.len() < 48 {
                    self.pending.push(c);
                } else {
                    self.oversized = true;
                }
            }
        }
        output
    }

    pub fn finish(&mut self) -> String {
        self.body = true;
        // A cut-off reserved header must never reach speech or history.
        if self.header || self.pending.starts_with("[[") {
            self.pending.clear();
        }
        std::mem::take(&mut self.pending)
    }
}
