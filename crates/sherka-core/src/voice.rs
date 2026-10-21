use crate::{Error, Result};
use serde::{Deserialize, Serialize};

/// Keep the beginning of a word while the recurrent VAD confirms its onset.
/// This adds no wait to the detector or end-of-turn timer; RAM only.
pub const PRE_ROLL_MS: usize = 800;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct AudioSettings {
    pub input_device: Option<String>,
    pub output_device: Option<String>,
    pub speakers: bool,
    pub silence_ms: u16,
    pub onset_ms: u16,
    pub interruption_ms: u16,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            input_device: None,
            output_device: None,
            speakers: true,
            silence_ms: 700,
            onset_ms: 100,
            interruption_ms: 100,
        }
    }
}

impl AudioSettings {
    pub fn validate(&self) -> Result<()> {
        if !(300..=2000).contains(&self.silence_ms)
            || !(50..=300).contains(&self.onset_ms)
            || !(50..=300).contains(&self.interruption_ms)
            || self
                .input_device
                .iter()
                .chain(self.output_device.iter())
                .any(|s| s.len() > 2048 || s.is_empty())
        {
            return Err(Error::Validation(
                "Les réglages du microphone sont invalides.".into(),
            ));
        }
        Ok(())
    }
}

/// Endpointing is independent from recognition. A provider's interim/final
/// messages never stop capture or create a second logical turn.
pub struct Endpoint {
    settings: AudioSettings,
    active: bool,
    onset: u16,
    silence: u16,
    speech_run: u16,
}

/// Speech needed to postpone the end of a turn. A lone VAD window (a breath,
/// click or lip noise) keeps the silence countdown running.
const SUSTAINED_SPEECH_MS: u16 = 60;

#[derive(Debug, PartialEq)]
pub enum Edge {
    None,
    Start,
    End,
}

impl Endpoint {
    pub fn new(settings: AudioSettings) -> Self {
        Self {
            settings,
            active: false,
            onset: 0,
            silence: 0,
            speech_run: 0,
        }
    }
    pub fn reset(&mut self) {
        self.active = false;
        self.onset = 0;
        self.silence = 0;
        self.speech_run = 0;
    }
    pub fn active(&self) -> bool {
        self.active
    }
    /// One call per 10 ms, after the local speech and echo decisions.
    pub fn feed(&mut self, speech: bool, responding: bool) -> Edge {
        if !self.active {
            self.onset = if speech { self.onset + 10 } else { 0 };
            let threshold = if responding {
                self.settings.interruption_ms
            } else {
                self.settings.onset_ms
            };
            if self.onset >= threshold {
                self.active = true;
                self.silence = 0;
                self.speech_run = 0;
                return Edge::Start;
            }
        } else {
            self.speech_run = if speech { self.speech_run + 10 } else { 0 };
            self.silence = if self.speech_run >= SUSTAINED_SPEECH_MS {
                0
            } else {
                self.silence + 10
            };
            if self.silence >= self.settings.silence_ms {
                self.reset();
                return Edge::End;
            }
        }
        Edge::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn short_noise_does_not_interrupt_and_natural_pauses_stay_in_one_turn() {
        let mut e = Endpoint::new(AudioSettings::default());
        for _ in 0..8 {
            assert_eq!(e.feed(true, true), Edge::None);
        }
        assert_eq!(e.feed(false, true), Edge::None);
        for _ in 0..9 {
            assert_eq!(e.feed(true, true), Edge::None);
        }
        assert_eq!(e.feed(true, true), Edge::Start);
        for _ in 0..60 {
            assert_eq!(e.feed(false, false), Edge::None);
        }
        for _ in 0..6 {
            assert_eq!(e.feed(true, false), Edge::None);
        }
        for _ in 0..69 {
            assert_eq!(e.feed(false, false), Edge::None);
        }
        assert_eq!(e.feed(false, false), Edge::End);
    }

    #[test]
    fn a_breath_or_click_after_speech_does_not_postpone_the_end_of_turn() {
        let mut e = Endpoint::new(AudioSettings::default());
        for _ in 0..10 {
            e.feed(true, false);
        }
        assert!(e.active());
        for _ in 0..40 {
            assert_eq!(e.feed(false, false), Edge::None);
        }
        // One 32 ms VAD window marks about three 10 ms frames as speech.
        for _ in 0..3 {
            assert_eq!(e.feed(true, false), Edge::None);
        }
        for _ in 0..26 {
            assert_eq!(e.feed(false, false), Edge::None);
        }
        assert_eq!(e.feed(false, false), Edge::End);
    }
}
