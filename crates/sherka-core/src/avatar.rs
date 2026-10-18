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

pub const PROMPT: &str = "\nBegin the FINAL answer (after any private thinking) with ONE avatar tag. Copy this format exactly: [[emotion:joy]] Your normal answer. Choose one: neutral=ordinary facts; joy=happy/excited/amused; sadness=sad/empathy; anger=annoyed/indignant; surprise=astonished; reflection=unsure/curious; tenderness=love/affection. Examples: [[emotion:joy]] Wonderful news! / [[emotion:sadness]] I'm sorry you're going through this. / [[emotion:reflection]] I'm not sure yet. / [[emotion:neutral]] The Moon orbits Earth. Use the tone of YOUR answer. Keep the tag in English even when speaking another language. Then answer naturally in the requested language; never explain the tag.";

/// Some local templates return reasoning in content instead of a separate API
/// field. Strip only a leading reasoning block, across arbitrary chunk splits.
#[derive(Default)]
pub struct AnswerStream {
    pending: String,
    closing: Option<&'static str>,
    body: bool,
}
impl AnswerStream {
    pub fn push(&mut self, text: &str) -> String {
        let mut out = String::new();
        for c in text.chars() {
            if self.body {
                out.push(c);
                continue;
            }
            if let Some(close) = self.closing {
                self.pending.push(c);
                if self.pending.ends_with(close) {
                    self.pending.clear();
                    self.closing = None;
                } else if self.pending.len() > close.len() * 2 {
                    let start = self
                        .pending
                        .char_indices()
                        .rev()
                        .nth(close.len())
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    self.pending.drain(..start);
                }
                continue;
            }
            if self.pending.is_empty() && (c.is_whitespace() || c == '\u{feff}') {
                continue;
            }
            self.pending.push(c);
            let pairs = [("<think>", "</think>"), ("<analysis>", "</analysis>")];
            if let Some((_, close)) = pairs.iter().find(|(open, _)| *open == self.pending) {
                self.closing = Some(close);
                self.pending.clear();
            } else if !pairs
                .iter()
                .any(|(open, _)| open.starts_with(&self.pending))
            {
                out.push_str(&std::mem::take(&mut self.pending));
                self.body = true;
            }
        }
        out
    }
}

fn emotion_value(value: &str) -> Option<Emotion> {
    use Emotion::*;
    Some(match value.trim().to_lowercase().as_str() {
        "neutral" | "neutre" | "calm" => Neutral,
        "joy" | "happy" | "happiness" | "joie" | "bonheur" => Joy,
        "sadness" | "sad" | "tristesse" => Sadness,
        "anger" | "angry" | "colère" | "colere" => Anger,
        "surprise" | "surprised" => Surprise,
        "reflection" | "thinking" | "curious" | "réflexion" | "reflexion" => Reflection,
        "tenderness" | "love" | "affection" | "tendresse" => Tenderness,
        _ => return None,
    })
}

/// Conservative fallback for models that omit the protocol. Match the tone of
/// the visible answer, never the user's keywords or the model's private thought.
// i18n: data-start
pub fn fallback_emotion(answer: &str) -> Emotion {
    use Emotion::*;
    let text = answer.to_lowercase().replace('’', "'");
    let groups: &[(Emotion, &[&str])] = &[
        (
            Tenderness,
            &[
                "je t'aime",
                "avec tendresse",
                "mon amour",
                "i love you",
                "with affection",
                "te quiero",
                "con cariño",
                "ti voglio bene",
                "con affetto",
                "ich liebe dich",
                "liebevoll",
                "我爱你",
                "温柔地",
                "愛して",
                "愛おしい",
                "🥰",
                "❤️",
            ],
        ),
        (
            Sadness,
            &[
                "je suis désolé",
                "je suis désolée",
                "c'est triste",
                "ça me peine",
                "de tout cœur avec toi",
                "i'm sorry",
                "i am sorry",
                "that is sad",
                "my condolences",
                "lo siento",
                "qué tristeza",
                "mi dispiace",
                "che tristezza",
                "es tut mir leid",
                "das ist traurig",
                "很难过",
                "很遗憾",
                "悲しい",
                "お悔やみ",
                "😢",
            ],
        ),
        (
            Anger,
            &[
                "ça m'énerve",
                "c'est inadmissible",
                "quelle injustice",
                "ça m'agace",
                "this is outrageous",
                "i'm furious",
                "so frustrating",
                "me enfada",
                "es indignante",
                "mi fa arrabbiare",
                "è inaccettabile",
                "das ärgert mich",
                "das ist empörend",
                "太气人",
                "很生气",
                "腹が立つ",
                "許せない",
                "😠",
            ],
        ),
        (
            Surprise,
            &[
                "quelle surprise",
                "je ne m'y attendais pas",
                "c'est stupéfiant",
                "what a surprise",
                "i didn't expect",
                "that's astonishing",
                "qué sorpresa",
                "che sorpresa",
                "was für eine überraschung",
                "真没想到",
                "太意外",
                "びっくり",
                "驚きました",
                "😮",
            ],
        ),
        (
            Joy,
            &[
                "quelle bonne nouvelle",
                "quelle excellente nouvelle",
                "je suis ravi",
                "ça me fait plaisir",
                "c'est génial",
                "haha",
                "wonderful news",
                "great news",
                "i'm delighted",
                "that's fantastic",
                "qué alegría",
                "me alegra",
                "che bella notizia",
                "sono felice",
                "das freut mich",
                "wunderbare neuigkeiten",
                "太开心",
                "太棒了",
                "嬉しい",
                "やった",
                "😊",
                "😄",
                "😂",
            ],
        ),
        (
            Reflection,
            &[
                "je me demande",
                "je ne suis pas sûr",
                "je ne suis pas sûre",
                "laisse-moi réfléchir",
                "réfléchissons",
                "i wonder",
                "i'm not sure",
                "let me think",
                "no estoy seguro",
                "me pregunto",
                "non sono sicuro",
                "mi chiedo",
                "ich bin mir nicht sicher",
                "ich frage mich",
                "我不确定",
                "让我想想",
                "かもしれません",
                "考えてみ",
                "🤔",
            ],
        ),
    ];
    groups
        .iter()
        .find(|(_, phrases)| phrases.iter().any(|p| text.contains(p)))
        .map(|(emotion, _)| *emotion)
        .unwrap_or_default()
}
// i18n: data-end

/// Recognizes only leading metadata, across arbitrary streaming boundaries.
/// No marker is required: ordinary text is released as soon as it cannot match.
/// Malformed reserved headers are discarded with a bounded buffer; prose and
/// quoted markers inside the answer are otherwise left untouched.
#[derive(Default)]
pub struct ExpressionStream {
    answer: AnswerStream,
    visible: String,
    pub explicit: bool,
    pending: String,
    body: bool,
    header: bool,
    oversized: bool,
    closing: bool,
    pub emotion: Emotion,
}

impl ExpressionStream {
    pub fn push(&mut self, text: &str) -> String {
        let text = self.answer.push(text);
        let out = self.push_expression(&text);
        if !self.explicit && self.visible.len() < 4096 {
            self.visible.extend(out.chars().take(1024));
            self.emotion = fallback_emotion(&self.visible);
        }
        out
    }
    fn push_expression(&mut self, text: &str) -> String {
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
                if self.pending.to_ascii_lowercase() == PREFIX {
                    self.pending.clear();
                    self.header = true;
                } else if !PREFIX.starts_with(&self.pending.to_ascii_lowercase()) {
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
                    if let Some(emotion) = emotion_value(&value) {
                        self.emotion = emotion;
                        self.explicit = true;
                    }
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
