//! Opt-in live diagnostic. Synthesizes a fixed test sentence in RAM; never
//! opens a microphone or plays sound. Requires both local servers and models.
use sherka_core::{connections::Connections, conversation::ChatMessage, providers::Providers};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() != Some("--live") {
        return Err(
            "Utilisation explicite : cargo run -p sherka-core --example voice_roundtrip -- --live"
                .into(),
        );
    }
    let provider = Providers::new(Connections {
        llm_model: Some("llama-3-8b-dolphin-uncensored-i1".into()),
        tts_engine: Some("omnivoice".into()),
        tts_voice: Some(
            std::env::var("SHERKA_TEST_VOICE_ID")
                .expect("set SHERKA_TEST_VOICE_ID to a VoiceStudio voice ID"),
        ),
        stt_model: Some("sherpa-parakeet-tdt-v3".into()),
        ..Default::default()
    })?;
    let start = Instant::now();
    let (tx, mut rx) = mpsc::channel(64);
    let generate = provider.chat(
        vec![
            ChatMessage {
                role: "system",
                content: "Réponds en français en une seule phrase courte.".into(),
            },
            ChatMessage {
                role: "user",
                content: "Présente-toi comme Sherka et dis que la connexion fonctionne.".into(),
            },
        ],
        tx,
    );
    let collect = async {
        let mut text = String::new();
        while let Some(s) = rx.recv().await {
            text.push_str(&s);
        }
        text
    };
    let (result, text) = tokio::join!(generate, collect);
    result?;
    println!("LLM {} ms : {}", start.elapsed().as_millis(), text);
    let start = Instant::now();
    let phrase = "Bonjour, je suis Sherka. La connexion vocale fonctionne.";
    let pcm = provider.speech(phrase).await?;
    println!(
        "TTS {} ms ; {} ms audio ; aucune lecture",
        start.elapsed().as_millis(),
        pcm.len() * 1000 / 24000
    );
    let mut acoustics = sherka_core::acoustics::Acoustics::new(24000, 24000, false)?;
    let mut frames = Vec::new();
    let mut speech_frames = 0;
    for chunk in pcm.chunks(240) {
        let mut src = [0.0_f32; 240];
        for (dest, value) in src.iter_mut().zip(chunk) {
            *dest = f32::from(*value) / 32768.0;
        }
        acoustics.render(&[0.0; 240])?;
        let processed = acoustics.capture(&src, 0)?;
        speech_frames += usize::from(processed.speech);
        frames.push(processed.pcm);
    }
    println!(
        "VAD : {speech_frames} trames de parole sur {}",
        frames.len()
    );
    if speech_frames < 10 {
        return Err("La voix générée n'est pas détectée par le VAD.".into());
    }
    let (tx, rx) = mpsc::channel(2048);
    let start = Instant::now();
    let send = async move {
        for frame in frames {
            tx.send(frame).await.unwrap();
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    };
    let transcribe = provider.transcribe(rx, |text, notice| {
        if notice {
            println!("Note STT : {text}");
        }
    });
    let (_, result) = tokio::join!(send, transcribe);
    let result = result?;
    println!(
        "STT {} ms (inclut la durée de capture simulée) : {}",
        start.elapsed().as_millis(),
        result
    );
    if result.trim().is_empty() {
        return Err("Transcription vide.".into());
    }
    // Stricter onset detection must not trade false cuts for lost short words.
    for word in ["Oui.", "Non."] {
        use sherka_core::voice::{AudioSettings, Edge, Endpoint};
        let mut pcm = provider.speech(word).await?;
        let audible_at = pcm
            .chunks(240)
            .position(|c| {
                c.iter()
                    .map(|s| (f32::from(*s) / 32768.0).powi(2))
                    .sum::<f32>()
                    / c.len() as f32
                    > 1e-6
            })
            .unwrap()
            * 10;
        pcm.extend(vec![0; 24000]);
        let mut acoustics = sherka_core::acoustics::Acoustics::new(24000, 24000, false)?;
        let mut endpoint = Endpoint::new(AudioSettings::default());
        let mut first = None;
        let mut last = None;
        for (i, chunk) in pcm.chunks(240).enumerate() {
            let mut samples = [0.0; 240];
            for (sample, value) in samples.iter_mut().zip(chunk) {
                *sample = f32::from(*value) / 32768.0;
            }
            acoustics.render(&[0.0; 240])?;
            let frame = acoustics.capture(&samples, 0)?;
            match endpoint.feed(frame.speech, true) {
                Edge::Start => {
                    first.get_or_insert(i * 10);
                }
                Edge::End => {
                    last = Some(i * 10);
                }
                Edge::None => {}
            }
        }
        println!("Short word {word}: audible={audible_at} ms, start={first:?} ms, end={last:?} ms");
        if first.is_none() || last.is_none() {
            return Err(format!("Short word lost: {word}").into());
        }
        if first
            .unwrap()
            .saturating_sub(sherka_core::voice::PRE_ROLL_MS)
            > audible_at
        {
            return Err(format!("Initial phoneme fell outside pre-roll: {word}").into());
        }
    }
    Ok(())
}
