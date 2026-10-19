//! Local, bounded emotional carry-over. No timer, model call or feedback into
//! the language-model prompt: one completed exchange advances the reservoir.
use crate::{Result, avatar::Emotion, storage::Store};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const EMOTIONS: [Emotion; 6] = [
    Emotion::Joy,
    Emotion::Sadness,
    Emotion::Anger,
    Emotion::Surprise,
    Emotion::Reflection,
    Emotion::Tenderness,
];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub scores: [u16; 6],
    pub dominant: Emotion,
    last_turn: Option<Uuid>,
    last_signal: Emotion,
    repeats: u8,
}

impl State {
    pub fn advance(&mut self, turn: Uuid, user: &str, expression: Emotion) -> bool {
        if self.last_turn == Some(turn) {
            return false;
        }
        self.last_turn = Some(turn);
        for score in &mut self.scores {
            *score = ((*score).min(80) * 90 / 100).saturating_sub(4);
        }
        let cue = user_cue(user);
        let signal = cue.unwrap_or(expression);
        self.repeats = if signal != Emotion::Neutral && signal == self.last_signal {
            self.repeats.saturating_add(1)
        } else {
            0
        };
        self.last_signal = signal;
        // Repeating a persona's angry/sad wording without fresh user evidence
        // must eventually stop reinforcing it, even with a poor avatar tagger.
        let reward = if cue.is_some() {
            24
        } else {
            match self.repeats {
                0 => 24,
                1 => 12,
                2 => 6,
                _ => 0,
            }
        };
        if let Some(index) = EMOTIONS.iter().position(|&e| e == signal) {
            for (other, score) in EMOTIONS.iter().zip(&mut self.scores) {
                let release = match (signal, other) {
                    (Emotion::Joy | Emotion::Tenderness, Emotion::Anger) => 22,
                    (Emotion::Joy | Emotion::Tenderness, Emotion::Sadness) => 14,
                    (Emotion::Anger, Emotion::Joy | Emotion::Tenderness) => 18,
                    _ if *other != signal => 4,
                    _ => 0,
                };
                if reward > 0 {
                    *score = score.saturating_sub(release);
                }
            }
            self.scores[index] = (self.scores[index] + reward).min(80);
        }
        // A common budget prevents all six gauges from saturating together.
        let total: u16 = self.scores.iter().sum();
        if total > 100 {
            for score in &mut self.scores {
                *score = *score * 100 / total;
            }
        }
        let (index, &best) = self
            .scores
            .iter()
            .enumerate()
            .max_by_key(|&(_, value)| value)
            .unwrap();
        let current = EMOTIONS
            .iter()
            .position(|&e| e == self.dominant)
            .map_or(0, |i| self.scores[i]);
        if best < 8 {
            self.dominant = Emotion::Neutral;
        } else if current < 8 || best >= current + 12 {
            self.dominant = EMOTIONS[index];
        }
        true
    }
}

/// Ground only the latest unquoted user input. These cues help small models
/// and let kindness de-escalate an angry reply without another LLM request.
// i18n: data-start
fn user_cue(user: &str) -> Option<Emotion> {
    let text = user.to_lowercase().replace('’', "'");
    if [
        "traduis",
        "translate",
        "traduce",
        "traduci",
        "übersetz",
        "翻译",
        "翻訳",
        "si ",
        "if ",
    ]
    .iter()
    .any(|p| text.trim_start().starts_with(p))
    {
        return None;
    }
    let mut plain = String::new();
    let mut quote = None;
    for c in text.chars() {
        if let Some(end) = quote {
            if c == end {
                quote = None;
            }
        } else if matches!(c, '"' | '`' | '«' | '“' | '「') {
            quote = Some(match c {
                '«' => '»',
                '“' => '”',
                '「' => '」',
                _ => c,
            });
            plain.push(' ');
        } else {
            plain.push(c);
        }
    }
    let groups: &[(Emotion, &[&str])] = &[
        (
            Emotion::Tenderness,
            &[
                "je t'aime",
                "je t'adore",
                "avec tendresse",
                "i love you",
                "i adore you",
                "te quiero",
                "te amo",
                "ti voglio bene",
                "ti amo",
                "ich liebe dich",
                "我爱你",
                "我愛你",
                "愛してる",
                "大好き",
            ],
        ),
        (
            Emotion::Joy,
            &[
                "tu es super",
                "t'es super",
                "tu es génial",
                "t'es génial",
                "merci pour ton aide",
                "tu es formidable",
                "excuse-moi",
                "je m'excuse",
                "you are wonderful",
                "you're wonderful",
                "you are great",
                "you're great",
                "you are amazing",
                "thank you for your help",
                "i appreciate you",
                "i apologize",
                "eres genial",
                "muchas gracias",
                "sei fantastico",
                "sei fantastica",
                "grazie mille",
                "du bist toll",
                "vielen dank",
                "你真棒",
                "谢谢你",
                "謝謝你",
                "ありがとう",
                "素晴らしい",
            ],
        ),
        (
            Emotion::Anger,
            &[
                "tu es nul",
                "t'es nul",
                "tu es stupide",
                "t'es stupide",
                "tu es idiot",
                "t'es idiot",
                "je te déteste",
                "you are stupid",
                "you're stupid",
                "you are useless",
                "you're useless",
                "i hate you",
                "eres inútil",
                "eres estupido",
                "eres estúpido",
                "sei stupido",
                "sei stupida",
                "du bist dumm",
                "du bist nutzlos",
                "你真笨",
                "你很蠢",
                "役立たず",
                "お前は馬鹿",
            ],
        ),
        (
            Emotion::Sadness,
            &[
                "je suis triste",
                "je me sens triste",
                "j'ai perdu mon",
                "i feel sad",
                "i am sad",
                "i'm sad",
                "estoy triste",
                "sono triste",
                "ich bin traurig",
                "我很难过",
                "我很難過",
                "悲しいです",
            ],
        ),
    ];
    groups
        .iter()
        .find(|(_, phrases)| phrases.iter().any(|p| plain.contains(p)))
        .map(|(emotion, _)| *emotion)
}
// i18n: data-end

pub struct Stored {
    pub epoch: i64,
    pub state: State,
}

impl Store {
    pub fn mood(&self, profile: Uuid) -> Result<Stored> {
        let db = self.db(profile)?;
        let (epoch, json): (i64, String) = db.query_row(
            "SELECT epoch,data FROM mood_state WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        Ok(Stored {
            epoch,
            state: serde_json::from_str(&json)?,
        })
    }
    pub fn reset_mood(&self, profile: Uuid) -> Result<Stored> {
        let db = self.db(profile)?;
        db.execute(
            "UPDATE mood_state SET epoch=epoch+1,data='{}' WHERE singleton=1",
            [],
        )?;
        self.mood(profile)
    }
    pub fn advance_mood(
        &self,
        profile: Uuid,
        epoch: i64,
        turn: Uuid,
        user: &str,
        expression: Emotion,
    ) -> Result<Emotion> {
        let profile_data = self.profile(profile)?;
        if !profile_data.details.mood_persistence
            || profile_data.details.connections.llm_provider.is_agent()
        {
            return Ok(Emotion::Neutral);
        }
        let mut db = self.db(profile)?;
        let tx = db.transaction()?;
        let (current, json): (i64, String) = tx.query_row(
            "SELECT epoch,data FROM mood_state WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let mut state: State = serde_json::from_str(&json)?;
        if current == epoch && state.advance(turn, user, expression) {
            tx.execute(
                "UPDATE mood_state SET data=?1 WHERE singleton=1 AND epoch=?2",
                params![serde_json::to_string(&state)?, epoch],
            )?;
        }
        tx.commit()?;
        Ok(state.dominant)
    }
}
