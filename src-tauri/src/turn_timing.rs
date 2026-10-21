//! Per-turn latency marks in milliseconds, measured from the end of the user's
//! speech (or from the start of a typed or spontaneous turn). Durations only:
//! no text, model, device or endpoint is recorded.
use serde::Serialize;
use std::{
    sync::atomic::{AtomicU32, Ordering},
    time::Instant,
};

#[derive(Clone, Copy)]
pub enum Mark {
    /// Final transcript accepted and the turn started (voice only).
    Transcribed,
    /// Context, memories, web evidence and model ready; request about to be sent.
    Prepared,
    FirstToken,
    /// First passage handed to speech synthesis.
    FirstPassage,
    /// First synthesized audio started playing.
    FirstAudio,
    TextComplete,
}
const MARKS: usize = 6;
const UNSET: u32 = u32::MAX;

pub struct TurnClock {
    origin: Instant,
    voice: bool,
    marks: [AtomicU32; MARKS],
}

#[derive(Serialize, Debug, PartialEq)]
pub struct TurnTiming {
    pub voice: bool,
    pub transcribed_ms: Option<u32>,
    pub prepared_ms: Option<u32>,
    pub first_token_ms: Option<u32>,
    pub first_passage_ms: Option<u32>,
    pub first_audio_ms: Option<u32>,
    pub text_complete_ms: Option<u32>,
}

impl TurnClock {
    /// `speech_end` is the endpoint of the utterance that produced this turn.
    pub fn new(speech_end: Option<Instant>) -> Self {
        Self {
            origin: speech_end.unwrap_or_else(Instant::now),
            voice: speech_end.is_some(),
            marks: std::array::from_fn(|_| AtomicU32::new(UNSET)),
        }
    }

    /// Only the first occurrence of each mark counts.
    pub fn mark(&self, mark: Mark) {
        let ms = u32::try_from(self.origin.elapsed().as_millis()).unwrap_or(UNSET - 1);
        let _ = self.marks[mark as usize].compare_exchange(
            UNSET,
            ms.min(UNSET - 1),
            Ordering::Relaxed,
            Ordering::Relaxed,
        );
    }

    pub fn summary(&self) -> TurnTiming {
        let get = |mark: Mark| {
            Some(self.marks[mark as usize].load(Ordering::Relaxed)).filter(|ms| *ms != UNSET)
        };
        TurnTiming {
            voice: self.voice,
            transcribed_ms: get(Mark::Transcribed),
            prepared_ms: get(Mark::Prepared),
            first_token_ms: get(Mark::FirstToken),
            first_passage_ms: get(Mark::FirstPassage),
            first_audio_ms: get(Mark::FirstAudio),
            text_complete_ms: get(Mark::TextComplete),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_are_relative_to_speech_end_and_recorded_once() {
        let end = Instant::now() - std::time::Duration::from_millis(500);
        let clock = TurnClock::new(Some(end));
        clock.mark(Mark::Transcribed);
        let first = clock.summary().transcribed_ms.unwrap();
        assert!(first >= 500);
        std::thread::sleep(std::time::Duration::from_millis(5));
        clock.mark(Mark::Transcribed);
        let summary = clock.summary();
        assert_eq!(summary.transcribed_ms, Some(first));
        assert!(summary.voice);
        assert_eq!(summary.first_audio_ms, None);
        let typed = TurnClock::new(None).summary();
        assert!(!typed.voice);
        assert_eq!(typed.prepared_ms, None);
    }
}
