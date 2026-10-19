//! One invitation per quiet period, rearmed only by a real user message.
use sherka_core::{
    model::InitiativeConfig,
    session::{ResponseState, TranscriptionState},
};
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{Manager, State};
use uuid::Uuid;
struct Timer {
    config: InitiativeConfig,
    due: Instant,
    waiting: bool,
}
#[derive(Default)]
pub struct Initiative {
    timers: Mutex<HashMap<Uuid, Timer>>,
}
fn delay(config: InitiativeConfig) -> Duration {
    let random = u64::from_le_bytes(Uuid::new_v4().as_bytes()[..8].try_into().unwrap());
    let lo = u64::from(config.min_minutes) * 60;
    let hi = u64::from(config.max_minutes) * 60;
    Duration::from_secs(lo + random % (hi - lo + 1))
}
impl Initiative {
    pub fn activity(&self, id: Uuid, reply: bool) {
        if let Some(timer) = self
            .timers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_mut(&id)
        {
            timer.due = Instant::now() + delay(timer.config);
            if reply {
                timer.waiting = false;
            }
        }
    }
    pub fn invitation_current(&self, id: Uuid) -> bool {
        self.timers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&id)
            .is_some_and(|timer| timer.waiting && Instant::now() >= timer.due)
    }
    fn retry_later(&self, id: Uuid) {
        if let Some(timer) = self
            .timers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_mut(&id)
        {
            timer.waiting = false;
            timer.due = Instant::now() + delay(timer.config);
        }
    }
    fn due(&self, id: Uuid, config: InitiativeConfig, busy: bool, now: Instant) -> bool {
        let mut timers = self.timers.lock().unwrap_or_else(|e| e.into_inner());
        let timer = timers.entry(id).or_insert_with(|| Timer {
            config,
            due: now + delay(config),
            waiting: false,
        });
        if timer.config != config {
            *timer = Timer {
                config,
                due: now + delay(config),
                waiting: false,
            };
        }
        if busy {
            timer.due = now + delay(config);
        }
        if !config.enabled || busy || timer.waiting || now < timer.due {
            return false;
        }
        timer.waiting = true;
        true
    }
}
#[tauri::command]
pub fn initiative_activity(shared: State<'_, crate::Shared>, state: State<'_, Initiative>) {
    if let Ok(core) = shared.0.lock() {
        state.activity(core.session.snapshot().profile_id, false);
    }
}
pub fn user_activity(app: &tauri::AppHandle, id: Uuid) {
    if let Some(state) = app.try_state::<Initiative>() {
        state.activity(id, true);
    }
}
pub fn start(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut previous = None;
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            if crate::windows::ensure_running().is_err() {
                break;
            }
            let shared = app.state::<crate::Shared>().inner().clone();
            let candidate = {
                let core = shared.0.lock().unwrap_or_else(|e| e.into_inner());
                let session = core.session.snapshot();
                core.profile(session.profile_id).ok().map(|profile| {
                    let config = &profile.details.connections;
                    let busy = previous != Some(profile.id)
                        || config.llm_provider.is_agent()
                        || config.llm_model.is_none()
                        || config.tts_engine.is_none()
                        || config.tts_voice.is_none()
                        || core.session.initiative_paused()
                        || session.response != ResponseState::Idle
                        || session.transcription != TranscriptionState::Idle
                        || !crate::windows::widget_visible(&app).unwrap_or(false);
                    (profile.id, profile.details.initiative, busy)
                })
            };
            if let Some((id, config, busy)) = candidate {
                previous = Some(id);
                if app
                    .state::<Initiative>()
                    .due(id, config, busy, Instant::now())
                {
                    let voice = app.state::<crate::conversation::Voice>().inner().clone();
                    if crate::conversation::speak_spontaneously(app.clone(), shared, voice, id)
                        .await
                        .is_err()
                    {
                        app.state::<Initiative>().retry_later(id);
                    }
                }
            }
        }
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_random_delay_and_one_invitation_until_a_real_reply() {
        let config = InitiativeConfig {
            enabled: true,
            min_minutes: 2,
            max_minutes: 4,
        };
        for _ in 0..100 {
            assert!((120..=240).contains(&delay(config).as_secs()));
        }
        let state = Initiative::default();
        let id = Uuid::new_v4();
        let now = Instant::now();
        assert!(!state.due(id, config, false, now));
        assert!(state.due(id, config, false, now + Duration::from_secs(241)));
        state.activity(id, false);
        assert!(!state.due(id, config, false, now + Duration::from_secs(10000)));
        state.activity(id, true);
        assert!(!state.due(id, config, true, now + Duration::from_secs(10000)));
        assert!(state.due(id, config, false, now + Duration::from_secs(10241)));
        assert!(!state.due(Uuid::new_v4(), config, false, now));
    }
}
