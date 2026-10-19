use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct OperationTag {
    pub profile_id: Uuid,
    pub session_id: Uuid,
    pub turn_id: Uuid,
    pub generation_id: Uuid,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MicrophoneState {
    Off,
    Listening,
    Unavailable,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptionState {
    Idle,
    Capturing,
    Transcribing,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResponseState {
    Idle,
    Generating,
    Speaking,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionSnapshot {
    pub profile_id: Uuid,
    pub session_id: Uuid,
    pub microphone: MicrophoneState,
    pub transcription: TranscriptionState,
    pub response: ResponseState,
    pub voice_paused: bool,
}

/// The one authority for profile/session/turn/generation identity. Cancellation
/// tokens are advisory; every consumer must also call accepts before applying
/// a late STT/LLM/TTS result, an audio chunk or a UI update.
pub struct Session {
    initiative_paused: bool,
    state: SessionSnapshot,
    current: Option<OperationTag>,
    cancellation: CancellationToken,
    stt_final_accepted: bool,
}

impl Session {
    pub fn new(profile_id: Uuid) -> Self {
        Self {
            initiative_paused: false,
            state: SessionSnapshot {
                profile_id,
                session_id: Uuid::new_v4(),
                microphone: MicrophoneState::Off,
                transcription: TranscriptionState::Idle,
                response: ResponseState::Idle,
                voice_paused: true,
            },
            current: None,
            cancellation: CancellationToken::new(),
            stt_final_accepted: false,
        }
    }

    pub fn snapshot(&self) -> SessionSnapshot {
        self.state.clone()
    }

    pub fn cancellation(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    pub fn accepts(&self, tag: OperationTag) -> bool {
        self.current == Some(tag) && !self.cancellation.is_cancelled()
    }

    /// Invalidates immediately. Audio queue flushing belongs in the backend,
    /// which must observe this cancellation before playback, not after STT.
    pub fn cancel(&mut self) {
        self.cancellation.cancel();
        self.current = None;
        self.state.transcription = TranscriptionState::Idle;
        self.state.response = ResponseState::Idle;
        self.stt_final_accepted = false;
    }

    pub fn switch_profile(&mut self, profile_id: Uuid) {
        self.cancel();
        self.state.profile_id = profile_id;
        self.state.session_id = Uuid::new_v4();
        // Global microphone consent is never changed by a profile switch.
    }

    pub fn initiative_paused(&self) -> bool {
        self.initiative_paused
    }
    pub fn resume_initiative(&mut self) {
        self.initiative_paused = false;
    }

    pub fn pause_voice(&mut self) {
        self.initiative_paused = true;
        self.cancel();
        self.state.microphone = MicrophoneState::Off;
        self.state.voice_paused = true;
    }

    pub fn begin_text_turn(&mut self) -> OperationTag {
        self.begin_turn(TranscriptionState::Idle, ResponseState::Generating)
    }

    /// Called only after a real backend has started capture with consent.
    pub fn capture_started(&mut self) {
        self.initiative_paused = false;
        self.state.microphone = MicrophoneState::Listening;
        self.state.voice_paused = false;
    }

    pub fn capture_failed(&mut self) {
        self.pause_voice();
        self.state.microphone = MicrophoneState::Unavailable;
    }

    pub fn complete(&mut self, tag: OperationTag) -> Result<()> {
        self.require_current(tag)?;
        self.cancel();
        Ok(())
    }

    pub fn begin_utterance(&mut self) -> Result<OperationTag> {
        if self.state.microphone != MicrophoneState::Listening {
            return Err(Error::Validation("Le microphone est coupé.".into()));
        }
        Ok(self.begin_turn(TranscriptionState::Capturing, ResponseState::Idle))
    }

    fn begin_turn(
        &mut self,
        transcription: TranscriptionState,
        response: ResponseState,
    ) -> OperationTag {
        self.cancel();
        self.cancellation = CancellationToken::new();
        let tag = OperationTag {
            profile_id: self.state.profile_id,
            session_id: self.state.session_id,
            turn_id: Uuid::new_v4(),
            generation_id: Uuid::new_v4(),
        };
        self.current = Some(tag);
        self.state.transcription = transcription;
        self.state.response = response;
        tag
    }

    pub fn finish_capture(&mut self, tag: OperationTag) -> Result<()> {
        self.require_current(tag)?;
        self.state.transcription = TranscriptionState::Transcribing;
        Ok(())
    }

    /// One finalized utterance per logical turn. A session summary cannot
    /// trigger a second response. Partial and blank results are not committed.
    pub fn accept_stt_final(&mut self, tag: OperationTag, text: &str) -> Result<Option<String>> {
        self.require_current(tag)?;
        if self.stt_final_accepted {
            return Ok(None);
        }
        if text.trim().is_empty() {
            self.cancel();
            return Ok(None);
        }
        self.stt_final_accepted = true;
        self.state.transcription = TranscriptionState::Idle;
        self.state.response = ResponseState::Generating;
        Ok(Some(text.trim().to_string()))
    }

    /// Actual playback acknowledgement, never triggered by a text token.
    pub fn playback_started(&mut self, tag: OperationTag) -> Result<()> {
        self.require_current(tag)?;
        self.state.response = ResponseState::Speaking;
        Ok(())
    }

    fn require_current(&self, tag: OperationTag) -> Result<()> {
        if self.accepts(tag) {
            Ok(())
        } else {
            Err(Error::StaleOperation)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interruption_rejects_every_late_event_and_cancels_workers() {
        let mut session = Session::new(Uuid::new_v4());
        session.capture_started();
        let old = session.begin_text_turn();
        let token = session.cancellation();
        session.playback_started(old).unwrap();
        assert_eq!(session.snapshot().microphone, MicrophoneState::Listening);
        let new = session.begin_utterance().unwrap();
        assert!(token.is_cancelled());
        assert!(!session.accepts(old));
        assert!(session.accepts(new));
        assert!(session.playback_started(old).is_err());
        assert!(session.accept_stt_final(old, "retard").is_err());
        assert_eq!(session.snapshot().response, ResponseState::Idle);
    }
    #[test]
    fn switching_away_and_back_does_not_revive_generation_or_microphone() {
        let a = Uuid::new_v4();
        let mut session = Session::new(a);
        let tag = session.begin_text_turn();
        session.switch_profile(Uuid::new_v4());
        session.switch_profile(a);
        assert!(!session.accepts(tag));
        assert_ne!(tag.session_id, session.snapshot().session_id);
        assert_eq!(session.snapshot().microphone, MicrophoneState::Off);
    }
    #[test]
    fn short_words_are_accepted_once_and_empty_interruption_stays_stopped() {
        let mut session = Session::new(Uuid::new_v4());
        session.capture_started();
        let tag = session.begin_utterance().unwrap();
        assert_eq!(
            session.accept_stt_final(tag, " oui ").unwrap(),
            Some("oui".into())
        );
        assert_eq!(
            session.accept_stt_final(tag, "résumé de session").unwrap(),
            None
        );
        session.playback_started(tag).unwrap();
        let empty = session.begin_utterance().unwrap();
        assert_eq!(session.accept_stt_final(empty, " ").unwrap(), None);
        assert!(!session.accepts(tag));
        assert_eq!(session.snapshot().response, ResponseState::Idle);
    }
    #[test]
    fn pause_invalidates_capture_and_output() {
        let mut session = Session::new(Uuid::new_v4());
        session.capture_started();
        let tag = session.begin_utterance().unwrap();
        session.pause_voice();
        assert!(!session.accepts(tag));
        assert!(session.begin_utterance().is_err());
        assert_eq!(session.snapshot().transcription, TranscriptionState::Idle);
    }
}
