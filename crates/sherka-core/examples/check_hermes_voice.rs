//! Opt-in real-service check. Uses an existing profile's settings read-only,
//! protected credentials and fresh remote sessions. No microphone or playback.
use sherka_core::{hermes::HermesRun, model::ProfileDraft, providers::Providers};
use std::{path::PathBuf, time::Duration};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 3 {
        return Err("usage: check_hermes_voice <profile.sqlite3> <private-report.json>".into());
    }
    let db = rusqlite::Connection::open_with_flags(
        &args[1],
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let json: String = db.query_row("SELECT details FROM profile WHERE singleton=1", [], |r| {
        r.get(0)
    })?;
    let profile: ProfileDraft = serde_json::from_str(&json)?;
    if !profile.connections.llm_provider.is_agent() {
        return Err("Select a Hermes profile".into());
    }
    // Unauthenticated connectivity check: print error classes, never headers,
    // credentials, URLs or profile contents.
    let reachable = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(8))
        .build()?
        .get(format!(
            "{}/v1/capabilities",
            profile.connections.api_base()?
        ))
        .send()
        .await;
    if let Err(error) = reachable {
        use std::error::Error;
        eprintln!(
            "Connectivity: connect={}, timeout={}",
            error.is_connect(),
            error.is_timeout()
        );
        let mut source = error.source();
        while let Some(cause) = source {
            eprintln!("Cause: {cause}");
            source = cause.source();
        }
        return Err("Hermes connectivity check failed".into());
    }
    let provider = Providers::new(profile.connections)?;
    provider.hermes_probe().await?;
    let mut reports = Vec::new();
    for (name, prompt, keyword) in [
        (
            "web",
            "Cherche sur Internet la date de sortie de Rust un point zéro. Réponds en une phrase avec une source.",
            "rust",
        ),
        (
            "skill",
            "Consulte la liste de tes skills et donne-moi un nom, sans exécuter le skill ni modifier de fichier.",
            "skill",
        ),
    ] {
        println!("{name}: synthesizing input");
        let spoken = provider.speech(prompt).await?;
        // VoiceStudio speech is 24 kHz; its transcription upload is 16 kHz.
        let pcm: Vec<i16> = (0..spoken.len() * 2 / 3)
            .map(|i| {
                let position = i as f64 * 1.5;
                let lo = position as usize;
                let fraction = position.fract();
                (f64::from(spoken[lo]) * (1.0 - fraction)
                    + f64::from(spoken[(lo + 1).min(spoken.len() - 1)]) * fraction)
                    as i16
            })
            .collect();
        let transcript = provider.batch_transcribe(&pcm).await?;
        if !transcript.to_lowercase().contains(keyword) {
            return Err(format!("{name}: unexpected transcription; nothing sent to Hermes").into());
        }
        let session = format!("sherka_check_{}", uuid::Uuid::new_v4().simple());
        let run = HermesRun::new(provider.clone());
        let (tx, mut rx) = mpsc::channel::<String>(64);
        let mut tools = 0;
        let response = async {
            let mut text = String::new();
            while let Some(part) = rx.recv().await {
                text.push_str(&part);
            }
            text
        };
        println!("{name}: transcription received, checking Hermes in a fresh session");
        let (outcome, reply) = tokio::join!(
            run.chat(&session, &session, &transcript, tx, |event| {
                if matches!(
                    event,
                    sherka_core::hermes::AgentEvent::Notice("Hermes utilise ses outils…")
                ) {
                    tools += 1;
                }
                Ok(())
            }),
            response
        );
        let outcome = outcome?;
        if reply.trim().is_empty() {
            return Err("Empty Hermes reply".into());
        }
        let first = reply.chars().take(220).collect::<String>();
        let audio = provider.speech(&first).await?;
        reports.push(serde_json::json!({"test":name,"transcript":transcript,"reply":reply,"tool_events":tools,"reply_audio_samples":audio.len(),"usage":outcome}));
        println!("{name}: completed; {tools} tool events; speech generated");
    }
    let run = HermesRun::new(provider);
    let task_run = run.clone();
    let session = format!("sherka_stop_check_{}", uuid::Uuid::new_v4().simple());
    let (tx, mut rx) = mpsc::channel::<String>(64);
    let mut task = tokio::spawn(async move {
        task_run.chat(&session, &session,
        "Test technique : sans outil ni souvenir durable, compte de un à mille, un nombre par ligne.", tx, |_| Ok(())).await
    });
    let first = tokio::time::timeout(Duration::from_secs(90), rx.recv()).await?;
    if first.is_none() {
        return Err("No streamed reply for stop check".into());
    }
    run.stop().await?;
    println!("/stop: server confirmed terminal run status");
    if tokio::time::timeout(Duration::from_secs(10), &mut task)
        .await
        .is_err()
    {
        task.abort();
    }
    let report = serde_json::json!({"synthetic_voice":true,"physical_microphone":false,"physical_playback":false,"stop_confirmed":true,"checks":reports});
    std::fs::write(
        PathBuf::from(&args[2]),
        serde_json::to_string_pretty(&report)?,
    )?;
    Ok(())
}
