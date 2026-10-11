//! Shared native capture processing, also exercised without opening devices.
use crate::{Error, Result, speech_detector::SpeechDetector};
use sonora::config::{EchoCanceller, NoiseSuppression};
use sonora::{AudioProcessing, Config, StreamConfig};
use std::collections::VecDeque;

pub struct CaptureFrame {
    pub pcm: Vec<i16>,
    pub speech: bool,
    pub level: f32,
    pub raw_vad: bool,
    pub attenuation_db: f32,
    pub echo_likelihood: f64,
    pub echo_match: f32,
}

pub struct Acoustics {
    apm: AudioProcessing,
    input: StreamConfig,
    output: StreamConfig,
    render_out: Vec<f32>,
    vad: webrtc_vad::Vad,
    speech_detector: SpeechDetector,
    echo_guard: Option<EchoGuard>,
}

impl Acoustics {
    pub fn new(input_rate: u32, render_rate: u32, speakers: bool) -> Result<Self> {
        let input = StreamConfig::new(input_rate, 1);
        let render = StreamConfig::new(render_rate, 1);
        Ok(Self {
            apm: AudioProcessing::builder()
                .config(Config {
                    echo_canceller: speakers.then(EchoCanceller::default),
                    noise_suppression: Some(NoiseSuppression::default()),
                    ..Default::default()
                })
                .capture_config(input)
                .render_config(render)
                .echo_detector(speakers)
                .build(),
            input,
            output: StreamConfig::new(16000, 1),
            render_out: vec![0.0; render_rate as usize / 100],
            vad: webrtc_vad::Vad::new_with_rate_and_mode(
                webrtc_vad::SampleRate::Rate16kHz,
                webrtc_vad::VadMode::Aggressive,
            ),
            echo_guard: speakers.then(EchoGuard::default),
            speech_detector: SpeechDetector::new()?,
        })
    }

    pub fn render(&mut self, samples: &[f32]) -> Result<()> {
        if let Some(guard) = &mut self.echo_guard {
            guard.render(samples);
        }
        self.apm
            .process_render_f32(&[samples], &mut [&mut self.render_out])
            .map_err(|_| Error::Validation("Référence d'écho audio invalide.".into()))
    }

    pub fn capture(&mut self, samples: &[f32], delay_ms: u32) -> Result<CaptureFrame> {
        let mut out = [0.0; 160];
        let _ = self.apm.set_stream_delay_ms(delay_ms.min(500) as i32);
        self.apm
            .process_capture_f32_with_config(&[samples], &self.input, &self.output, &mut [&mut out])
            .map_err(|_| Error::Validation("Traitement du microphone impossible.".into()))?;
        let pcm: Vec<i16> = out
            .iter()
            .map(|f| (f.clamp(-1.0, 1.0) * 32767.0) as i16)
            .collect();
        let raw_vad = self
            .vad
            .is_voice_segment(&pcm)
            .map_err(|_| Error::Validation("Format de détection vocale invalide.".into()))?;
        let input_power = power(samples);
        let output_power = power(&out);
        let voice = self.speech_detector.feed(&out)?;
        let (speech, echo_match) = self
            .echo_guard
            .as_mut()
            .map_or((voice, 0.0), |guard| guard.capture(samples, &out, voice));
        Ok(CaptureFrame {
            pcm,
            speech,
            raw_vad,
            echo_match,
            level: output_power.sqrt(),
            attenuation_db: 10.0 * ((input_power + 1e-12) / (output_power + 1e-12)).log10(),
            echo_likelihood: self
                .apm
                .statistics()
                .residual_echo_likelihood
                .unwrap_or(0.0),
        })
    }
}

fn power(samples: &[f32]) -> f32 {
    samples.iter().map(|v| v * v).sum::<f32>() / samples.len().max(1) as f32
}

/// AEC removes the echo; this guard only rejects a *remaining* copy of our
/// output before it can cancel a turn. It never stops or mutes the microphone.
/// Silero is the speech detector. Both the cleaned signal and the
/// microphone mixture are compared: AEC can color a residual strongly, while
/// a separate near-end voice breaks the match with the known loudspeaker audio.
/// The bounded reference includes the acoustic tail and unknown Bluetooth delay.
struct EchoGuard {
    render: VecDeque<f32>,
    capture: VecDeque<f32>,
    raw_capture: VecDeque<f32>,
    render_tail: u16,
    echo_hold: u16,
}

impl Default for EchoGuard {
    fn default() -> Self {
        // Before this output stream starts, its render reference is silence.
        // Include that prefix so the first delayed phoneme can already match.
        Self {
            render: vec![0.0; Self::HISTORY].into(),
            capture: vec![0.0; Self::WINDOW].into(),
            raw_capture: vec![0.0; Self::WINDOW].into(),
            render_tail: 0,
            echo_hold: 0,
        }
    }
}

impl EchoGuard {
    const FRAME: usize = 20; // 2 kHz, used only for echo matching, never STT.
    const WINDOW: usize = 240; // 120 ms; onset confirmation remains independent.
    const HISTORY: usize = 2560; // 1.28 s, bounded and never persisted.

    fn reduce(samples: &[f32]) -> [f32; Self::FRAME] {
        let mut frame = [0.0; Self::FRAME];
        for (i, value) in frame.iter_mut().enumerate() {
            let a = i * samples.len() / Self::FRAME;
            let b = (i + 1) * samples.len() / Self::FRAME;
            *value = samples[a..b].iter().sum::<f32>() / (b - a).max(1) as f32;
        }
        frame
    }

    fn render(&mut self, samples: &[f32]) {
        self.render.extend(Self::reduce(samples));
        while self.render.len() > Self::HISTORY {
            self.render.pop_front();
        }
        if power(samples) > 1e-8 {
            self.render_tail = 128;
        }
    }

    fn capture(&mut self, raw: &[f32], samples: &[f32], vad: bool) -> (bool, f32) {
        self.capture.extend(Self::reduce(samples));
        self.raw_capture.extend(Self::reduce(raw));
        while self.capture.len() > Self::WINDOW {
            self.capture.pop_front();
            self.raw_capture.pop_front();
        }
        let recent_render = self.render_tail > 0;
        self.render_tail = self.render_tail.saturating_sub(1);
        self.echo_hold = self.echo_hold.saturating_sub(1);
        if !vad || !recent_render {
            return (vad, 0.0);
        }
        // VAD has its own hangover: a tiny AEC residual can remain labelled as
        // speech even after suppression. This -70 dBFS floor applies only with
        // an actual render reference; it does not replace VAD or raise the
        // sensitivity threshold when the user talks in silence.
        if power(samples) < 1e-7 {
            return (false, 0.0);
        }
        let captured = self.capture.make_contiguous();
        let raw = self.raw_capture.make_contiguous();
        let rendered = self.render.make_contiguous();
        if captured.len() < Self::WINDOW || rendered.len() < captured.len() {
            return (vad, 0.0);
        }
        let n = captured.len();
        let mean = captured.iter().sum::<f32>() / n as f32;
        let raw_mean = raw.iter().sum::<f32>() / n as f32;
        let mut centered = [0.0; Self::WINDOW];
        let mut centered_raw = [0.0; Self::WINDOW];
        for (out, sample) in centered.iter_mut().zip(captured.iter()) {
            *out = sample - mean;
        }
        for (out, sample) in centered_raw.iter_mut().zip(raw.iter()) {
            *out = sample - raw_mean;
        }
        let energy = centered.iter().map(|v| v * v).sum::<f32>();
        let raw_energy = centered_raw.iter().map(|v| v * v).sum::<f32>();
        let mut sum = rendered[..n].iter().sum::<f32>();
        let mut squares = rendered[..n].iter().map(|v| v * v).sum::<f32>();
        let mut best = 0.0_f32;
        let mut best_raw = 0.0_f32;
        for (offset, reference) in rendered.windows(n).enumerate() {
            if offset > 0 {
                let old = rendered[offset - 1];
                let new = reference[n - 1];
                sum += new - old;
                squares += new * new - old * old;
            }
            let reference_energy = (squares - sum * sum / n as f32).max(0.0);
            if energy * reference_energy > 1e-12 {
                let dot = centered
                    .iter()
                    .zip(reference)
                    .map(|(x, y)| x * y)
                    .sum::<f32>();
                best = best.max((dot * dot / (energy * reference_energy)).min(1.0));
            }
            if raw_energy * reference_energy > 1e-12 {
                let dot = centered_raw
                    .iter()
                    .zip(reference)
                    .map(|(x, y)| x * y)
                    .sum::<f32>();
                best_raw = best_raw.max((dot * dot / (raw_energy * reference_energy)).min(1.0));
            }
        }
        if best >= 0.42 || best_raw >= 0.78 {
            // A few unmatched phonemes between matching echo frames should
            // not create a new turn. Independent speech releases this short
            // hold; capture and the bounded pre-roll continue throughout.
            self.echo_hold = 12;
        }
        (self.echo_hold == 0, best.max(best_raw).sqrt())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signal(at: usize, seed: u32) -> f32 {
        let mut n = (at as u32).wrapping_add(seed).wrapping_mul(747_796_405);
        n = ((n >> ((n >> 28) + 4)) ^ n).wrapping_mul(277_803_737);
        (((n >> 22) ^ n) as f32 / u32::MAX as f32 - 0.5) * 0.4
    }

    #[test]
    fn delayed_echo_cannot_start_a_turn_but_simultaneous_other_voice_can() {
        let mut guard = EchoGuard::default();
        let mut first_near = None;
        for frame in 0..180 {
            let mut render = [0.0; 160];
            let mut raw = [0.0; 160];
            let mut cleaned = [0.0; 160];
            for i in 0..160 {
                let at = frame * 160 + i;
                render[i] = signal(at, 42);
                let echo = if at >= 4000 {
                    (signal(at - 4000, 42) * 3.0).tanh() * 0.15
                } else {
                    0.0
                };
                let near = if frame >= 120 {
                    signal(at, 987_654) * 0.7
                } else {
                    0.0
                };
                raw[i] = echo + near;
                cleaned[i] = echo * 0.1 + near;
            }
            guard.render(&render);
            // Force VAD-positive evidence to exercise the echo veto separately
            // from the speech model. The live simulation also runs actual VAD.
            let (voice, _) = guard.capture(&raw, &cleaned, true);
            if frame < 120 {
                assert!(!voice, "echo incorrectly accepted at {} ms", frame * 10);
            } else if voice {
                first_near.get_or_insert(frame);
            }
        }
        assert!(
            first_near.is_some_and(|f| f <= 155),
            "independent voice blocked: {first_near:?}"
        );
        assert!(guard.render.len() <= EchoGuard::HISTORY);
        assert_eq!(guard.capture.len(), EchoGuard::WINDOW);
    }

    #[test]
    fn echo_guard_does_not_replace_vad_or_block_quiet_voice_without_playback() {
        let mut guard = EchoGuard::default();
        for _ in 0..150 {
            guard.render(&[0.0; 160]);
            assert!(guard.capture(&[1e-5; 160], &[1e-5; 160], true).0);
            assert!(!guard.capture(&[0.2; 160], &[0.2; 160], false).0);
        }
        assert!(guard.render.len() <= EchoGuard::HISTORY);
    }

    #[test]
    fn headphone_noise_cannot_become_a_spurious_interruption() {
        let mut processor = Acoustics::new(48000, 48000, false).unwrap();
        let mut endpoint = crate::voice::Endpoint::new(crate::voice::AudioSettings::default());
        let mut legacy_frames = 0;
        for frame in 0..200 {
            let samples: Vec<_> = (0..480).map(|i| signal(frame * 480 + i, 7)).collect();
            processor.render(&samples).unwrap();
            let output = processor.capture(&samples, 20).unwrap();
            legacy_frames += usize::from(output.raw_vad);
            assert_ne!(
                endpoint.feed(output.speech, true),
                crate::voice::Edge::Start
            );
        }
        assert!(
            legacy_frames >= 10,
            "fixture must expose WebRTC's false positives"
        );
    }
}
