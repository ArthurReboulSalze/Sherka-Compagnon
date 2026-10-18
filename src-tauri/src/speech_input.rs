//! Retain original PCM while ASR is flushing, so a resumed sentence can be
//! transcribed as one input instead of losing the cancelled first fragment.
use sherka_core::session::OperationTag;
use uuid::Uuid;
const MAX_SAMPLES: usize = 16_000 * 90;
#[derive(Default)]
pub struct PendingSpeech {
    data: Option<(Uuid, OperationTag, Vec<i16>)>,
}
impl PendingSpeech {
    pub fn start(
        &mut self,
        run: Uuid,
        previous: Option<OperationTag>,
        next: OperationTag,
        pcm: Vec<i16>,
    ) -> Option<Vec<i16>> {
        let mut full = self
            .data
            .take()
            .filter(|(r, t, _)| *r == run && Some(*t) == previous)
            .map(|(_, _, p)| p)
            .unwrap_or_default();
        if full.len() + pcm.len() > MAX_SAMPLES {
            return None;
        }
        full.extend(pcm);
        self.data = Some((run, next, full.clone()));
        Some(full)
    }
    pub fn append(&mut self, run: Uuid, tag: OperationTag, pcm: &[i16]) -> bool {
        if let Some((r, t, data)) = &mut self.data
            && *r == run
            && *t == tag
        {
            if data.len() + pcm.len() > MAX_SAMPLES {
                self.data = None;
                return false;
            }
            data.extend_from_slice(pcm);
        }
        true
    }
    pub fn finish(&mut self, tag: OperationTag) {
        if self.data.as_ref().is_some_and(|(_, t, _)| *t == tag) {
            self.data = None;
        }
    }
    pub fn clear(&mut self) {
        self.data = None;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use sherka_core::session::Session;
    #[test]
    fn resumed_asr_keeps_original_pcm_but_never_crosses_run_or_turn_boundaries() {
        let mut session = Session::new(Uuid::new_v4());
        session.capture_started();
        let a = session.begin_utterance().unwrap();
        let run = Uuid::new_v4();
        let mut p = PendingSpeech::default();
        assert_eq!(p.start(run, None, a, vec![1, 2]).unwrap(), vec![1, 2]);
        assert!(p.append(run, a, &[3]));
        let b = session.begin_utterance().unwrap();
        assert_eq!(
            p.start(run, Some(a), b, vec![4, 5]).unwrap(),
            vec![1, 2, 3, 4, 5]
        );
        p.finish(a);
        let c = session.begin_utterance().unwrap();
        assert_eq!(
            p.start(run, Some(b), c, vec![6]).unwrap(),
            vec![1, 2, 3, 4, 5, 6]
        );
        p.finish(c);
        assert_eq!(p.start(run, Some(c), c, vec![7]).unwrap(), vec![7]);
        assert_eq!(
            p.start(Uuid::new_v4(), Some(c), c, vec![8]).unwrap(),
            vec![8]
        );
    }
}
