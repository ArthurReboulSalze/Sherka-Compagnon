//! Silero VAD 6.2, embedded and CPU-only. No device access or network calls.
//! 32 ms model windows, with the recurrent state and 64-sample context required
//! by the upstream ONNX wrapper. WebRTC's VAD hangover is not voice evidence.
use crate::{Error, Result};
use ort::{session::Session, value::TensorRef};

pub struct SpeechDetector {
    session: Session,
    state: [f32; 256],
    input: [f32; 576],
    used: usize,
    speech: bool,
}

fn failure(_: ort::Error) -> Error {
    Error::Validation("La détection de parole locale a échoué. Réactive le micro.".into())
}

impl SpeechDetector {
    pub fn new() -> Result<Self> {
        let session = Session::builder()
            .and_then(|s| s.with_intra_threads(1))
            .and_then(|s| s.with_inter_threads(1))
            .and_then(|s| s.commit_from_memory(include_bytes!("../models/silero_vad.onnx")))
            .map_err(failure)?;
        Ok(Self {
            session,
            state: [0.0; 256],
            input: [0.0; 576],
            used: 0,
            speech: false,
        })
    }

    pub fn feed(&mut self, samples: &[f32]) -> Result<bool> {
        for &sample in samples {
            self.input[64 + self.used] = sample;
            self.used += 1;
            if self.used == 512 {
                let outputs = self.session.run(ort::inputs! {
                    "input" => TensorRef::from_array_view(([1, 576], &self.input[..])).map_err(failure)?,
                    "state" => TensorRef::from_array_view(([2, 1, 128], &self.state[..])).map_err(failure)?,
                    "sr" => TensorRef::from_array_view((Vec::<i64>::new(), &[16000_i64][..])).map_err(failure)?
                }).map_err(failure)?;
                let (_, probability) = outputs["output"]
                    .try_extract_tensor::<f32>()
                    .map_err(failure)?;
                let (_, state) = outputs["stateN"]
                    .try_extract_tensor::<f32>()
                    .map_err(failure)?;
                self.speech = probability[0] >= if self.speech { 0.4 } else { 0.65 };
                self.state.copy_from_slice(state);
                self.input.copy_within(512..576, 0);
                self.used = 0;
            }
        }
        Ok(self.speech)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voice::{AudioSettings, Edge, Endpoint};

    #[test]
    fn silence_hum_and_intermittent_clicks_do_not_interrupt() {
        let mut detector = SpeechDetector::new().unwrap();
        let mut endpoint = Endpoint::new(AudioSettings::default());
        for frame in 0..1200 {
            let samples: Vec<_> = (0..160)
                .map(|i| {
                    let at = frame * 160 + i;
                    let hum = (at as f32 * std::f32::consts::TAU * 60.0 / 16000.0).sin() * 0.006;
                    let noise =
                        (((at as u32).wrapping_mul(747_796_405) >> 8) as f32 / 16_777_216.0 - 0.5)
                            * 0.005;
                    let click = if at % 8000 < 80 {
                        0.2 * (at as f32).sin()
                    } else {
                        0.0
                    };
                    if frame < 100 {
                        0.0
                    } else {
                        hum + noise + click
                    }
                })
                .collect();
            let speech = detector.feed(&samples).unwrap();
            assert_ne!(
                endpoint.feed(speech, true),
                Edge::Start,
                "false start at {} ms",
                frame * 10
            );
        }
    }
}
