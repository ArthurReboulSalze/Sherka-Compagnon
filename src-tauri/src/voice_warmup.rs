//! Warm VoiceStudio up once it is ready, so the first real turn does not pay
//! for loading its models. A short word goes through the path a turn uses and
//! its audio is discarded: nothing is played, shown, stored or sent anywhere,
//! and no turn waits for it beyond VoiceStudio's own queue.
use crate::{Shared, voice_startup::VoiceStartup};
use sherka_core::{connections::Connections, providers::Providers};
use std::{
    collections::HashSet,
    sync::{
        LazyLock, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager};
use tokio_util::sync::CancellationToken;

/// One model to load, identified by what selects it on the server.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Job {
    Speech {
        url: String,
        engine: String,
        voice: String,
        language: String,
    },
    Transcription {
        url: String,
        model: String,
    },
}

/// What a companion's connections ask to warm up. Nothing without a TTS voice
/// (a Hermes-only setup that never speaks); no transcription warm-up for a
/// Sherpa model, which streams over another route.
pub fn jobs(c: &Connections) -> Vec<Job> {
    let url = c.voice_studio_url.clone();
    let text = |value: &Option<String>| value.clone().filter(|v| !v.trim().is_empty());
    let mut jobs = Vec::new();
    if let (Some(engine), Some(voice)) = (text(&c.tts_engine), text(&c.tts_voice)) {
        jobs.push(Job::Speech {
            url: url.clone(),
            engine,
            voice,
            language: c.language.clone(),
        });
    }
    if let Some(model) = text(&c.stt_model).filter(|m| !m.starts_with("sherpa-")) {
        jobs.push(Job::Transcription { url, model });
    }
    jobs
}

/// Whether a change of the active companion's connections deserves a warm-up.
pub fn speech_changed(before: &Connections, after: &Connections) -> bool {
    jobs(before) != jobs(after)
}

/// A short word in the companion's language.
fn greeting(language: &str) -> &'static str {
    match language
        .split(['-', '_'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "fr" => "Bonjour.",
        "es" => "Hola.",
        "it" => "Ciao.",
        "de" => "Hallo.",
        "zh" => "你好。",
        "ja" => "こんにちは。",
        _ => "Hello.",
    }
}

/// What was warmed since VoiceStudio last started, and what is warming now.
/// A job is tried once per VoiceStudio run, failed or not.
#[derive(Default)]
pub struct Warmed {
    done: HashSet<Job>,
    running: HashSet<Job>,
    generation: u64,
}
impl Warmed {
    /// Claim a job: `None` when it is done or already in progress.
    pub fn begin(&mut self, job: &Job) -> Option<u64> {
        (!self.done.contains(job) && self.running.insert(job.clone())).then_some(self.generation)
    }
    /// The job ended. `done` keeps it from being repeated; a cancelled job is
    /// released instead. An end that belongs to a previous run is ignored.
    pub fn end(&mut self, job: &Job, generation: u64, done: bool) {
        if generation != self.generation {
            return;
        }
        self.running.remove(job);
        if done {
            self.done.insert(job.clone());
        }
    }
    /// VoiceStudio started again: its models are cold, whatever was running
    /// is stale.
    pub fn reset(&mut self) {
        self.done.clear();
        self.running.clear();
        self.generation += 1;
    }
}

static WARMED: LazyLock<Mutex<Warmed>> = LazyLock::new(Default::default);
static CANCEL: LazyLock<Mutex<CancellationToken>> = LazyLock::new(Default::default);
static STOPPED: AtomicBool = AtomicBool::new(false);

fn warmed() -> std::sync::MutexGuard<'static, Warmed> {
    WARMED.lock().unwrap_or_else(|e| e.into_inner())
}

/// Stop what is warming (a pause) without forbidding the next warm-ups.
pub fn cancel() {
    let mut token = CANCEL.lock().unwrap_or_else(|e| e.into_inner());
    token.cancel();
    *token = CancellationToken::new();
}
/// Sherka is quitting: stop for good.
pub fn stop() {
    STOPPED.store(true, Ordering::SeqCst);
    cancel();
}

/// VoiceStudio just became ready. `cold`: it was not ready at the first probe,
/// so it has (re)started and what was warmed before is gone.
pub fn on_ready(app: &AppHandle, cold: bool) {
    if cold {
        warmed().reset();
    }
    schedule(app);
}
/// The active companion or the guest changed.
pub fn companions_changed(app: &AppHandle) {
    schedule(app);
}

fn schedule(app: &AppHandle) {
    if STOPPED.load(Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Some(startup) = app.try_state::<VoiceStartup>() else {
            return;
        };
        let status = startup.status();
        if status.phase != "ready" {
            // Not started, disabled, or still loading: `on_ready` will come.
            return;
        }
        let shared = app.state::<Shared>().inner().clone();
        let connections = crate::with_core(shared, |c| {
            let mut people = vec![c.session.snapshot().profile_id];
            people.extend(c.guest());
            people
                .into_iter()
                .map(|id| c.speaking_profile(id).map(|p| p.details.connections))
                .collect::<sherka_core::Result<Vec<_>>>()
        })
        .await
        .unwrap_or_default();
        for c in connections {
            // Only the server that was just checked is warmed.
            if sherka_core::connections::base_url(&c.voice_studio_url)
                .ok()
                .as_deref()
                == Some(status.url.as_str())
            {
                warm(&app, c).await;
            }
        }
    });
}

async fn warm(app: &AppHandle, connections: Connections) {
    for job in jobs(&connections) {
        let token = CANCEL.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let Some(generation) = warmed().begin(&job) else {
            continue;
        };
        let started = Instant::now();
        let Ok(provider) = Providers::new(connections.clone()) else {
            warmed().end(&job, generation, true);
            continue;
        };
        let result = tokio::select! {
            biased;
            _ = token.cancelled() => None,
            result = tokio::time::timeout(Duration::from_secs(180), run(&provider, &job, &connections)) =>
                Some(result.unwrap_or(false)),
        };
        warmed().end(&job, generation, result.is_some());
        let Some(ok) = result else { continue };
        if STOPPED.load(Ordering::SeqCst) {
            return;
        }
        if let Some(voice) = app.try_state::<crate::conversation::Voice>() {
            voice.diagnostics.record_service(
                "voicestudio_warmup",
                if ok { "ok" } else { "failed" },
                started.elapsed().as_millis() as u64,
            );
        }
    }
}

// The result is dropped: only the loading matters.
async fn run(provider: &Providers, job: &Job, connections: &Connections) -> bool {
    match job {
        Job::Speech { .. } => provider
            .speech(greeting(&connections.language))
            .await
            .is_ok(),
        // Half a second of silence, 16 kHz mono.
        Job::Transcription { .. } => provider.batch_transcribe(&[0i16; 8000]).await.is_ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn voiced() -> Connections {
        Connections {
            tts_engine: Some("kokoro".into()),
            tts_voice: Some("ff_siwis".into()),
            stt_model: Some("small".into()),
            ..Default::default()
        }
    }

    #[test]
    fn speech_and_transcription_are_warmed_when_configured() {
        let jobs = jobs(&voiced());
        assert_eq!(jobs.len(), 2);
        assert!(
            matches!(&jobs[0], Job::Speech { engine, voice, .. } if engine == "kokoro" && voice == "ff_siwis")
        );
        assert!(matches!(&jobs[1], Job::Transcription { model, .. } if model == "small"));
    }

    #[test]
    fn nothing_is_warmed_without_a_voice_and_sherpa_streams_elsewhere() {
        let silent = Connections {
            tts_voice: None,
            stt_model: None,
            ..voiced()
        };
        assert!(jobs(&silent).is_empty());
        let blank = Connections {
            tts_engine: Some(" ".into()),
            stt_model: Some("".into()),
            ..voiced()
        };
        assert!(jobs(&blank).is_empty());
        let sherpa = Connections {
            stt_model: Some("sherpa-onnx".into()),
            ..voiced()
        };
        assert_eq!(jobs(&sherpa).len(), 1);
    }

    #[test]
    fn only_a_different_engine_voice_language_or_model_is_warmed_again() {
        let base = voiced();
        let same = Connections {
            volume: 0.1,
            speed: 1.4,
            ..voiced()
        };
        assert!(!speech_changed(&base, &same));
        for other in [
            Connections {
                tts_voice: Some("other".into()),
                ..voiced()
            },
            Connections {
                tts_engine: Some("other".into()),
                ..voiced()
            },
            Connections {
                language: "en".into(),
                ..voiced()
            },
            Connections {
                stt_model: Some("large".into()),
                ..voiced()
            },
        ] {
            assert!(speech_changed(&base, &other));
        }
    }

    #[test]
    fn a_job_runs_once_at_a_time_and_once_per_voicestudio_run() {
        let job = jobs(&voiced()).remove(0);
        let mut warmed = Warmed::default();
        let first = warmed.begin(&job).unwrap();
        // Already in progress, then done: never repeated.
        assert!(warmed.begin(&job).is_none());
        warmed.end(&job, first, true);
        assert!(warmed.begin(&job).is_none());
        // VoiceStudio restarted: cold again.
        warmed.reset();
        let second = warmed.begin(&job).unwrap();
        assert_ne!(first, second);
        // A cancelled job is released without being counted as done.
        warmed.end(&job, second, false);
        assert!(warmed.begin(&job).is_some());
    }

    #[test]
    fn the_end_of_a_previous_run_does_not_mark_the_new_one() {
        let job = jobs(&voiced()).remove(0);
        let mut warmed = Warmed::default();
        let old = warmed.begin(&job).unwrap();
        warmed.reset();
        let new = warmed.begin(&job).unwrap();
        warmed.end(&job, old, true);
        // Neither released nor done: the new run still owns it.
        assert!(warmed.begin(&job).is_none());
        warmed.end(&job, new, true);
        assert!(warmed.begin(&job).is_none());
    }

    #[test]
    fn the_greeting_follows_the_companion_language() {
        assert_eq!(greeting("fr"), "Bonjour.");
        assert_eq!(greeting("fr-FR"), "Bonjour.");
        assert_eq!(greeting("DE"), "Hallo.");
        assert_eq!(greeting("xx"), "Hello.");
    }
}
