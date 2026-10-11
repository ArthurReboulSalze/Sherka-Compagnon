//! Opt-in acoustic simulation: generated speech, no microphone or playback.
use sherka_core::{
    acoustics::Acoustics,
    connections::Connections,
    providers::{Providers, pcm_bytes},
    voice::{AudioSettings, Edge, Endpoint, PRE_ROLL_MS},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::args().nth(1).unwrap_or_default();
    if mode != "--live" && mode != "--cached" {
        return Err(
            "Usage: echo_check --live (generated speech) or --cached; no microphone".into(),
        );
    }
    let paths = ["artifacts/echo-far.pcm", "artifacts/echo-near.pcm"];
    if mode == "--live" {
        let provider = Providers::new(Connections {
            tts_engine: Some("omnivoice".into()),
            tts_voice: Some(
                std::env::var("SHERKA_TEST_VOICE_ID")
                    .expect("set SHERKA_TEST_VOICE_ID to a VoiceStudio voice ID"),
            ),
            ..Default::default()
        })?;
        for (path, phrase) in paths.iter().zip([
            "La Terre et la Lune voyagent ensemble dans l'espace. Leur distance varie tout au long de leur orbite, et je peux te raconter la suite tranquillement.",
            "Attends Sherka, je voudrais te poser une autre question. Peux-tu m'écouter un instant et arrêter ta réponse ?",
        ]) {
            std::fs::write(path, pcm_bytes(&provider.speech(phrase).await?))?;
        }
    }
    let load = |path: &str| -> Result<Vec<f32>, std::io::Error> {
        Ok(std::fs::read(path)?
            .chunks_exact(2)
            .flat_map(|b| {
                let f = f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0;
                [f, f]
            })
            .collect())
    };
    let far = load(paths[0])?;
    let near = load(paths[1])?;
    let near_onset = near
        .chunks(480)
        .position(|c| c.iter().map(|s| s * s).sum::<f32>() / c.len() as f32 > 1e-6)
        .unwrap()
        * 10;
    // Headphones: background noise/clicks must not interrupt; actual generated
    // speech must still be accepted, including a quieter microphone level.
    for gain in [0.7, 0.12] {
        let mut processor = Acoustics::new(48000, 48000, false)?;
        let mut endpoint = Endpoint::new(AudioSettings::default());
        let mut first = None;
        for frame in 0..1400_usize {
            let capture: Vec<_> = (0..480)
                .map(|i| {
                    let at = frame * 480 + i;
                    let noise =
                        (((at as u32).wrapping_mul(747_796_405) >> 8) as f32 / 16_777_216.0 - 0.5)
                            * 0.004;
                    let click = if at % 24000 < 120 {
                        (at as f32).sin() * 0.08
                    } else {
                        0.0
                    };
                    let speech = at
                        .checked_sub(48000 * 3)
                        .and_then(|a| near.get(a))
                        .copied()
                        .unwrap_or(0.0)
                        * gain;
                    noise + click + speech
                })
                .collect();
            processor.render(&[0.0; 480])?;
            let output = processor.capture(&capture, 20)?;
            if endpoint.feed(output.speech, true) == Edge::Start {
                assert!(frame >= 300, "headphone noise interrupted before voice");
                first.get_or_insert(frame * 10);
            }
        }
        println!("headphones gain={gain}, first voice={first:?} ms (starts at 3000 ms)");
        assert!(first.is_some_and(|at| (3000..=4300).contains(&at)));
        assert!(
            first.unwrap().saturating_sub(PRE_ROLL_MS) <= 3000 + near_onset,
            "pre-roll lost the initial word"
        );
    }
    for delay_ms in [350, 700] {
        for near_gain in [0.0, 0.7, 0.25] {
            let double_talk = near_gain > 0.0;
            let mut processor = Acoustics::new(48000, 48000, true)?;
            let mut endpoint = Endpoint::new(AudioSettings::default());
            let mut starts = Vec::new();
            let mut legacy_endpoint = Endpoint::new(AudioSettings::default());
            let mut legacy_starts = Vec::new();
            let mut raw_run = 0;
            let mut max_raw_run = 0;
            let mut raw_frames = 0;
            let mut guarded_frames = 0;
            let mut echo_stats = Vec::new();
            let far_at = |index: usize| -> f32 {
                if index < 48000 {
                    return 0.0;
                }
                let index = (index - 48000) % (far.len() + 24000);
                far.get(index).copied().unwrap_or(0.0) * 0.8
            };
            for frame in 0..2400_usize {
                let mut render = [0.0; 480];
                let mut capture = [0.0; 480];
                for i in 0..480 {
                    let at = frame * 480 + i;
                    render[i] = far_at(at);
                    let changing_delay = delay_ms + if (frame / 400) % 2 == 1 { 140 } else { 0 };
                    let delayed = at.saturating_sub(changing_delay * 48);
                    let echo = far_at(delayed) * 0.45
                        + far_at(delayed.saturating_sub(1136)) * 0.14
                        + far_at(delayed.saturating_sub(2371)) * 0.07;
                    // Nonlinear speaker/microphone coloration and changing
                    // room gain leave speech-like residuals after linear AEC.
                    capture[i] =
                        (echo * 8.0).tanh() * 0.2 * (1.0 + 0.3 * (at as f32 / 23000.0).sin());
                    if double_talk && at >= 48000 * 10 {
                        capture[i] += near.get(at - 48000 * 10).copied().unwrap_or(0.0) * near_gain;
                    }
                }
                processor.render(&render)?;
                let output = processor.capture(&capture, 30)?;
                raw_run = if output.raw_vad { raw_run + 1 } else { 0 };
                max_raw_run = max_raw_run.max(raw_run);
                raw_frames += usize::from(output.raw_vad);
                guarded_frames += usize::from(output.speech);
                if endpoint.feed(output.speech, true) == Edge::Start {
                    starts.push(frame * 10);
                }
                if legacy_endpoint.feed(output.raw_vad, true) == Edge::Start {
                    legacy_starts.push(frame * 10);
                }
                if output.speech && !double_talk && frame % 10 == 0 {
                    echo_stats.push((
                        frame * 10,
                        (output.attenuation_db * 10.0).round() / 10.0,
                        (output.echo_match * 100.0).round() / 100.0,
                        output.level,
                    ));
                }
            }
            println!(
                "delay={delay_ms}ms near_gain={near_gain} starts_ms={starts:?} previous_starts_ms={legacy_starts:?} raw_frames={raw_frames} max_raw_run_ms={} accepted_frames={guarded_frames} samples={:?}",
                max_raw_run * 10,
                echo_stats.iter().take(12).collect::<Vec<_>>()
            );
            if double_talk {
                assert!(
                    starts.iter().any(|t| (10000..16000).contains(t)),
                    "Near-end interruption was lost"
                );
                assert!(
                    !starts.iter().any(|t| *t < 10000),
                    "Echo interrupted before the user spoke"
                );
            } else {
                assert!(starts.is_empty(), "Echo-only playback must not interrupt");
            }
        }
    }
    Ok(())
}
