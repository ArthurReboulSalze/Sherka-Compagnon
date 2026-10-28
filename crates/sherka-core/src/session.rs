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
    /// The guest companion of a three-way conversation, for this session only.
    pub guest_profile_id: Option<Uuid>,
}

/// The one authority for profile/session/turn/generation identity. Cancellation
/// tokens are advisory; every consumer must also call accepts before applying
/// a late STT/LLM/TTS result, an audio chunk or a UI update.
pub struct Session {
    initiative_paused: bool,
    state: SessionSnapshot,
    current: Option<OperationTag>,
    /// The next speaker, generating ahead while `current` still speaks.
    pending: Option<OperationTag>,
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
                guest_profile_id: None,
            },
            current: None,
            pending: None,
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

    /// The prefetching next speaker of the turn. It owns no live state and is
    /// refused everywhere `accepts` is required until it is promoted.
    pub fn accepts_pending(&self, tag: OperationTag) -> bool {
        self.pending == Some(tag) && !self.cancellation.is_cancelled()
    }

    /// Invalidates immediately. Audio queue flushing belongs in the backend,
    /// which must observe this cancellation before playback, not after STT.
    pub fn cancel(&mut self) {
        self.cancellation.cancel();
        self.current = None;
        self.pending = None;
        self.state.transcription = TranscriptionState::Idle;
        self.state.response = ResponseState::Idle;
        self.stt_final_accepted = false;
    }

    pub fn switch_profile(&mut self, profile_id: Uuid) {
        self.cancel();
        self.state.profile_id = profile_id;
        self.state.session_id = Uuid::new_v4();
        self.state.guest_profile_id = None;
        // Global microphone consent is never changed by a profile switch.
    }

    pub fn set_guest(&mut self, id: Option<Uuid>) {
        self.state.guest_profile_id = id;
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

    /// Microphone off without cancelling a reply in progress.
    pub fn mute_microphone(&mut self) {
        self.initiative_paused = true;
        self.state.microphone = MicrophoneState::Off;
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

    /// The next speaker of the same turn: same session, turn and cancellation
    /// token, so one barge-in cancels every speaker. The previous tag stops
    /// being accepted.
    pub fn begin_peer(&mut self, after: OperationTag, profile_id: Uuid) -> Result<OperationTag> {
        self.require_current(after)?;
        let tag = Self::peer_tag(after, profile_id);
        self.current = Some(tag);
        self.pending = None;
        self.state.transcription = TranscriptionState::Idle;
        self.state.response = ResponseState::Generating;
        Ok(tag)
    }

    /// Registers the next speaker without replacing the current one, so it can
    /// prepare while `after` still speaks. Same turn and cancellation token;
    /// `current`, the response state and late events of `after` are untouched.
    pub fn begin_pending_peer(
        &mut self,
        after: OperationTag,
        profile_id: Uuid,
    ) -> Result<OperationTag> {
        self.require_current(after)?;
        let tag = Self::peer_tag(after, profile_id);
        self.pending = Some(tag);
        Ok(tag)
    }

    /// The pending speaker takes over: what `begin_peer` does at once.
    pub fn promote_pending(&mut self, tag: OperationTag) -> Result<()> {
        let same_turn = self
            .current
            .is_some_and(|c| (c.session_id, c.turn_id) == (tag.session_id, tag.turn_id));
        if !self.accepts_pending(tag) || !same_turn {
            return Err(Error::StaleOperation);
        }
        self.current = Some(tag);
        self.pending = None;
        self.state.transcription = TranscriptionState::Idle;
        self.state.response = ResponseState::Generating;
        Ok(())
    }

    fn peer_tag(after: OperationTag, profile_id: Uuid) -> OperationTag {
        OperationTag {
            profile_id,
            session_id: after.session_id,
            turn_id: after.turn_id,
            generation_id: Uuid::new_v4(),
        }
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
    fn muting_the_microphone_lets_a_spoken_reply_finish() {
        let mut session = Session::new(Uuid::new_v4());
        session.capture_started();
        let utterance = session.begin_utterance().unwrap();
        session.finish_capture(utterance).unwrap();
        session
            .accept_stt_final(utterance, "Raconte-moi une histoire")
            .unwrap();
        let token = session.cancellation();
        session.playback_started(utterance).unwrap();
        session.mute_microphone();
        assert_eq!(session.snapshot().microphone, MicrophoneState::Off);
        assert!(!token.is_cancelled());
        assert!(session.accepts(utterance));
        assert_eq!(session.snapshot().response, ResponseState::Speaking);
        // No new utterance can start while muted; the reply still completes.
        assert!(session.begin_utterance().is_err());
        session.complete(utterance).unwrap();
        // Global pause, unlike mute, still stops a reply.
        session.capture_started();
        let other = session.begin_text_turn();
        let token = session.cancellation();
        session.pause_voice();
        assert!(token.is_cancelled());
        assert!(!session.accepts(other));
    }
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
    fn a_second_speaker_shares_the_turn_and_its_cancellation() {
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let mut session = Session::new(a);
        let first = session.begin_text_turn();
        let token = session.cancellation();
        let second = session.begin_peer(first, b).unwrap();
        assert_eq!(second.profile_id, b);
        assert_eq!(
            (second.session_id, second.turn_id),
            (first.session_id, first.turn_id)
        );
        assert_ne!(second.generation_id, first.generation_id);
        assert!(!session.accepts(first));
        assert!(session.accepts(second));
        assert!(!token.is_cancelled());
        assert_eq!(session.snapshot().response, ResponseState::Generating);
        // A stale tag cannot start another speaker, and complete returns to idle.
        assert!(session.begin_peer(first, b).is_err());
        session.complete(second).unwrap();
        assert!(token.is_cancelled());
        assert_eq!(session.snapshot().response, ResponseState::Idle);
        // A barge-in invalidates the new speaker; a cancelled turn refuses a peer.
        let first = session.begin_text_turn();
        let second = session.begin_peer(first, b).unwrap();
        session.cancel();
        assert!(!session.accepts(second));
        assert!(session.begin_peer(second, a).is_err());
    }
    #[test]
    fn a_pending_speaker_prepares_without_taking_over_until_promoted() {
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let mut session = Session::new(a);
        let first = session.begin_text_turn();
        let token = session.cancellation();
        let pending = session.begin_pending_peer(first, b).unwrap();
        assert_eq!(pending.profile_id, b);
        assert_eq!(
            (pending.session_id, pending.turn_id),
            (first.session_id, first.turn_id)
        );
        assert_ne!(pending.generation_id, first.generation_id);
        // The first speaker keeps every right; the pending one has none yet.
        assert!(session.accepts(first));
        assert!(!session.accepts(pending));
        assert!(session.accepts_pending(pending));
        assert!(!session.accepts_pending(first));
        session.playback_started(first).unwrap();
        assert_eq!(session.snapshot().response, ResponseState::Speaking);
        assert!(session.playback_started(pending).is_err());
        // A stale tag cannot register a pending speaker.
        assert!(session.begin_pending_peer(pending, a).is_err());
        session.promote_pending(pending).unwrap();
        assert!(session.accepts(pending));
        assert!(!session.accepts(first));
        assert!(!session.accepts_pending(pending));
        assert_eq!(session.snapshot().response, ResponseState::Generating);
        assert!(!token.is_cancelled());
        // Nothing is pending any more.
        assert!(session.promote_pending(pending).is_err());
        session.complete(pending).unwrap();
    }
    #[test]
    fn cancelling_or_replacing_the_turn_drops_the_pending_speaker() {
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let mut session = Session::new(a);
        // Barge-in.
        let first = session.begin_text_turn();
        let pending = session.begin_pending_peer(first, b).unwrap();
        session.cancel();
        assert!(!session.accepts_pending(pending));
        assert!(session.promote_pending(pending).is_err());
        assert!(!session.accepts(pending));
        // A new turn does not inherit the old pending speaker.
        let first = session.begin_text_turn();
        let old = session.begin_pending_peer(first, b).unwrap();
        let next = session.begin_text_turn();
        assert!(!session.accepts_pending(old));
        assert!(session.promote_pending(old).is_err());
        assert!(session.accepts(next));
        // Starting the next speaker the usual way replaces the pending one.
        let pending = session.begin_pending_peer(next, b).unwrap();
        let second = session.begin_peer(next, b).unwrap();
        assert!(!session.accepts_pending(pending));
        assert!(session.promote_pending(pending).is_err());
        assert!(session.accepts(second));
        // A pending speaker needs a live current speaker, and a token that is
        // still alive when it is promoted.
        session.complete(second).unwrap();
        assert!(session.begin_pending_peer(second, a).is_err());
        let first = session.begin_text_turn();
        let pending = session.begin_pending_peer(first, b).unwrap();
        session.pause_voice();
        assert!(session.promote_pending(pending).is_err());
    }
    #[test]
    fn the_guest_is_session_only() {
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let mut session = Session::new(a);
        assert_eq!(session.snapshot().guest_profile_id, None);
        session.set_guest(Some(b));
        assert_eq!(session.snapshot().guest_profile_id, Some(b));
        session.cancel();
        assert_eq!(session.snapshot().guest_profile_id, Some(b));
        session.switch_profile(b);
        assert_eq!(session.snapshot().guest_profile_id, None);
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
