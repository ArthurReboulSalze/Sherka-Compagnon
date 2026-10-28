use crate::{Shared, with_core};
use serde::Serialize;
use sherka_core::{
    Error, Result,
    audio::{AudioController, AudioEvent, Playback},
    conversation::{LiveConversation, SpeechText},
    event::AppEvent,
    providers::Providers,
    session::{MicrophoneState, OperationTag, SessionSnapshot},
    storage::Delivery,
};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{Mutex as AsyncMutex, mpsc};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Clone)]
pub struct Voice {
    pub audio: Arc<Mutex<AudioController>>,
    pub gate: Arc<AsyncMutex<()>>,
    pub playback: Playback,
    pub models: Arc<sherka_core::models::ModelManager>,
    run_id: Arc<Mutex<Option<Uuid>>>,
    pending: Arc<Mutex<CancellationToken>>,
    hermes_gate: Arc<AsyncMutex<()>>,
    hermes_run: Arc<Mutex<Option<(OperationTag, sherka_core::hermes::HermesRun)>>>,
    pub diagnostics: Arc<crate::audio_diagnostics::AudioDiagnostics>,
    speech_input: Arc<Mutex<crate::speech_input::PendingSpeech>>,
    speech_end: Arc<Mutex<Option<(OperationTag, Instant)>>>,
}

#[derive(Clone, Serialize)]
pub struct LiveState {
    pub session: SessionSnapshot,
    pub live: LiveConversation,
}

pub fn live(shared: &Shared) -> std::result::Result<LiveState, String> {
    let core = shared
        .0
        .lock()
        .map_err(|_| "Moteur indisponible.".to_string())?;
    Ok(LiveState {
        session: core.session.snapshot(),
        live: core.live.clone(),
    })
}

pub fn emit(app: &AppHandle, shared: &Shared) {
    if let Ok(value) = live(shared) {
        let on = value.session.microphone == MicrophoneState::Listening;
        crate::windows::update_microphone(app, on);
        let _ = app.emit("sherka:live", value);
    }
}

fn update<F>(app: &AppHandle, shared: &Shared, tag: OperationTag, f: F) -> Result<()>
where
    F: FnOnce(&mut sherka_core::Companion) -> Result<()>,
{
    quiet_update(shared, tag, f)?;
    emit(app, shared);
    Ok(())
}

/// Same checks as `update`, without notifying the webviews.
fn quiet_update<F>(shared: &Shared, tag: OperationTag, f: F) -> Result<()>
where
    F: FnOnce(&mut sherka_core::Companion) -> Result<()>,
{
    let mut core = shared
        .0
        .lock()
        .map_err(|_| Error::Validation("Moteur indisponible.".into()))?;
    if !core.session.accepts(tag) {
        return Err(Error::StaleOperation);
    }
    f(&mut core)
}

/// Streamed text reaches the webviews at most this often. Each event carries
/// the whole live state, so one event per token grew quadratically.
const LIVE_TEXT_INTERVAL: Duration = Duration::from_millis(40);

/// When the webviews last saw the streamed reply, and whether text arrived since.
#[derive(Default)]
struct LivePaint {
    shown: Option<(Instant, sherka_core::avatar::Emotion)>,
    held: bool,
}
impl LivePaint {
    fn show(&mut self, app: &AppHandle, shared: &Shared, emotion: sherka_core::avatar::Emotion) {
        emit(app, shared);
        self.shown = Some((Instant::now(), emotion));
        self.held = false;
    }
    /// Expression changes show at once; text is coalesced.
    fn text(&mut self, app: &AppHandle, shared: &Shared, emotion: sherka_core::avatar::Emotion) {
        if self
            .shown
            .is_none_or(|(at, shown)| shown != emotion || at.elapsed() >= LIVE_TEXT_INTERVAL)
        {
            self.show(app, shared, emotion);
        } else {
            self.held = true;
        }
    }
    /// The last paint, when newer text is waiting to be shown.
    fn held(&self) -> Option<(Instant, sherka_core::avatar::Emotion)> {
        self.shown.filter(|_| self.held)
    }
}

impl Voice {
    pub fn setup(app: AppHandle, shared: Shared, directory: &std::path::Path) -> Self {
        let (tx, rx) = mpsc::channel(256);
        let controller = AudioController::new(tx);
        let voice = Self {
            playback: controller.playback.clone(),
            models: Arc::new(Default::default()),
            audio: Arc::new(Mutex::new(controller)),
            gate: Arc::new(AsyncMutex::new(())),
            run_id: Arc::new(Mutex::new(None)),
            pending: Arc::new(Mutex::new(CancellationToken::new())),
            hermes_gate: Arc::new(AsyncMutex::new(())),
            hermes_run: Arc::new(Mutex::new(None)),
            diagnostics: Arc::new(crate::audio_diagnostics::AudioDiagnostics::new(directory)),
            speech_input: Arc::new(Mutex::new(Default::default())),
            speech_end: Arc::new(Mutex::new(None)),
        };
        tauri::async_runtime::spawn(audio_events(app, shared, voice.clone(), rx));
        voice
    }
    fn diagnose(&self, shared: &Shared, event: &'static str, audio_error: Option<&str>) {
        let (reply, heard) = shared
            .0
            .lock()
            .map(|c| (c.live.reply.chars().count(), c.live.heard.chars().count()))
            .unwrap_or_default();
        self.diagnostics.record(event, reply, heard, audio_error);
    }
    /// When the utterance that produced this turn ended, if it did.
    fn take_speech_end(&self, tag: OperationTag) -> Option<Instant> {
        let mut end = self.speech_end.lock().unwrap_or_else(|e| e.into_inner());
        end.take_if(|(at, _)| *at == tag)
            .map(|(_, instant)| instant)
    }
    fn begin_pending(&self) -> CancellationToken {
        let mut token = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        token.cancel();
        *token = CancellationToken::new();
        token.clone()
    }
    pub fn cancel_pending(&self) {
        self.begin_pending();
        self.speech_input
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }
    pub async fn stop_audio(&self) {
        self.speech_input
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        if let Ok(mut id) = self.run_id.lock() {
            *id = None;
        }
        self.playback.interrupt();
        let audio = self.audio.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || {
            if let Ok(mut a) = audio.lock() {
                a.stop();
            }
        })
        .await;
    }
    async fn ensure_audio(
        &self,
        shared: &Shared,
        capture: bool,
    ) -> std::result::Result<(), String> {
        let settings = with_core(shared.clone(), |c| Ok(c.audio_settings())).await?;
        let audio = self.audio.clone();
        let run_id = tauri::async_runtime::spawn_blocking(move || {
            audio
                .lock()
                .map_err(|_| "Moteur audio indisponible.".to_string())?
                .ensure(settings, capture)
                .map_err(|e| e.to_string())
        })
        .await
        .map_err(|_| "Le moteur audio n'a pas démarré.".to_string())??;
        *self
            .run_id
            .lock()
            .map_err(|_| "Moteur audio indisponible.".to_string())? = Some(run_id);
        Ok(())
    }
}

/// `/stop` controls only the active companion's remote run, never a service.
pub async fn stop_hermes(
    app: &AppHandle,
    shared: Shared,
    voice: &Voice,
) -> std::result::Result<(), String> {
    let profile_id = with_core(shared.clone(), |c| {
        let id = c.session.snapshot().profile_id;
        if !c
            .store
            .profile(id)?
            .details
            .connections
            .llm_provider
            .is_agent()
        {
            return Err(Error::StaleOperation);
        }
        Ok(id)
    })
    .await?;
    pause(app, shared.clone(), voice, false).await?;
    with_core(shared.clone(), |c| {
        c.live.notice = Some("Arrêt Hermes demandé…".into());
        Ok(())
    })
    .await?;
    emit(app, &shared);
    let _gate = voice.hermes_gate.lock().await;
    let pending = voice
        .hermes_run
        .lock()
        .map_err(|_| "Moteur indisponible.")?
        .clone();
    if let Some((tag, run)) = pending
        && tag.profile_id == profile_id
    {
        if let Err(e) = run.stop().await {
            let message = format!(
                "{} L’arrêt Hermes n’est pas confirmé. Réessaie /stop avant de poursuivre.",
                e
            );
            set_error(app, &shared, &message);
            return Err(message);
        }
        *voice
            .hermes_run
            .lock()
            .map_err(|_| "Moteur indisponible.")? = None;
    }
    with_core(shared.clone(), move |c| {
        if c.session.snapshot().profile_id == profile_id {
            c.live.error = None;
            c.live.notice = Some("Arrêt Hermes confirmé.".into());
        }
        Ok(())
    })
    .await?;
    emit(app, &shared);
    Ok(())
}

pub async fn pause(
    app: &AppHandle,
    shared: Shared,
    voice: &Voice,
    microphone: bool,
) -> std::result::Result<(), String> {
    voice.diagnose(
        &shared,
        if microphone {
            "microphone_paused"
        } else {
            "response_stopped"
        },
        None,
    );
    voice.cancel_pending();
    voice.playback.interrupt();
    let result = with_core(shared.clone(), move |c| {
        if microphone {
            c.pause_voice()
        } else {
            c.stop_response()
        }
    })
    .await;
    if microphone {
        voice.stop_audio().await;
    }
    emit(app, &shared);
    let _ = app.emit("sherka:changed", ());
    result
}

/// Microphone button: stop listening but let the companion finish its reply.
/// The audio device closes once nothing remains to play. Global pause and
/// Stop still interrupt the reply.
async fn mute(app: &AppHandle, shared: Shared, voice: &Voice) -> std::result::Result<(), String> {
    voice.diagnose(&shared, "microphone_muted", None);
    voice.cancel_pending();
    voice
        .speech_input
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clear();
    let audio = voice.audio.clone();
    let _ = tauri::async_runtime::spawn_blocking(move || {
        if let Ok(audio) = audio.lock() {
            audio.set_listening(false);
        }
    })
    .await;
    with_core(shared.clone(), |c| {
        c.mute_microphone();
        Ok(())
    })
    .await?;
    emit(app, &shared);
    let _ = app.emit("sherka:changed", ());
    let voice = voice.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            if live(&shared).map_or(true, |s| s.session.microphone == MicrophoneState::Listening) {
                return;
            }
            if !voice.playback.responding() && !voice.playback.speaking() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
        let _gate = voice.gate.lock().await;
        if live(&shared).is_ok_and(|s| s.session.microphone != MicrophoneState::Listening)
            && !voice.playback.responding()
            && !voice.playback.speaking()
        {
            voice.stop_audio().await;
        }
    });
    Ok(())
}

pub async fn toggle(
    app: AppHandle,
    shared: Shared,
    voice: Voice,
) -> std::result::Result<(), String> {
    crate::windows::ensure_running()?;
    let gate = voice.gate.lock().await;
    crate::windows::ensure_running()?;
    if live(&shared)?.session.microphone == MicrophoneState::Listening {
        return mute(&app, shared, &voice).await;
    }
    let connections=with_core(shared.clone(),|c| {
        let p=c.profile(c.session.snapshot().profile_id)?;
        if (!p.details.connections.llm_provider.is_agent() && p.details.connections.llm_model.is_none()) || p.details.connections.stt_model.is_none() || p.details.connections.tts_engine.is_none() || p.details.connections.tts_voice.is_none() {
            return Err(Error::Validation("Choisis le modèle de conversation, le moteur de transcription et la voix dans les réglages avant d'activer le micro.".into()));
        }
        Ok(p.details.connections)
    }).await?;
    let pending = voice.begin_pending();
    // Keep settings, profile switches and Pause available during a cold start.
    // A cancelled request must never open the microphone when readiness arrives.
    drop(gate);
    let startup = app.state::<crate::voice_startup::VoiceStartup>();
    tokio::select! {
        biased;
        _ = pending.cancelled() => return Err("Demande vocale annulée.".into()),
        result = startup.ensure(&connections.voice_studio_url) => result?,
    }
    let _gate = voice.gate.lock().await;
    crate::windows::ensure_running()?;
    if pending.is_cancelled() {
        return Err("Demande vocale annulée.".into());
    }
    if live(&shared)?.session.microphone == MicrophoneState::Listening {
        return mute(&app, shared, &voice).await;
    }
    // Read-only preflight before opening the microphone. No model downloads.
    // VoiceStudio may still be finishing its start-up: transient failures wait.
    let catalog =
        crate::voice_startup::voice_catalog(&connections.voice_studio_url, &pending).await?;
    let http_available = catalog
        .stt_models
        .iter()
        .any(|m| Some(m) == connections.stt_model.as_ref());
    let direct_available = catalog.dictation_engine_available
        && catalog
            .dictation_models
            .iter()
            .any(|m| m.installed && Some(&m.id) == connections.stt_model.as_ref());
    if !http_available && !direct_available {
        return Err(
            "Le moteur STT choisi n'est plus actif dans VoiceStudio. Actualise les réglages."
                .into(),
        );
    }
    voice.ensure_audio(&shared, true).await?;
    voice.diagnose(&shared, "microphone_started", None);
    with_core(shared.clone(), |c| {
        c.live.error = None;
        c.live.notice = None;
        c.session.capture_started();
        Ok(())
    })
    .await?;
    emit(&app, &shared);
    Ok(())
}

pub async fn send(
    app: AppHandle,
    shared: Shared,
    voice: Voice,
    profile_id: Uuid,
    text: String,
    speak: bool,
    attachments: Vec<sherka_core::attachments::Input>,
) -> std::result::Result<(), String> {
    crate::initiative::user_activity(&app, profile_id);
    send_inner(
        app,
        shared,
        voice,
        profile_id,
        text,
        speak,
        attachments,
        None,
    )
    .await
}
pub async fn speak_spontaneously(
    app: AppHandle,
    shared: Shared,
    voice: Voice,
    profile_id: Uuid,
) -> std::result::Result<(), String> {
    send_inner(
        app,
        shared,
        voice,
        profile_id,
        AppEvent::Idle.label().into(),
        true,
        vec![],
        Some(AppEvent::Idle),
    )
    .await
}
/// The master reacts to an application event with one spoken remark. Silent
/// when the conversation is busy, the event is stale or the master cannot
/// answer or speak.
pub async fn speak_event(
    app: AppHandle,
    shared: Shared,
    voice: Voice,
    profile_id: Uuid,
    event: AppEvent,
) -> std::result::Result<(), String> {
    send_inner(
        app,
        shared,
        voice,
        profile_id,
        event.label().into(),
        true,
        vec![],
        Some(event),
    )
    .await
}
/// The master greets the guest that was just invited, unless it is no longer
/// the guest.
pub async fn greet_guest(
    app: AppHandle,
    shared: Shared,
    voice: Voice,
    guest: Uuid,
) -> std::result::Result<(), String> {
    let found = with_core(shared.clone(), move |c| {
        if c.guest() != Some(guest) {
            return Ok(None);
        }
        let master = c.session.snapshot().profile_id;
        Ok(sherka_core::event::guest_joined(c)?.map(|event| (master, event)))
    })
    .await?;
    let Some((master, event)) = found else {
        return Ok(());
    };
    speak_event(app, shared, voice, master, event).await
}
#[allow(clippy::too_many_arguments)]
async fn send_inner(
    app: AppHandle,
    shared: Shared,
    voice: Voice,
    profile_id: Uuid,
    text: String,
    speak: bool,
    attachments: Vec<sherka_core::attachments::Input>,
    event: Option<AppEvent>,
) -> std::result::Result<(), String> {
    let spontaneous = event.is_some();
    crate::windows::ensure_running()?;
    let gate = voice.gate.lock().await;
    crate::windows::ensure_running()?;
    if text.trim().is_empty() || text.len() > 128_000 {
        return Err("Écris un message de 1 à 128 000 octets.".into());
    }
    if text.trim().eq_ignore_ascii_case("/stop") {
        let agent = with_core(shared.clone(), move |c| {
            Ok(c.session.snapshot().profile_id == profile_id
                && c.profile(profile_id)?
                    .details
                    .connections
                    .llm_provider
                    .is_agent())
        })
        .await?;
        if agent {
            return stop_hermes(&app, shared, &voice).await;
        }
    }
    let command = text.clone();
    let gated = event.clone();
    let (connections, maintenance) = with_core(shared.clone(), move |core| {
        if core.session.snapshot().profile_id != profile_id {
            return Err(Error::StaleOperation);
        }
        let profile = core.profile(profile_id)?;
        if let Some(event) = &gated {
            check_event(core, &profile, event)?;
        }
        let maintenance = (!spontaneous && !profile.details.connections.llm_provider.is_agent())
            .then(|| {
                sherka_core::privacy::contextual_command(
                    &command,
                    &profile.details.name,
                    core.pending_clear
                        .valid(profile_id, core.session.snapshot().session_id),
                )
            })
            .flatten();
        if maintenance.is_none() {
            core.pending_clear = Default::default();
        }
        if maintenance.is_none()
            && !profile.details.connections.llm_provider.is_agent()
            && profile.details.connections.llm_model.is_none()
        {
            return Err(Error::Validation(
                "Choisis un modèle de conversation dans Compagnons.".into(),
            ));
        }
        if maintenance.is_none()
            && speak
            && (profile.details.connections.tts_engine.is_none()
                || profile.details.connections.tts_voice.is_none()
                || core
                    .guest()
                    .and_then(|guest| core.profile(guest).ok())
                    .is_some_and(|guest| {
                        guest.details.connections.tts_engine.is_none()
                            || guest.details.connections.tts_voice.is_none()
                    }))
        {
            return Err(Error::Validation(
                "Choisis un moteur vocal et une voix, ou désactive la lecture de la réponse."
                    .into(),
            ));
        }
        Ok((profile.details.connections, maintenance))
    })
    .await?;
    if let Some(command) = maintenance {
        // A local privacy command also works with an offline or unconfigured
        // provider. Speech is only an optional, non-persisted acknowledgement.
        drop(gate);
        return maintenance_command(&app, shared, &voice, profile_id, command, speak, None).await;
    }
    let mut provider = Providers::new(connections).map_err(|e| e.to_string())?;
    provider.images = tauri::async_runtime::spawn_blocking(move || {
        sherka_core::attachments::normalize(attachments)
    })
    .await
    .map_err(|_| "Impossible de lire ce fichier.".to_string())?
    .map_err(|e| e.to_string())?;
    if !provider.images.is_empty()
        && !provider
            .image_support()
            .await
            .map_err(|e| e.to_string())?
            .images
    {
        return Err(
            "Ce modèle n’accepte pas les images ou sa capacité visuelle n’est pas confirmée."
                .into(),
        );
    }
    let pending = voice.begin_pending();
    let _gate = if speak {
        drop(gate);
        let startup = app.state::<crate::voice_startup::VoiceStartup>();
        tokio::select! {
            biased;
            _ = pending.cancelled() => return Err("Demande vocale annulée.".into()),
            result = startup.ensure(&provider.connections.voice_studio_url) => result?,
        }
        voice.gate.lock().await
    } else {
        gate
    };
    crate::windows::ensure_running()?;
    if pending.is_cancelled() {
        return Err("Demande vocale annulée.".into());
    }
    if let Some(event) = event.clone() {
        // Only the idle timer is tied to its own invitation window.
        if event == AppEvent::Idle
            && !app
                .state::<crate::initiative::Initiative>()
                .invitation_current(profile_id)
        {
            return Err(Error::StaleOperation.to_string());
        }
        with_core(shared.clone(), move |c| {
            check_event(c, &c.profile(profile_id)?, &event)
        })
        .await?;
    }
    voice.playback.interrupt();
    if !spontaneous {
        with_core(shared.clone(), |c| c.stop_response()).await?;
    }
    if speak {
        voice.ensure_audio(&shared, false).await?;
    }
    let user = text.trim().to_owned();
    let recorded = user.clone();
    let checked = event.clone();
    let (tag, token, guest) = with_core(shared.clone(), move |c| {
        if c.session.snapshot().profile_id != profile_id {
            return Err(Error::StaleOperation);
        }
        if let Some(event) = &checked {
            check_event(c, &c.profile(profile_id)?, event)?;
        }
        let tag = c.session.begin_text_turn();
        if spontaneous {
            // Not the user: a pending memory extraction stays alive.
            c.reset_live_for_event(tag);
        } else {
            c.reset_live(tag);
        }
        if !spontaneous {
            // Also written to the guest's history during a three-way conversation.
            c.record_user_message(tag, &recorded, Delivery::TextOnly)?;
        }
        Ok((tag, c.session.cancellation(), c.guest()))
    })
    .await?;
    app.state::<crate::illustrations::Illustrations>()
        .cancel(&app, false);
    emit(&app, &shared);
    let _ = app.emit("sherka:history", profile_id);
    if let Some(guest) = guest {
        let _ = app.emit("sherka:history", guest);
    }
    let priority = app
        .state::<crate::illustrations::Illustrations>()
        .priority(app.clone(), shared.clone());
    if guest.is_some() {
        tauri::async_runtime::spawn(run_trio_turn(
            app,
            shared,
            voice.clone(),
            tag,
            token,
            provider,
            (!spontaneous).then_some(user),
            event,
            speak,
            priority,
        ));
    } else {
        tauri::async_runtime::spawn(run_turn(
            app,
            shared,
            voice.clone(),
            tag,
            token,
            provider,
            user,
            speak,
            priority,
            event,
            None,
        ));
    }
    Ok(())
}

async fn audio_events(
    app: AppHandle,
    shared: Shared,
    voice: Voice,
    mut events: mpsc::Receiver<AudioEvent>,
) {
    let mut capture: Option<(Uuid, OperationTag, mpsc::Sender<Vec<i16>>)> = None;
    while let Some(event) = events.recv().await {
        let id = match &event {
            AudioEvent::Start(id, _)
            | AudioEvent::Frame(id, _)
            | AudioEvent::End(id)
            | AudioEvent::Meter(id, _, _)
            | AudioEvent::InputGap(id)
            | AudioEvent::Failed(id, _) => *id,
        };
        // A stopped/replaced device session can never feed the new profile.
        let relevant = voice
            .run_id
            .lock()
            .is_ok_and(|current| *current == Some(id));
        if !relevant {
            continue;
        }
        match event {
            AudioEvent::Start(id, pcm) => {
                app.state::<crate::illustrations::Illustrations>()
                    .cancel(&app, false);
                voice.diagnose(&shared, "speech_detected", None);
                capture = None;
                let current_run = voice.run_id.clone();
                let prepared = with_core(shared.clone(), move |c| {
                    if !current_run.lock().is_ok_and(|run| *run == Some(id)) {
                        return Err(Error::StaleOperation);
                    }
                    let previous = if c.session.snapshot().transcription
                        == sherka_core::session::TranscriptionState::Transcribing
                    {
                        c.live.tag
                    } else {
                        None
                    };
                    c.stop_response()?;
                    let tag = c.session.begin_utterance()?;
                    c.reset_live(tag);
                    let provider = Providers::new(c.profile(tag.profile_id)?.details.connections)?;
                    Ok((tag, c.session.cancellation(), provider, previous))
                })
                .await;
                if let Ok((tag, token, provider, previous)) = prepared {
                    crate::initiative::user_activity(&app, tag.profile_id);
                    let pcm = voice
                        .speech_input
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .start(id, previous, tag, pcm);
                    let Some(pcm) = pcm else {
                        fail_turn(&app,&shared,tag,"Cette prise de parole est trop longue pour le contexte choisi. Fais une pause et reprends avec une phrase plus courte.").await;
                        continue;
                    };
                    let (tx, rx) = mpsc::channel(2048);
                    let _ = tx.try_send(pcm);
                    capture = Some((id, tag, tx));
                    emit(&app, &shared);
                    let _ = app.emit("sherka:history", tag.profile_id);
                    tauri::async_runtime::spawn(recognize(
                        app.clone(),
                        shared.clone(),
                        voice.clone(),
                        tag,
                        token,
                        provider,
                        rx,
                    ));
                }
            }
            AudioEvent::Frame(id, pcm) => {
                if let Some((run, tag, tx)) = capture.as_ref()
                    && *run == id
                {
                    let current = shared.0.lock().is_ok_and(|c| c.session.accepts(*tag));
                    if !current {
                        capture = None;
                        continue;
                    }
                    if !voice
                        .speech_input
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .append(id, *tag, &pcm)
                    {
                        let tag = *tag;
                        capture = None;
                        fail_turn(&app,&shared,tag,"Cette prise de parole est trop longue pour le contexte choisi. Fais une pause et reprends avec une phrase plus courte.").await;
                        continue;
                    }
                    if let Err(error) = tx.try_send(pcm) {
                        let tag = *tag;
                        let abandoned = capture.take();
                        let full = matches!(error, mpsc::error::TrySendError::Full(_));
                        voice.diagnose(
                            &shared,
                            if full {
                                "transcription_backpressure"
                            } else {
                                "transcription_closed"
                            },
                            None,
                        );
                        // A failed/slow transcription is local to this utterance.
                        // Never revoke microphone consent or close WASAPI.
                        fail_turn(&app, &shared, tag,
                            "La transcription a été interrompue. Le micro reste actif ; fais une pause puis répète ta phrase.").await;
                        // Cancel before EOF: otherwise dropping the sender
                        // could finalize and send a damaged partial command.
                        drop(abandoned);
                    }
                }
            }
            AudioEvent::End(id) => {
                if let Some((run, tag, tx)) = capture.take()
                    && run == id
                {
                    *voice.speech_end.lock().unwrap_or_else(|e| e.into_inner()) =
                        Some((tag, Instant::now()));
                    let _ = update(&app, &shared, tag, |c| c.session.finish_capture(tag));
                    drop(tx);
                }
            }
            AudioEvent::Meter(_, level, speech) => {
                if live(&shared).is_ok_and(|s| s.session.microphone == MicrophoneState::Listening) {
                    let _ = app.emit(
                        "sherka:meter",
                        serde_json::json!({"level":level,"speech":speech}),
                    );
                }
            }
            AudioEvent::InputGap(_) => {
                voice
                    .speech_input
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clear();
                voice.diagnose(&shared, "capture_gap_recovered", None);
                if let Some((_, tag, input)) = capture.take() {
                    fail_turn(&app, &shared, tag,
                        "La transcription a été interrompue. Le micro reste actif ; fais une pause puis répète ta phrase.").await;
                    drop(input);
                }
            }
            AudioEvent::Failed(_, message) => {
                voice
                    .speech_input
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clear();
                voice.diagnose(&shared, "audio_failed", Some(&message));
                capture = None;
                voice.playback.interrupt();
                let _ = with_core(shared.clone(), move |c| {
                    c.stop_response()?;
                    c.session.capture_failed();
                    c.live.error = Some(message);
                    Ok(())
                })
                .await;
                emit(&app, &shared);
            }
        }
    }
}

fn set_error(app: &AppHandle, shared: &Shared, message: &str) {
    if let Ok(mut core) = shared.0.lock() {
        core.live.error = Some(message.into());
    }
    emit(app, shared);
}

async fn recognize(
    app: AppHandle,
    shared: Shared,
    voice: Voice,
    tag: OperationTag,
    token: CancellationToken,
    provider: Providers,
    input: mpsc::Receiver<Vec<i16>>,
) {
    let priority = app
        .state::<crate::illustrations::Illustrations>()
        .priority(app.clone(), shared.clone());
    tokio::select! {biased;_=token.cancelled()=>return,_=priority.ready()=>{}}
    let progress = |text: &str, notice: bool| {
        let _ = update(&app, &shared, tag, |c| {
            if notice {
                c.live.notice = Some(text.into());
            } else {
                c.live.transcript = text.into();
            }
            Ok(())
        });
    };
    let result = tokio::select! { biased; _=token.cancelled()=>return, r=provider.transcribe(input,progress)=>r };
    match result {
        Ok(text) => {
            let text = sherka_core::transcript::clean(&text);
            if text.is_empty() {
                voice.diagnose(&shared, "stt_noise_discarded", None);
            }
            let result = with_core(shared.clone(), move |c| {
                let Some(text) = c.session.accept_stt_final(tag, &text)? else {
                    c.live.tag = None;
                    c.live.transcript.clear();
                    return Ok(None);
                };
                c.record_user_message(tag, &text, Delivery::TextOnly)?;
                c.live.transcript = text.clone();
                Ok(Some((text, c.guest())))
            })
            .await;
            voice
                .speech_input
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .finish(tag);
            match result {
                Ok(Some((messages, guest))) => {
                    emit(&app, &shared);
                    let _ = app.emit("sherka:history", tag.profile_id);
                    if let Some(guest) = guest {
                        let _ = app.emit("sherka:history", guest);
                        run_trio_turn(
                            app,
                            shared,
                            voice,
                            tag,
                            token,
                            provider,
                            Some(messages),
                            None,
                            true,
                            priority,
                        )
                        .await;
                    } else {
                        run_turn(
                            app, shared, voice, tag, token, provider, messages, true, priority,
                            None, None,
                        )
                        .await;
                    }
                }
                Ok(None) => emit(&app, &shared),
                Err(e) => fail_turn(&app, &shared, tag, &e).await,
            }
        }
        Err(error) => {
            voice
                .speech_input
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .finish(tag);
            voice.diagnose(&shared, "transcription_failed", None);
            fail_turn(&app, &shared, tag, &error.to_string()).await;
        }
    }
}

async fn fail_turn(app: &AppHandle, shared: &Shared, tag: OperationTag, message: &str) {
    let text = message.to_owned();
    let _ = with_core(shared.clone(), move |c| record_turn_failure(c, tag, text)).await;
    emit(app, shared);
    let _ = app.emit("sherka:history", tag.profile_id);
}

fn record_turn_failure(
    c: &mut sherka_core::Companion,
    tag: OperationTag,
    message: String,
) -> Result<()> {
    if c.live.tag != Some(tag) {
        return Err(Error::StaleOperation);
    }
    c.stop_response()?;
    c.live.error = Some(message);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn maintenance_command(
    app: &AppHandle,
    shared: Shared,
    voice: &Voice,
    profile_id: Uuid,
    command: sherka_core::privacy::Command,
    speak: bool,
    expected: Option<OperationTag>,
) -> std::result::Result<(), String> {
    use sherka_core::privacy::Command;
    with_core(shared.clone(), move |c| {
        if c.session.snapshot().profile_id != profile_id
            || expected.is_some_and(|tag| !c.session.accepts(tag))
        {
            return Err(Error::StaleOperation);
        }
        Ok(())
    })
    .await?;
    match command {
        Command::Optimize => {
            crate::memory_ops::prepare(app, shared.clone(), voice, profile_id).await?;
        }
        Command::ApplyOptimization => {
            crate::memory_ops::finish(app, shared.clone(), profile_id, true).await?
        }
        Command::ConfirmClear => {
            let confirmed = with_core(shared.clone(), move |c| {
                let session = c.session.snapshot();
                if session.profile_id != profile_id
                    || expected.is_some_and(|tag| !c.session.accepts(tag))
                {
                    return Err(Error::StaleOperation);
                }
                let confirmed = c.pending_clear.take(profile_id, session.session_id);
                if !confirmed {
                    c.stop_response()?;
                    c.live.notice = Some(
                        "Aucune suppression à confirmer. Demande d’abord d’effacer la discussion."
                            .into(),
                    );
                }
                Ok(confirmed)
            })
            .await?;
            if confirmed {
                clear(
                    app,
                    shared.clone(),
                    voice,
                    profile_id,
                    false,
                    false,
                    expected,
                )
                .await?;
            }
        }
        Command::RequestClear | Command::Cancel => {
            let _gate = voice.gate.lock().await;
            voice.playback.interrupt();
            with_core(shared.clone(),move|c|{
                if c.session.snapshot().profile_id!=profile_id || expected.is_some_and(|tag|!c.session.accepts(tag)) {return Err(Error::StaleOperation);}
                c.stop_response()?;c.optimization=None;
                if command==Command::RequestClear {
                    c.pending_clear.request(profile_id,c.session.snapshot().session_id);
                    c.live.notice=Some("Effacer toute la discussion en conservant les faits mémorisés ? Dis « confirme la suppression » ou écris /confirm-clear. Pour annuler : /cancel.".into());
                } else {c.pending_clear=Default::default();c.live.notice=Some("Opération annulée. La discussion et les souvenirs sont conservés.".into());}
                Ok(())
            }).await?;
        }
    }
    emit(app, &shared);
    if speak {
        let (connections, session, message) = with_core(shared.clone(), move |c| {
            if c.session.snapshot().profile_id != profile_id {
                return Err(Error::StaleOperation);
            }
            Ok((
                c.profile(profile_id)?.details.connections,
                c.session.snapshot().session_id,
                c.live.notice.clone().unwrap_or_default(),
            ))
        })
        .await?;
        if connections.tts_engine.is_some()
            && connections.tts_voice.is_some()
            && let Ok(provider) = Providers::new(connections)
        {
            let (app, voice) = (app.clone(), voice.clone());
            tauri::async_runtime::spawn(async move {
                speak_receipt(
                    &app, shared, &voice, &provider, profile_id, session, message,
                )
                .await;
            });
        }
    }
    Ok(())
}

pub async fn clear(
    app: &AppHandle,
    shared: Shared,
    voice: &Voice,
    profile_id: Uuid,
    erase_saved_facts: bool,
    erase_mood: bool,
    expected: Option<OperationTag>,
) -> std::result::Result<Uuid, String> {
    crate::windows::ensure_running()?;
    let _gate = voice.gate.lock().await;
    crate::windows::ensure_running()?;
    let (agent, listening) = with_core(shared.clone(), move |c| {
        if c.session.snapshot().profile_id != profile_id
            || expected.is_some_and(|tag| !c.session.accepts(tag))
        {
            return Err(Error::StaleOperation);
        }
        Ok((
            c.profile(profile_id)?
                .details
                .connections
                .llm_provider
                .is_agent(),
            c.session.snapshot().microphone == MicrophoneState::Listening,
        ))
    })
    .await?;
    voice.cancel_pending();
    if agent {
        stop_hermes(app, shared.clone(), voice).await?;
    }
    // Retire input too: queued pre-deletion audio must not recreate messages.
    with_core(shared.clone(), |c| c.pause_voice()).await?;
    voice.stop_audio().await;
    let session = with_core(shared.clone(), move |c| {
        if c.session.snapshot().profile_id != profile_id {
            return Err(Error::StaleOperation);
        }
        c.clear_conversation_with_mood(profile_id, erase_saved_facts, erase_mood)?;
        c.live.notice = Some(if agent {
            "Historique local effacé. Les conversations et souvenirs du serveur Hermes restent gérés par Hermes."
        } else if erase_saved_facts {
            "Conversation et faits sauvegardés effacés."
        } else {
            "Conversation effacée. Les faits sauvegardés sont conservés."
        }.into());
        Ok(c.session.snapshot().session_id)
    }).await?;
    app.state::<crate::illustrations::Illustrations>()
        .cancel(app, true);
    crate::illustrations::synchronize(app, &shared);
    if listening && crate::windows::ensure_running().is_ok() {
        match voice.ensure_audio(&shared, true).await {
            Ok(()) => {
                with_core(shared.clone(), |c| {
                    c.session.capture_started();
                    Ok(())
                })
                .await?;
            }
            Err(error) => {
                with_core(shared.clone(), move |c| {
                    c.session.capture_failed();
                    c.live.error = Some(error);
                    Ok(())
                })
                .await?;
            }
        }
    }
    emit(app, &shared);
    let _ = app.emit("sherka:history", profile_id);
    let _ = app.emit("sherka:memory", profile_id);
    let _ = app.emit("sherka:cleared", profile_id);
    Ok(session)
}

/// A local receipt can be spoken without creating a new history entry or
/// sending the deletion request back to a language model.
async fn speak_receipt(
    app: &AppHandle,
    shared: Shared,
    voice: &Voice,
    provider: &Providers,
    profile_id: Uuid,
    session_id: Uuid,
    message: String,
) {
    let gate = voice.gate.lock().await;
    if crate::windows::ensure_running().is_err() {
        return;
    }
    if voice.ensure_audio(&shared, false).await.is_err() {
        return;
    }
    let language =
        sherka_core::i18n::Language::from_code(&provider.connections.language).unwrap_or_default();
    let text = sherka_core::i18n::text_in(language, &message);
    let Ok((tag, token)) = with_core(shared.clone(), move |c| {
        let current = c.session.snapshot();
        if current.profile_id != profile_id
            || current.session_id != session_id
            || current.transcription != sherka_core::session::TranscriptionState::Idle
            || current.response != sherka_core::session::ResponseState::Idle
        {
            return Err(Error::StaleOperation);
        }
        // Keep live.tag/reply empty: interruptions must never archive this
        // acknowledgement as another conversation after the deletion.
        let tag = c.session.begin_text_turn();
        Ok((tag, c.session.cancellation()))
    })
    .await
    else {
        return;
    };
    drop(gate);
    let say = async {
        let priority = app
            .state::<crate::illustrations::Illustrations>()
            .priority(app.clone(), shared.clone());
        priority.ready().await;
        voice.playback.arm(token.clone())?;
        let pcm = provider.speech(&text).await?;
        drop(priority);
        let receipt = voice
            .playback
            .enqueue(pcm, provider.connections.volume, token.clone())?;
        receipt.started.await.map_err(|_| Error::StaleOperation)?;
        update(app, &shared, tag, |c| c.session.playback_started(tag))?;
        receipt.done.await.map_err(|_| Error::StaleOperation)?;
        Ok::<_, Error>(())
    };
    let result = tokio::select! { biased; _=token.cancelled()=>return, value=say=>value };
    let _ = update(app, &shared, tag, |c| {
        c.session.complete(tag)?;
        if result.is_err() {
            c.live.error = Some(
                "La confirmation vocale n’a pas pu être lue. La conversation a bien été effacée."
                    .into(),
            );
        }
        Ok(())
    });
}

/// Load the conversation model for this turn and adapt the provider to it.
/// Which operation of the session a preparation step works for: the one
/// speaking now, the next speaker generating ahead (`Session::pending`), or a
/// reaction prepared for after the session turn is over (`Detached`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Gate {
    Current,
    Pending,
    /// Prepared for the relay while the user may speak: no session turn
    /// stays open for it, only the relay token (and its cancellation by the
    /// caller) keeps it alive.
    Detached,
}
impl Gate {
    fn accepts(self, core: &sherka_core::Companion, tag: OperationTag) -> bool {
        match self {
            Self::Current => core.session.accepts(tag),
            Self::Pending => core.session.accepts_pending(tag),
            Self::Detached => core.relay_active(),
        }
    }
}

/// The tag of a reaction prepared outside any session operation: the session
/// never accepts it. The turn is the one that holds the exchange it answers.
fn detached_tag(after: OperationTag, profile_id: Uuid) -> OperationTag {
    OperationTag {
        profile_id,
        session_id: after.session_id,
        turn_id: after.turn_id,
        generation_id: Uuid::new_v4(),
    }
}

async fn activate_model(
    voice: &Voice,
    shared: &Shared,
    tag: OperationTag,
    gate: Gate,
    provider: &mut Providers,
) -> std::result::Result<sherka_core::catalogs::LanguageModel, String> {
    let models = voice.models.clone();
    let config = provider.connections.clone();
    let state = shared.clone();
    // Keep load/unload reconciliation alive even if the spoken turn is
    // interrupted while LM Studio is still loading a model.
    let model = tauri::async_runtime::spawn(async move {
        models
            .activate(&config, None, || {
                state.0.lock().is_ok_and(|c| gate.accepts(&c, tag))
            })
            .await
    })
    .await
    .map_err(|_| "Le chargement du modèle a échoué.".to_owned())?
    .map_err(|e| e.to_string())?
    .ok_or("Choisis un modèle de conversation dans Compagnons.")?;
    provider.connections.llm_model = Some(model.id.clone());
    if !provider.images.is_empty() && !provider.supports_images(&model).images {
        return Err(
            "Ce modèle n’accepte pas les images ou sa capacité visuelle n’est pas confirmée."
                .into(),
        );
    }
    provider.use_api_model(&model);
    provider
        .connections
        .validate_reasoning(&model)
        .map_err(|e| e.to_string())?;
    Ok(model)
}

#[allow(clippy::too_many_arguments)]
async fn prepare_turn(
    app: &AppHandle,
    shared: &Shared,
    voice: &Voice,
    tag: OperationTag,
    provider: &mut Providers,
    user: &str,
    event: Option<&AppEvent>,
    peer: Option<&sherka_core::trio::Peer>,
) -> std::result::Result<sherka_core::conversation::PreparedChat, String> {
    let model = activate_model(voice, shared, tag, Gate::Current, provider).await?;
    let peer = peer.cloned();
    let illustrations = app.state::<crate::illustrations::Illustrations>();
    if let Some(event) = event.cloned() {
        // Not a request: an image request still waiting is stale.
        illustrations.forget_wish();
        return with_core(shared.clone(), move |c| {
            if !c.session.accepts(tag) {
                return Err(Error::StaleOperation);
            }
            let mut profile = c.speaking_profile(tag.profile_id)?;
            profile.details.connections.response_tokens =
                sherka_core::trio::response_tokens(256, peer.as_ref().is_some_and(|p| p.quick));
            let budget = sherka_core::conversation::budget(&profile.details, &model)?;
            let history = c.store.context_history(tag.profile_id)?;
            let recall = c.store.recall(tag.profile_id, "")?;
            // The remark after the Image button knows the generator is on.
            let images = matches!(event, AppEvent::ImagesOn { .. })
                .then(|| {
                    sherka_core::illustration::ImagePrompt::new(
                        &profile.details.illustration,
                        true,
                        None,
                    )
                })
                .flatten();
            let mut chat = sherka_core::conversation::prepare_context_with_avatar(
                &profile.details,
                &history,
                &event.prompt(),
                &recall,
                budget,
                None,
                None,
                !profile.emotion_avatars.is_empty() || profile.details.mood_persistence,
                peer.as_ref(),
                &c.trio_settings().user_name,
                sherka_core::trio::Last::Attributed,
                images.as_ref(),
            )?;
            // A bounded invitation uses the normal cancellable TTS pipeline.
            chat.budget.response_tokens = chat.budget.response_tokens.min(256);
            c.live.budget = Some(chat.budget.clone());
            Ok(chat)
        })
        .await;
    }
    let text = user.to_owned();
    let quick_peer = peer.clone();
    let (profile, source, guard, visible, trio) = with_core(shared.clone(), move |c| {
        if !c.session.accepts(tag) {
            return Err(Error::StaleOperation);
        }
        let mut profile = c.speaking_profile(tag.profile_id)?;
        // Quick replies in a three-way turn also shorten the response budget.
        if let Some(peer) = &quick_peer {
            let connections = &mut profile.details.connections;
            connections.response_tokens =
                sherka_core::trio::response_tokens(connections.response_tokens, peer.quick);
        }
        let source = c.store.source_for_turn(tag.profile_id, tag.turn_id)?;
        let guard = c.store.memory_guard(tag.profile_id)?;
        let recall = c.store.recall(tag.profile_id, &text)?;
        let mut visible = recall.details;
        visible.extend(recall.essential);
        for m in c
            .store
            .memories(tag.profile_id, "")?
            .entries
            .into_iter()
            .filter(|m| m.status == "active")
        {
            if !visible.iter().any(|v| v.id == m.id) {
                visible.push(m);
            }
        }
        Ok((profile, source, guard, visible, c.guest().is_some()))
    })
    .await?;
    let (image_plan, images_on) =
        switch_images(app, shared, tag, &profile.details.illustration, trio, user).await;
    let mut budget =
        sherka_core::conversation::budget(&profile.details, &model).map_err(|e| e.to_string())?;
    budget.safety_tokens += provider.images.len() * 4096;
    let mode = with_core(shared.clone(), move |c| {
        c.web_search_mode_for(tag.profile_id)
    })
    .await?;
    let mut web = None;
    if let Some(explicit) = sherka_core::web::explicit_query(user) {
        if mode == sherka_core::web::SearchMode::Off {
            web = Some(sherka_core::web::Report { error: Some("La recherche Internet est désactivée. Active-la dans Réglages pour chercher en ligne.".into()), ..Default::default() });
        } else {
            web =
                Some(search_web(app, shared, tag, explicit, &provider.connections.language).await?);
        }
    } else if mode == sherka_core::web::SearchMode::Auto
        && !sherka_core::memory::memory_intent(user)
        && !sherka_core::web::forbids_search(user)
    {
        update(app, shared, tag, |c| {
            c.live.notice = Some("Je vérifie si une recherche Internet est utile…".into());
            Ok(())
        })
        .map_err(|e| e.to_string())?;
        match provider.web_plan(user, budget.limit).await {
            Ok(plan) if plan.search => {
                web = Some(
                    search_web(
                        app,
                        shared,
                        tag,
                        &plan.query,
                        &provider.connections.language,
                    )
                    .await?,
                )
            }
            Ok(_) => {}
            // A planning failure is not a failed search. Keep the normal
            // conversational reply; only an actual search produces a receipt.
            Err(_) => {}
        }
        update(app, shared, tag, |c| {
            c.live.notice = None;
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    }
    let mut receipt = None;
    if sherka_core::memory::memory_intent(user) {
        let mut visible = visible;
        let (messages, schema) = loop {
            let request = sherka_core::memory::plan_request(user, &visible);
            if sherka_core::conversation::estimate(&request.0) + 1280 <= budget.limit as usize
                || visible.is_empty()
            {
                break request;
            }
            visible.pop();
        };
        let planned = provider
            .structured::<sherka_core::memory::MemoryPlan>(messages, schema, budget.limit)
            .await;
        let applied = match planned {
            Ok(plan) => {
                with_core(shared.clone(), move |c| {
                    if !c.session.accepts(tag) {
                        return Err(Error::StaleOperation);
                    }
                    c.store.apply_memory_plan(
                        &guard,
                        &source,
                        &visible,
                        plan,
                        &tag.generation_id.to_string(),
                    )
                })
                .await
            }
            Err(error) => Err(error.to_string()),
        };
        receipt = Some(match applied {
            Ok(value) => value,
            Err(e) => format!("Aucune modification de mémoire effectuée : {e}"),
        });
        let confirmed = receipt.clone();
        update(app, shared, tag, move |c| {
            c.live.memory_receipt = confirmed;
            Ok(())
        })
        .map_err(|e| e.to_string())?;
        let _ = app.emit("sherka:memory", tag.profile_id);
    }
    let text = user.to_owned();
    let image_receipt = image_plan.receipt();
    let images =
        crate::image_control::prompt(image_plan.tell, &profile.details.illustration, images_on);
    let prepared = with_core(shared.clone(), move |c| {
        if !c.session.accepts(tag) {
            return Err(Error::StaleOperation);
        }
        let source = c.store.source_for_turn(tag.profile_id, tag.turn_id)?;
        let history = c
            .store
            .context_history(tag.profile_id)?
            .into_iter()
            .filter(|m| m.id != source.id)
            .collect::<Vec<_>>();
        let recall = c.store.recall(tag.profile_id, &text)?;
        let prepared = sherka_core::conversation::prepare_context_with_avatar(
            &profile.details,
            &history,
            &text,
            &recall,
            budget,
            receipt.as_deref(),
            web,
            !profile.emotion_avatars.is_empty() || profile.details.mood_persistence,
            peer.as_ref(),
            &c.trio_settings().user_name,
            sherka_core::trio::Last::User,
            images.as_ref(),
        )?;
        c.live.budget = Some(prepared.budget.clone());
        c.live.web = prepared.web.clone();
        c.live.image_receipt = image_receipt;
        Ok(prepared)
    })
    .await?;
    emit(app, shared);
    Ok(prepared)
}

/// Decides what the user's words ask of the image generator and does it, before
/// the reply, so that the companion can tell the truth. The switch is the one
/// of the widget button; the turn is not cancelled by it: it only saves the
/// profile, synchronizes the illustrations (their own token) and notifies.
/// Returns the decision and whether the generator is on afterwards.
async fn switch_images(
    app: &AppHandle,
    shared: &Shared,
    tag: OperationTag,
    config: &sherka_core::illustration::Config,
    trio: bool,
    user: &str,
) -> (crate::image_control::Plan, bool) {
    use crate::image_control::{Facts, Tell, Wish, plan};
    use sherka_core::illustration::{ImageIntent, ImageReceipt};
    let images = app.state::<crate::illustrations::Illustrations>();
    let intent = sherka_core::illustration::image_intent(user);
    let facts = Facts {
        enabled: config.enabled,
        ready: sherka_core::illustration::generator_ready(config),
        trio,
        wished: images.has_wish(tag.profile_id, tag.session_id),
    };
    let asks_on =
        intent == Some(ImageIntent::Make) && sherka_core::illustration::asks_switch_on(user);
    let mut plan = plan(intent, asks_on, facts);
    let mut enabled = config.enabled;
    if let Some(on) = plan.switch {
        match crate::desktop_tools::set_image_enabled(app, shared.clone(), on).await {
            Ok(()) => enabled = on,
            Err(_) => plan = plan.failed(),
        }
    }
    if plan.tell == Tell::State(Some(ImageReceipt::SwitchedOn)) {
        // A command exchange is not illustrated, not even by a live resume.
        images.skip_resume();
    }
    match plan.wish {
        Wish::Keep => {}
        Wish::Clear => images.forget_wish(),
        Wish::Store => images.keep_wish(tag.profile_id, tag.session_id, user),
    }
    (plan, enabled)
}

/// The second speaker's context: its own history and memories, with the first
/// speaker's line as the final message. No memory commands, no web search.
/// A `Gate::Pending` preparation leaves the live state alone and notifies
/// nobody; the caller publishes the returned budget when the speaker takes over.
#[allow(clippy::too_many_arguments)]
async fn prepare_peer(
    app: &AppHandle,
    shared: &Shared,
    voice: &Voice,
    tag: OperationTag,
    gate: Gate,
    provider: &mut Providers,
    follow: &str,
    peer: &sherka_core::trio::Peer,
    first_generation: Uuid,
    user_turn: Uuid,
    spontaneous: bool,
) -> std::result::Result<sherka_core::conversation::PreparedChat, String> {
    let model = activate_model(voice, shared, tag, gate, provider).await?;
    let (follow, peer) = (follow.to_owned(), peer.clone());
    let prepared = with_core(shared.clone(), move |c| {
        if !gate.accepts(c, tag) {
            return Err(Error::StaleOperation);
        }
        let mut profile = c.speaking_profile(tag.profile_id)?;
        let connections = &mut profile.details.connections;
        if spontaneous {
            connections.response_tokens = 256;
        }
        connections.response_tokens =
            sherka_core::trio::response_tokens(connections.response_tokens, peer.quick);
        let budget = sherka_core::conversation::budget(&profile.details, &model)?;
        // The user's message belongs to `user_turn`, which is an earlier turn
        // when this is a relay.
        let user_row = c
            .store
            .source_for_turn(tag.profile_id, user_turn)
            .ok()
            .map(|source| source.id);
        let excluded = peer_exclusions(user_row, first_generation);
        let history = c
            .store
            .context_history(tag.profile_id)?
            .into_iter()
            .filter(|m| !excluded.contains(&m.id))
            .collect::<Vec<_>>();
        let recall = c.store.recall(tag.profile_id, &follow)?;
        let mut chat = sherka_core::conversation::prepare_context_with_avatar(
            &profile.details,
            &history,
            &follow,
            &recall,
            budget,
            None,
            None,
            !profile.emotion_avatars.is_empty() || profile.details.mood_persistence,
            Some(&peer),
            &c.trio_settings().user_name,
            sherka_core::trio::Last::Attributed,
            None,
        )?;
        if spontaneous {
            chat.budget.response_tokens = chat.budget.response_tokens.min(256);
        }
        if gate == Gate::Current {
            c.live.budget = Some(chat.budget.clone());
        }
        Ok(chat)
    })
    .await?;
    if gate == Gate::Current {
        emit(app, shared);
    }
    Ok(prepared)
}

async fn search_web(
    app: &AppHandle,
    shared: &Shared,
    tag: OperationTag,
    query: &str,
    language: &str,
) -> std::result::Result<sherka_core::web::Report, String> {
    let checked = sherka_core::web::query(query);
    let mut report = sherka_core::web::Report {
        query: checked.as_ref().cloned().unwrap_or_default(),
        searching: true,
        ..Default::default()
    };
    update(app, shared, tag, |c| {
        c.live.web = Some(report.clone());
        Ok(())
    })
    .map_err(|e| e.to_string())?;
    let result = match checked {
        Ok(query) => {
            let config = with_core(shared.clone(), move |c| {
                if !c.session.accepts(tag) {
                    return Err(Error::StaleOperation);
                }
                Ok(c.search_config())
            })
            .await?;
            sherka_core::search_provider::search(&config, &query, language).await
        }
        Err(error) => Err(error),
    };
    report.searching = false;
    match result {
        Ok(sources) => report.sources = sources,
        Err(error) => report.error = Some(error.to_string()),
    }
    update(app, shared, tag, |c| {
        c.live.web = Some(report.clone());
        Ok(())
    })
    .map_err(|e| e.to_string())?;
    Ok(report)
}

/// Extract memories once the conversation is quiet: one job, and in a
/// three-way conversation one companion after the other.
fn schedule_memory(app: AppHandle, shared: Shared, profiles: Vec<Uuid>, provider: Providers) {
    let token = match shared.0.lock() {
        Ok(c) => c.memory_job.clone(),
        Err(_) => return,
    };
    tauri::async_runtime::spawn(async move {
        let job = async {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            for profile in profiles {
                extract_memories(&app, &shared, &token, &provider, profile).await;
            }
        };
        tokio::select! {biased; _=token.cancelled()=>{}, _=job=>{}}
    });
}

/// How long an extraction waits for the companions to fall silent.
const MEMORY_IDLE_LIMIT: std::time::Duration = std::time::Duration::from_secs(300);

/// The master or the guest.
fn memory_participant(c: &sherka_core::Companion, profile: Uuid) -> bool {
    c.session.snapshot().profile_id == profile || c.guest() == Some(profile)
}

/// What an extraction does now: go ahead, wait (a reply is being produced, as
/// during the relay of a three-way conversation) or give up.
#[derive(Debug, PartialEq)]
enum MemoryTurn {
    Go,
    Wait,
    Stale,
}

fn memory_turn(c: &sherka_core::Companion, profile: Uuid) -> MemoryTurn {
    match (memory_participant(c, profile), c.live.tag.is_none()) {
        (false, _) => MemoryTurn::Stale,
        (true, false) => MemoryTurn::Wait,
        (true, true) => MemoryTurn::Go,
    }
}

/// Run `job` on the core as soon as the companions are silent: poll every
/// second, for at most `MEMORY_IDLE_LIMIT`, until `token` is cancelled.
async fn when_quiet<T: Send + 'static>(
    shared: &Shared,
    token: &CancellationToken,
    profile: Uuid,
    job: impl Fn(&mut sherka_core::Companion) -> Result<T> + Send + Sync + 'static,
) -> Option<T> {
    let job = std::sync::Arc::new(job);
    let started = std::time::Instant::now();
    loop {
        let (job, cancelled) = (job.clone(), token.clone());
        let done = with_core(shared.clone(), move |c| {
            if cancelled.is_cancelled() {
                return Err(Error::StaleOperation);
            }
            match memory_turn(c, profile) {
                MemoryTurn::Go => job(c).map(Some),
                MemoryTurn::Wait => Ok(None),
                MemoryTurn::Stale => Err(Error::StaleOperation),
            }
        })
        .await
        .ok()?;
        if done.is_some() || started.elapsed() >= MEMORY_IDLE_LIMIT {
            return done;
        }
        tokio::select! {
            biased;
            _ = token.cancelled() => return None,
            _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {}
        }
    }
}

async fn extract_memories(
    app: &AppHandle,
    shared: &Shared,
    token: &CancellationToken,
    provider: &Providers,
    profile: Uuid,
) -> Option<()> {
    let batch = when_quiet(shared, token, profile, move |c| {
        c.store.extraction_batch(profile)
    })
    .await
    .flatten()?;
    let catalog = provider.catalog().await.ok()?;
    let model =
        sherka_core::models::selected(&catalog.models, provider.connections.llm_model.as_deref()?)
            .ok()?;
    if provider.connections.llm_provider.is_local() && model.loaded != Some(true) {
        return None;
    }
    let limit = if provider.connections.llm_provider.is_local() {
        model.loaded_context_length?
    } else {
        model
            .max_context_length
            .or(provider.connections.context_limit.map(u64::from))
            .unwrap_or(sherka_core::connections::FALLBACK_CONTEXT_TOKENS)
    };
    let (messages, schema) = sherka_core::memory::consolidation_request(&batch);
    let priority = app
        .state::<crate::illustrations::Illustrations>()
        .priority(app.clone(), shared.clone());
    priority.ready().await;
    let value = provider
        .structured::<sherka_core::memory::Consolidation>(
            messages,
            schema,
            limit.min(
                provider
                    .connections
                    .context_limit
                    .map(u64::from)
                    .unwrap_or(limit),
            ),
        )
        .await
        .ok()?;
    // The reply may have started while the model was thinking: wait for it too.
    let value = std::sync::Mutex::new(Some(value));
    when_quiet(shared, token, profile, move |c| {
        let value = value
            .lock()
            .ok()
            .and_then(|mut slot| slot.take())
            .ok_or(Error::StaleOperation)?;
        c.store.consolidate(&batch, value)
    })
    .await?;
    let _ = app.emit("sherka:memory", profile);
    Some(())
}

pub fn switch_model(
    app: AppHandle,
    shared: Shared,
    voice: &Voice,
    previous: sherka_core::connections::Connections,
) {
    let models = voice.models.clone();
    let (profile, revision, next) = match shared.0.lock() {
        Ok(c) => match c.profile(c.session.snapshot().profile_id) {
            Ok(p) => (p.id, p.revision, p.details.connections),
            Err(_) => return,
        },
        Err(_) => return,
    };
    let priority = app
        .state::<crate::illustrations::Illustrations>()
        .priority(app.clone(), shared.clone());
    tauri::async_runtime::spawn(async move {
        priority.ready().await;
        let valid = || {
            shared.0.lock().is_ok_and(|c| {
                c.session.snapshot().profile_id == profile
                    && c.profile(profile).is_ok_and(|p| p.revision == revision)
            })
        };
        let result = models.activate(&next, Some(&previous), valid).await;
        if let Ok(mut c) = shared.0.lock()
            && c.session.snapshot().profile_id == profile
            && c.profile(profile).is_ok_and(|p| p.revision == revision)
        {
            match result {
                Ok(Some(model)) => {
                    c.live.notice = Some(if next.llm_provider.is_local() {
                        format!(
                            "{} chargé · contexte : {} tokens.",
                            model.label,
                            model.loaded_context_length.unwrap_or(0)
                        )
                    } else {
                        format!(
                            "{} sélectionné chez {}.",
                            model.label,
                            next.llm_provider.label()
                        )
                    })
                }
                Ok(None) => {
                    c.live.notice = Some(if next.llm_provider.is_agent() {
                        "Hermes gère la conversation, sa mémoire et ses outils.".into()
                    } else {
                        "Choisis un modèle pour ce compagnon dans les réglages.".into()
                    })
                }
                Err(Error::StaleOperation) => {}
                Err(e) => c.live.error = Some(e.to_string()),
            }
        }
        emit(&app, &shared);
    });
}

/// One companion's part in a three-way turn.
enum TrioStep {
    /// Answers first, or alone when named. `continues`: the other companion
    /// speaks next, so the turn stays open after this reply.
    First {
        peer: sherka_core::trio::Peer,
        continues: bool,
        /// Tells the prefetching second speaker how far this reply has come.
        signals: Option<FirstSignals>,
    },
    /// Replies to the first speaker's line and may stay silent.
    Second {
        peer: sherka_core::trio::Peer,
        follow: String,
        first_generation: Uuid,
        /// The turn that holds the user's message (a relay has a turn of its own).
        user_turn: Uuid,
        /// The reply was already generated (and its start synthesized) while
        /// the first speaker was talking; `None` generates it now.
        prefetch: Option<Box<Prefetch>>,
        /// A reply of the relay chain keeps the turn open and tells the
        /// speaker who follows how far it has come, like `First`.
        signals: Option<FirstSignals>,
        continues: bool,
    },
}
impl TrioStep {
    fn peer(&self) -> &sherka_core::trio::Peer {
        match self {
            Self::First { peer, .. } | Self::Second { peer, .. } => peer,
        }
    }
}

/// How one `run_turn` ended.
enum TurnEnd {
    /// The reply was saved. `memory`: its history may feed memory extraction.
    Done {
        reply: String,
        provider: Box<Providers>,
        memory: bool,
    },
    /// The second speaker chose silence; nothing was saved.
    Passed,
    /// Cancelled, failed, or replaced by a maintenance command.
    Stopped,
}

/// Holds back the start of a second speaker's reply while it could still be
/// `[[pass]]`. Without a filter (everybody else) text goes straight through.
struct PassGate {
    filter: Option<sherka_core::trio::PassFilter>,
    passed: bool,
}
impl PassGate {
    fn new(second: bool) -> Self {
        Self {
            filter: second.then(sherka_core::trio::PassFilter::new),
            passed: false,
        }
    }
    /// The text to show and speak now, if any.
    fn push(&mut self, text: String) -> Option<String> {
        use sherka_core::trio::PassOutput;
        let Some(filter) = &mut self.filter else {
            return Some(text);
        };
        match filter.push(&text) {
            PassOutput::Pending => None,
            PassOutput::Text(text) => Some(text),
            PassOutput::Passed => {
                self.passed = true;
                None
            }
        }
    }
    /// End of the reply: text still held back, or the decision to stay silent
    /// (an empty reply counts as silence).
    fn finish(&mut self) -> Option<String> {
        use sherka_core::trio::PassOutput;
        match self.filter.as_mut()?.finish() {
            PassOutput::Text(text) => Some(text),
            PassOutput::Passed => {
                self.passed = true;
                None
            }
            PassOutput::Pending => None,
        }
    }
}

/// What the listener sees and hears of a streamed reply: the expression header
/// and, for a second speaker, a leading `[[pass]]` removed, emoji stripped. The
/// live reply and the prefetch both use it, so they produce the same text.
struct ReplyFilter {
    expression: sherka_core::avatar::ExpressionStream,
    expressions: bool,
    gate: PassGate,
}
impl ReplyFilter {
    fn new(expressions: bool, second: bool) -> Self {
        Self {
            expression: Default::default(),
            expressions,
            gate: PassGate::new(second),
        }
    }
    fn emotion(&self) -> sherka_core::avatar::Emotion {
        self.expression.emotion
    }
    fn passed(&self) -> bool {
        self.gate.passed
    }
    /// The visible text of one streamed chunk, if any (possibly empty).
    fn push(&mut self, chunk: &str) -> Option<String> {
        let text = if self.expressions {
            self.expression.push(chunk)
        } else {
            chunk.to_owned()
        };
        self.gate
            .push(text)
            .map(|text| sherka_core::conversation::plain_reply(&text))
    }
    /// The text still held back at the end of the stream.
    fn finish(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        if self.expressions
            && let Some(text) = self.gate.push(self.expression.finish())
        {
            out.push(sherka_core::conversation::plain_reply(&text));
        }
        if let Some(text) = self.gate.finish() {
            out.push(sherka_core::conversation::plain_reply(&text));
        }
        out
    }
}

/// A synthesized passage: its text and its PCM.
type Passage = (String, Vec<i16>);
/// Passages of the prefetched reply synthesized while the first speaker is
/// still audible. More would only delay the second speaker's own start.
const PRESYNTH: usize = 2;
/// Tokens of the prefetched reply buffered for the live replay.
const PREFETCH_TOKENS: usize = 8192;

/// How far the first speaker's reply has come, for the second one's prefetch.
/// A sender dropped unsent means the reply failed or was cut: nothing to follow.
struct FirstSignals {
    /// The text is complete: its visible reply. The language model is free.
    text: tokio::sync::oneshot::Sender<String>,
    /// Every passage is synthesized (or nothing is spoken). VoiceStudio is free.
    synthesized: tokio::sync::oneshot::Sender<()>,
}

/// The first passages of a prefetched reply, synthesized ahead of its turn.
struct PcmCache {
    passages: mpsc::Receiver<Passage>,
    /// Stops the synthesis when the cache is dropped.
    _halt: Option<tokio_util::sync::DropGuard>,
}
impl PcmCache {
    /// The audio of `phrase` if it is the next prepared passage. Waits for a
    /// passage still being synthesized rather than requesting it twice. A
    /// different phrase (chunking is deterministic, so this is rare) or an
    /// exhausted cache drops the cache: the caller synthesizes from there on.
    async fn take(cache: &mut Option<Self>, phrase: &str) -> Option<Vec<i16>> {
        match cache.as_mut()?.passages.recv().await {
            Some((cached, pcm)) if cached == phrase => Some(pcm),
            _ => {
                *cache = None;
                None
            }
        }
    }
}

/// The second speaker's reply, started while the first one is still speaking:
/// context built, language model running, first passages synthesized. It owns
/// no live state; `run_trio_turn` promotes its tag when the first speaker is done.
struct Prefetch {
    /// The follow-up message it answers; a mismatch discards the prefetch.
    follow: String,
    tag: OperationTag,
    /// Model activated; the same provider synthesizes the speech.
    provider: Providers,
    /// Without its messages: only the budget and the expression flag are read.
    chat: sherka_core::conversation::PreparedChat,
    /// Every raw token of the reply, from the first; the live pipeline replays them.
    tokens: mpsc::Receiver<String>,
    generation: tauri::async_runtime::JoinHandle<Result<sherka_core::providers::ChatOutcome>>,
    cache: Option<PcmCache>,
}

/// Who the prefetch is for and what it needs from the turn.
struct PrefetchPlan {
    /// `Pending` (the turn stays open) or `Detached` (the relay token alone).
    gate: Gate,
    first_tag: OperationTag,
    first: sherka_core::trio::Participant,
    second: Uuid,
    user: Option<String>,
    /// How the companions call the user (empty: "the user").
    user_name: String,
    quick: bool,
    spontaneous: bool,
    speak: bool,
}

/// Generate the second speaker's reply ahead of its turn. Waits for the first
/// reply's text (so the language model is free), then runs the second speaker's
/// context and generation under a tag that cannot touch the live state: a
/// pending one of the open turn (`Gate::Pending`), or a detached one
/// (`Gate::Detached`) for a reaction that comes after the turn is over and the
/// user may speak. Once the first reply is synthesized (VoiceStudio is free) it
/// synthesizes the first passages. Any failure returns `None` and the turn runs
/// as before. `token` ends everything: for a pending prefetch it is a child of
/// the turn's token, for a detached one a child of the relay token, so the
/// user's activity drops the language-model stream and the synthesis at once.
async fn start_prefetch(
    app: AppHandle,
    shared: Shared,
    voice: Voice,
    token: CancellationToken,
    plan: PrefetchPlan,
    text: tokio::sync::oneshot::Receiver<String>,
    synthesized: tokio::sync::oneshot::Receiver<()>,
) -> Option<Prefetch> {
    let reply = tokio::select! {biased; _=token.cancelled()=>return None, reply=text=>reply.ok()?};
    let PrefetchPlan {
        gate,
        first_tag,
        first,
        second,
        user,
        user_name,
        quick,
        spontaneous,
        speak,
    } = plan;
    let follow = sherka_core::trio::follow_message(user.as_deref(), &user_name, &first, &reply);
    let tag = match gate {
        Gate::Detached => detached_tag(first_tag, second),
        _ => with_core(shared.clone(), move |c| {
            c.session.begin_pending_peer(first_tag, second)
        })
        .await
        .ok()?,
    };
    let mut provider = speaker_provider(&shared, second).await.ok()?;
    let peer = sherka_core::trio::Peer {
        name: first.name,
        second: true,
        quick,
    };
    let mut chat = tokio::select! {biased; _=token.cancelled()=>return None, chat=prepare_peer(
        &app,
        &shared,
        &voice,
        tag,
        gate,
        &mut provider,
        &follow,
        &peer,
        first_tag.generation_id,
        first_tag.turn_id,
        spontaneous,
    )=>chat.ok()?};
    let messages = std::mem::take(&mut chat.messages);
    let max_tokens = chat.budget.response_tokens;
    let expressions = chat.avatar_emotions;
    let (llm_tx, mut llm_rx) = mpsc::channel::<String>(64);
    let (tokens_tx, tokens) = mpsc::channel::<String>(PREFETCH_TOKENS);
    let (generating, stop) = (provider.clone(), token.clone());
    let generation = tauri::async_runtime::spawn(async move {
        tokio::select! {
            biased;
            _=stop.cancelled()=>Err(Error::StaleOperation),
            outcome=generating.chat_budgeted(messages, llm_tx, max_tokens)=>outcome
        }
    });
    // The text the listener will get, chunked as the live pipeline does, so the
    // first passages can be synthesized before that pipeline starts.
    let speech = SpeechText::shared();
    let (writer, stop) = (speech.clone(), token.clone());
    tauri::async_runtime::spawn(async move {
        let mut filter = ReplyFilter::new(expressions, true);
        loop {
            let next =
                tokio::select! {biased; _=stop.cancelled()=>None, token=llm_rx.recv()=>token};
            let Some(raw) = next else { break };
            if speak && let Some(text) = filter.push(&raw) {
                let _ = writer.push(&text);
            }
            if tokens_tx.send(raw).await.is_err() {
                break;
            }
        }
        if speak {
            for text in filter.finish() {
                let _ = writer.push(&text);
            }
        }
        writer.finish();
    });
    let cache = speak.then(|| {
        let (passages_tx, passages) = mpsc::channel::<Passage>(PRESYNTH);
        let halt = token.child_token();
        let (stop, synth) = (halt.clone(), provider.clone());
        tauri::async_runtime::spawn(async move {
            tokio::select! {biased; _=stop.cancelled()=>return, free=synthesized=>if free.is_err() { return }}
            for _ in 0..PRESYNTH {
                let phrase = tokio::select! {biased; _=stop.cancelled()=>None, next=speech.next()=>next.ok().flatten()};
                let Some(phrase) = phrase else { break };
                let pcm = tokio::select! {biased; _=stop.cancelled()=>None, pcm=synth.speech(&phrase)=>pcm.ok()};
                let Some(pcm) = pcm else { break };
                if passages_tx.send((phrase, pcm)).await.is_err() {
                    break;
                }
            }
        });
        PcmCache {
            passages,
            _halt: Some(halt.drop_guard()),
        }
    });
    Some(Prefetch {
        follow,
        tag,
        provider,
        chat,
        tokens,
        generation,
        cache,
    })
}

/// Who speaks, in order. One companion answers the user (the addressee, else
/// the coin). A spontaneous turn has no addressee: a coin decides who opens and
/// the other reacts, unless the event is for the master alone (`alone`).
fn speaking_order(
    user: Option<&str>,
    master: &sherka_core::trio::Participant,
    guest: &sherka_core::trio::Participant,
    coin: bool,
    alone: bool,
) -> Vec<Uuid> {
    match user {
        // The user is answered by one companion; the other waits for the relay.
        Some(text) => vec![sherka_core::trio::answerer(text, master, guest, coin)],
        None if alone => vec![master.id],
        None if coin => vec![master.id, guest.id],
        None => vec![guest.id, master.id],
    }
}

/// Rows of the second speaker's own history that the follow-up message
/// already carries: the user's message of this turn and the first reply.
fn peer_exclusions(user_row: Option<String>, first_generation: Uuid) -> Vec<String> {
    user_row
        .into_iter()
        .chain([first_generation.to_string()])
        .collect()
}

/// The provider a trio speaker generates with: its own voice and language,
/// the master's language model.
async fn speaker_provider(shared: &Shared, id: Uuid) -> std::result::Result<Providers, String> {
    let profile = with_core(shared.clone(), move |c| c.speaking_profile(id)).await?;
    Providers::new(profile.details.connections).map_err(|e| e.to_string())
}

/// A privacy or maintenance command in the master's message runs instead of a
/// reply. True when one was handled.
async fn handle_maintenance(
    app: &AppHandle,
    shared: &Shared,
    voice: &Voice,
    tag: OperationTag,
    user: &str,
    speak: bool,
) -> bool {
    let command = shared.0.lock().ok().and_then(|mut c| {
        let command = c.profile(tag.profile_id).ok().and_then(|p| {
            sherka_core::privacy::contextual_command(
                user,
                &p.details.name,
                c.pending_clear.valid(tag.profile_id, tag.session_id),
            )
        });
        if command.is_none() {
            c.pending_clear = Default::default();
        }
        command
    });
    let Some(command) = command else {
        return false;
    };
    if let Err(error) = maintenance_command(
        app,
        shared.clone(),
        voice,
        tag.profile_id,
        command,
        speak,
        Some(tag),
    )
    .await
    {
        if let Ok(mut c) = shared.0.lock()
            && c.session.snapshot().profile_id == tag.profile_id
            && c.session.snapshot().session_id == tag.session_id
        {
            c.live.error = Some(error);
        }
        emit(app, shared);
    }
    true
}

/// A three-way turn: the companion(s) answering the user, or opening a
/// spontaneous exchange when `user` is `None`, one after the other. `tag` and
/// `provider` are the master's; one session turn and one cancellation token
/// cover every speaker, so a barge-in stops them all.
#[allow(clippy::too_many_arguments)]
async fn run_trio_turn(
    app: AppHandle,
    shared: Shared,
    voice: Voice,
    tag: OperationTag,
    token: CancellationToken,
    mut provider: Providers,
    user: Option<String>,
    event: Option<AppEvent>,
    speak: bool,
    priority: crate::illustrations::Priority,
) {
    use sherka_core::trio;
    let spontaneous = user.is_none();
    if let Some(text) = &user {
        // Maintenance commands belong to the master and are not answered.
        tokio::select! {biased;_=token.cancelled()=>return,_=priority.ready()=>{}}
        if handle_maintenance(&app, &shared, &voice, tag, text, speak).await {
            return;
        }
    }
    let Some(((master, guest), settings)) = shared
        .0
        .lock()
        .ok()
        .and_then(|c| Some((c.participants()?, c.trio_settings())))
    else {
        return;
    };
    let quick = settings.quick_replies;
    let text = user.clone().unwrap_or_else(|| {
        event
            .as_ref()
            .map_or(AppEvent::Idle.label(), AppEvent::label)
            .into()
    });
    let alone = event.as_ref().is_some_and(AppEvent::master_only);
    let order = speaking_order(user.as_deref(), &master, &guest, trio::coin(), alone);
    let named = |id: Uuid| {
        if id == master.id {
            master.clone()
        } else {
            guest.clone()
        }
    };
    let other = |id: Uuid| {
        if id == master.id {
            guest.clone()
        } else {
            master.clone()
        }
    };
    let first = named(order[0]);
    let mut first_tag = tag;
    if first.id != tag.profile_id {
        let id = first.id;
        let begun = with_core(shared.clone(), move |c| {
            let next = c.session.begin_peer(tag, id)?;
            // The turn's own reset already settled the memory extraction.
            c.reset_live_for_event(next);
            Ok(next)
        })
        .await;
        let Ok(next) = begun else { return };
        first_tag = next;
        match speaker_provider(&shared, id).await {
            Ok(mut next) => {
                next.images = std::mem::take(&mut provider.images);
                provider = next;
            }
            Err(error) => {
                fail_turn(&app, &shared, first_tag, &error).await;
                return;
            }
        }
    }
    let ids = [master.id, guest.id];
    let announce = |app: &AppHandle| {
        for id in ids {
            let _ = app.emit("sherka:history", id);
        }
    };
    // After a user turn and the arrival remark the other companion waits for
    // the cooldown, so the relay is armed now: its token keeps the reaction
    // prepared during the answer alive, and the user's activity ends both.
    let relay_token =
        if relay_follows(user.as_deref(), event.as_ref()) && settings.relay_seconds > 0 {
            with_core(shared.clone(), move |c| {
                Ok(c.session.accepts(first_tag).then(|| c.arm_relay()))
            })
            .await
            .ok()
            .flatten()
        } else {
            None
        };
    // The second speaker prepares its reply while the first one speaks. This
    // token ends that work, whatever the reason the first speaker stops; it is
    // cancelled when this function returns, after the second speaker's turn.
    let ahead = token.child_token();
    let _halt = ahead.clone().drop_guard();
    let mut signals = None;
    let mut prefetching = None;
    // The reaction prepared for after the cooldown, detached from this turn.
    let mut detached = None;
    if let Some(&second_id) = order.get(1) {
        let (text_tx, text_rx) = tokio::sync::oneshot::channel();
        let (synth_tx, synth_rx) = tokio::sync::oneshot::channel();
        signals = Some(FirstSignals {
            text: text_tx,
            synthesized: synth_tx,
        });
        prefetching = Some(tauri::async_runtime::spawn(start_prefetch(
            app.clone(),
            shared.clone(),
            voice.clone(),
            ahead.clone(),
            PrefetchPlan {
                gate: Gate::Pending,
                first_tag,
                first: first.clone(),
                second: second_id,
                user: user.clone(),
                user_name: settings.user_name.clone(),
                quick,
                spontaneous,
                speak,
            },
            text_rx,
            synth_rx,
        )));
    } else if let Some(relay) = &relay_token {
        let (ready, prepared) = prepare_ahead(
            &app,
            &shared,
            &voice,
            relay,
            PrefetchPlan {
                gate: Gate::Detached,
                first_tag,
                first: first.clone(),
                second: other(first.id).id,
                user: user.clone(),
                user_name: settings.user_name.clone(),
                quick,
                spontaneous: false,
                speak,
            },
        );
        signals = Some(ready);
        detached = Some(prepared);
    }
    let first_end = run_turn(
        app.clone(),
        shared.clone(),
        voice.clone(),
        first_tag,
        token.clone(),
        provider,
        text.clone(),
        speak,
        priority,
        event.clone(),
        Some(TrioStep::First {
            peer: trio::Peer {
                name: other(first.id).name,
                second: false,
                quick,
            },
            continues: order.len() > 1,
            signals,
        }),
    )
    .await;
    let TurnEnd::Done {
        reply,
        provider: first_provider,
        memory,
    } = first_end
    else {
        announce(&app);
        return;
    };
    let mut memory_provider = *first_provider;
    let mut stopped = false;
    // The last spoken line, and passes in a row: where the relay takes over.
    let mut last =
        (!reply.trim().is_empty()).then(|| (first.clone(), reply.clone(), first_tag.generation_id));
    let mut passes = 0;
    if let Some(&second_id) = order.get(1) {
        if reply.trim().is_empty() {
            // Nothing to react to: end the turn here.
            let _ = with_core(shared.clone(), move |c| c.discard_generation(first_tag)).await;
        } else {
            let follow = trio::follow_message(user.as_deref(), &settings.user_name, &first, &reply);
            // The reply generated ahead, if it is ready and answers this very text.
            let mut prefetch = match prefetching {
                Some(handle) => tokio::select! {biased; _=token.cancelled()=>None, ready=handle=>ready.ok().flatten()},
                None => None,
            }
            .filter(|p| p.follow == follow);
            if prefetch.is_none() {
                // Nothing to promote: stop any work still running ahead.
                ahead.cancel();
            }
            let ahead_tag = prefetch.as_ref().map(|p| (p.tag, p.chat.budget.clone()));
            let begun = with_core(shared.clone(), move |c| {
                hand_over(c, first_tag, second_id, ahead_tag, false)
            })
            .await;
            if begun.is_ok() && prefetch.is_some() {
                emit(&app, &shared);
            }
            match begun {
                // The user interrupted between the two speakers.
                Err(_) => stopped = true,
                Ok(second_tag) => match match &prefetch {
                    Some(ahead) => Ok(ahead.provider.clone()),
                    None => speaker_provider(&shared, second_id).await,
                } {
                    Err(error) => {
                        fail_turn(&app, &shared, second_tag, &error).await;
                        stopped = true;
                    }
                    Ok(second_provider) => {
                        let priority = app
                            .state::<crate::illustrations::Illustrations>()
                            .priority(app.clone(), shared.clone());
                        let end = run_turn(
                            app.clone(),
                            shared.clone(),
                            voice.clone(),
                            second_tag,
                            token.clone(),
                            second_provider,
                            text,
                            speak,
                            priority,
                            event.clone(),
                            Some(TrioStep::Second {
                                peer: trio::Peer {
                                    name: first.name.clone(),
                                    second: true,
                                    quick,
                                },
                                follow,
                                first_generation: first_tag.generation_id,
                                user_turn: first_tag.turn_id,
                                prefetch: prefetch.take().map(Box::new),
                                signals: None,
                                continues: false,
                            }),
                        )
                        .await;
                        match end {
                            TurnEnd::Done {
                                provider,
                                reply: second_reply,
                                ..
                            } => {
                                memory_provider = *provider;
                                last = Some((
                                    named(second_id),
                                    second_reply,
                                    second_tag.generation_id,
                                ));
                            }
                            TurnEnd::Passed => passes = 1,
                            TurnEnd::Stopped => stopped = true,
                        }
                    }
                },
            }
        }
    }
    // Both companions' histories feed memory, one after the other.
    if memory && !spontaneous && !stopped {
        memory_provider.images.clear();
        schedule_memory(app.clone(), shared.clone(), ids.to_vec(), memory_provider);
    }
    emit(&app, &shared);
    announce(&app);
    // The exchange ended normally and the companions wait for the user: if the
    // user stays silent for the cooldown, the other companion reacts, then
    // they chain replies (see `schedule_relay`). After a user turn and after
    // the arrival remark; solo turns, idle invitations and a stop do not.
    let wait = relay_follows(user.as_deref(), event.as_ref());
    if wait && (stopped || last.is_none() || relay_token.is_none()) {
        // Why the companions do not wait for the user after this exchange.
        let why = if stopped {
            "stopped"
        } else if last.is_none() {
            "no_reply"
        } else if settings.relay_seconds == 0 {
            "off"
        } else {
            "cancelled"
        };
        voice.diagnose(&shared, "trio_relay_skipped", Some(why));
    }
    if let (false, Some((last, last_reply, last_generation)), Some(token)) =
        (stopped, last, relay_token)
    {
        schedule_relay(
            app,
            shared,
            voice,
            RelayPlan {
                token,
                ahead: detached,
                master,
                guest,
                session_id: first_tag.session_id,
                // None after the arrival remark: there is no user message.
                user: user.clone(),
                user_turn: first_tag.turn_id,
                last,
                last_reply,
                last_generation,
                state: sherka_core::trio::RelayState {
                    passes,
                    ..Default::default()
                },
                speak,
            },
        );
    }
}

/// Whether the companions wait for the user after an exchange that ended
/// normally, and start the silence relay: after a user turn and after the
/// arrival remark (an event that `arms_relay`), not after an idle invitation
/// or a spontaneous exchange.
fn relay_follows(user: Option<&str>, event: Option<&AppEvent>) -> bool {
    user.is_some() || event.is_some_and(AppEvent::arms_relay)
}

/// The next speaker of an open turn takes over: the prepared one (`ahead`) is
/// promoted, otherwise a fresh one begins. `keep_relay`: the speaker belongs to
/// the relay chain, which must not cancel itself; otherwise it is the second
/// speaker of a spontaneous exchange, which ends the relay. Neither cancels a
/// pending memory extraction.
fn hand_over(
    c: &mut sherka_core::Companion,
    after: OperationTag,
    next: Uuid,
    ahead: Option<(OperationTag, sherka_core::conversation::ContextBudget)>,
    keep_relay: bool,
) -> Result<OperationTag> {
    let tag = match &ahead {
        Some((tag, _)) => {
            c.session.promote_pending(*tag)?;
            *tag
        }
        None => c.session.begin_peer(after, next)?,
    };
    if keep_relay {
        c.reset_live_keeping_relay(tag);
    } else {
        c.reset_live_for_event(tag);
    }
    if let Some((_, budget)) = ahead {
        c.live.budget = Some(budget);
    }
    Ok(tag)
}

/// Where the companions stand once an exchange with the user ended normally:
/// they wait for the user (see `schedule_relay`).
struct RelayPlan {
    /// Armed before the answer (`run_trio_turn`), so that the reaction could
    /// be prepared during it; ended by the user's activity.
    token: CancellationToken,
    /// That reaction, if it is being prepared (`Gate::Detached`).
    ahead: Option<Ahead>,
    master: sherka_core::trio::Participant,
    guest: sherka_core::trio::Participant,
    session_id: Uuid,
    /// The user's message, until a relay reply has been spoken: the follow-up
    /// message carries it, so the user's row is left out of the history.
    user: Option<String>,
    /// The turn that holds the user's message.
    user_turn: Uuid,
    /// Whose line the next reply reacts to: the author of the last spoken reply.
    last: sherka_core::trio::Participant,
    last_reply: String,
    last_generation: Uuid,
    state: sherka_core::trio::RelayState,
    speak: bool,
}

/// Wait for the cooldown, then let the other companion react; after a quick
/// second silence the companions chain replies until the user cuts in
/// (`run_relay`). One token covers all of it: the core cancels it on a new
/// turn, a stop, a pause, a guest or profile change, and `initiative_activity`
/// does when the user types. The chain's own replies use
/// `reset_live_keeping_relay`, so they never cancel it. Started only while its
/// token is alive and this very turn is the last thing that happened.
fn schedule_relay(app: AppHandle, shared: Shared, voice: Voice, plan: RelayPlan) {
    use sherka_core::session::{ResponseState, TranscriptionState};
    // The first condition that fails is logged (fixed codes, no content).
    let armed = shared.0.lock().map_err(|_| "engine").and_then(|c| {
        let session = c.session.snapshot();
        if plan.token.is_cancelled() {
            Err("cancelled")
        } else if c.live.tag.is_some() {
            Err("live_tag")
        } else if session.response != ResponseState::Idle {
            Err("response")
        } else if session.transcription != TranscriptionState::Idle {
            Err("transcription")
        } else if session.session_id != plan.session_id {
            Err("session")
        } else if session.profile_id != plan.master.id {
            Err("master")
        } else if c.guest() != Some(plan.guest.id) {
            Err("guest")
        } else {
            Ok(c.trio_settings().relay_seconds)
        }
    });
    match armed {
        Ok(seconds) => {
            voice.diagnose(
                &shared,
                "trio_relay_armed",
                Some(&format!("delay_s={seconds}")),
            );
            let token = plan.token.clone();
            tauri::async_runtime::spawn(run_relay(app, shared, voice, token, plan));
        }
        Err(why) => voice.diagnose(
            &shared,
            "trio_relay_ended",
            Some(&format!("not_armed:{why}")),
        ),
    }
}

/// The next reply of the chain, started while the current one talks.
struct Ahead {
    handle: Option<tauri::async_runtime::JoinHandle<Option<Prefetch>>>,
    /// Ends the preparation when dropped, whatever happens to the chain.
    _halt: tokio_util::sync::DropGuard,
}

/// Start preparing the next reaction while the current reply is produced:
/// the signals the speaker sends (text complete, speech synthesized) and the
/// handle the relay keeps. `parent` ends it: the turn's token for a pending
/// prefetch, the relay token for a detached one; dropping the `Ahead` does too.
fn prepare_ahead(
    app: &AppHandle,
    shared: &Shared,
    voice: &Voice,
    parent: &CancellationToken,
    plan: PrefetchPlan,
) -> (FirstSignals, Ahead) {
    let (text_tx, text_rx) = tokio::sync::oneshot::channel();
    let (synth_tx, synth_rx) = tokio::sync::oneshot::channel();
    let halt = parent.child_token();
    let handle = tauri::async_runtime::spawn(start_prefetch(
        app.clone(),
        shared.clone(),
        voice.clone(),
        halt.clone(),
        plan,
        text_rx,
        synth_rx,
    ));
    (
        FirstSignals {
            text: text_tx,
            synthesized: synth_tx,
        },
        Ahead {
            handle: Some(handle),
            _halt: halt.drop_guard(),
        },
    )
}

/// The companions' reactions to the user's silence, one step at a time.
/// `plan.state.next` decides: wait the cooldown (`relay_seconds`, after the
/// answer to the user), then `QUICK_SILENCE` after the first reaction, then
/// react after a short pause (the chain). Each step is the companion who
/// did not speak last reacting to the last line, and may pass. A reply that
/// another follows at once keeps the session turn open (`continues`), so one
/// barge-in stops everything and the next speaker prepares while this one
/// talks (`start_prefetch`, pending); a reply followed by a wait closes it, so
/// the session is idle while the user is expected to speak, and the next
/// speaker prepares detached from it, gated by the relay token alone. The
/// first reaction was prepared during the answer to the user. No user row, no
/// memory.
async fn run_relay(
    app: AppHandle,
    shared: Shared,
    voice: Voice,
    relay: CancellationToken,
    mut plan: RelayPlan,
) {
    use sherka_core::trio::{self, RelayNext};
    // The turn left open by a reply that another follows at once.
    let mut open: Option<OperationTag> = None;
    // The next reply, if it is being prepared: during the answer to the user,
    // or while the previous reply of the relay talks.
    let mut carried: Option<Ahead> = plan.ahead.take();
    // Why the relay ended: a fixed code for `audio-events.log`.
    let reason: &'static str = loop {
        let Some(settings) = shared.0.lock().ok().map(|c| c.trio_settings()) else {
            break "failed";
        };
        let (pause, silence) = match plan.state.next(settings.relay_seconds) {
            RelayNext::Stop => {
                break if settings.relay_seconds == 0 {
                    "off"
                } else if plan.state.replies >= trio::CHAIN_LIMIT {
                    "limit"
                } else {
                    "passes"
                };
            }
            RelayNext::Wait(pause) => (pause, true),
            RelayNext::Chain(pause) => (pause, false),
        };
        if silence {
            let wait = if pause == trio::QUICK_SILENCE {
                "quick_silence"
            } else {
                "cooldown"
            };
            voice.diagnose(&shared, "trio_relay_wait", Some(wait));
        }
        tokio::select! {biased; _=relay.cancelled()=>break "cancelled", _=tokio::time::sleep(pause)=>{}}
        if silence {
            plan.state.silence();
        }
        // Nobody to talk to: the widget is hidden. (Outside any lock: the
        // native getter waits for the UI event loop.)
        if !crate::windows::widget_visible(&app).unwrap_or(false) {
            break "widget_hidden";
        }
        let who = trio::reactor(plan.last.id, &plan.master, &plan.guest).clone();
        let continues = plan.state.chains_after_reply(settings.relay_seconds);
        let follow = trio::follow_message(
            plan.user.as_deref(),
            &settings.user_name,
            &plan.last,
            &plan.last_reply,
        );
        // The reply prepared during the previous one, if it answers this very line.
        let mut ahead = carried.take();
        let prefetch = match ahead.as_mut().and_then(|a| a.handle.take()) {
            Some(handle) => tokio::select! {biased; _=relay.cancelled()=>None, ready=handle=>ready.ok().flatten()},
            None => None,
        }
        .filter(|p| p.follow == follow);
        // Kept alive through this step only when it is promoted.
        let _promoted = prefetch.is_some().then_some(ahead).flatten();
        // Promoted into the open turn, or, after a wait, ready to speak in a
        // fresh turn (the budget is published when the speaker takes over).
        let ahead_tag = prefetch.as_ref().map(|p| (p.tag, p.chat.budget.clone()));
        let detached_budget = ahead_tag
            .as_ref()
            .filter(|_| open.is_none())
            .map(|(_, budget)| budget.clone());
        let begun = {
            let (master, guest, session_id) = (plan.master.id, plan.guest.id, plan.session_id);
            let (token, reactor, after) = (relay.clone(), who.id, open);
            with_core(shared.clone(), move |c| {
                let session = c.session.snapshot();
                // Still the same conversation, and the user is silent.
                if token.is_cancelled() {
                    return Ok(Err("cancelled"));
                }
                if c.guest() != Some(guest)
                    || session.profile_id != master
                    || session.session_id != session_id
                {
                    return Ok(Err("stale"));
                }
                if c.session.initiative_paused() {
                    return Ok(Err("paused"));
                }
                let tag = match after {
                    Some(after) => hand_over(c, after, reactor, ahead_tag, true)?,
                    None => {
                        use sherka_core::session::{ResponseState, TranscriptionState};
                        // Nothing playing, captured or running.
                        if session.response != ResponseState::Idle
                            || session.transcription != TranscriptionState::Idle
                            || c.live.tag.is_some()
                        {
                            return Ok(Err("busy"));
                        }
                        let turn = c.session.begin_text_turn();
                        let tag = c.session.begin_peer(turn, reactor)?;
                        c.reset_live_keeping_relay(tag);
                        if detached_budget.is_some() {
                            c.live.budget = detached_budget;
                        }
                        tag
                    }
                };
                Ok(Ok((tag, c.session.cancellation())))
            })
            .await
        };
        let (tag, token) = match begun {
            Ok(Ok(started)) => started,
            Ok(Err(why)) => break why,
            // The turn the chain was holding open is gone: the user cut in.
            Err(_) => break "stale",
        };
        emit(&app, &shared);
        let provider = match &prefetch {
            Some(ahead) => Ok(ahead.provider.clone()),
            None => speaker_provider(&shared, who.id).await,
        };
        let provider = match provider {
            Ok(provider) => provider,
            Err(error) => {
                fail_turn(&app, &shared, tag, &error).await;
                open = None;
                break "failed";
            }
        };
        // The speaker who follows prepares while this one talks: in the open
        // turn when it follows at once, otherwise (a wait, so the session turn
        // closes and the user may speak) detached, gated by the relay token.
        let waits = !continues && plan.state.waits_after_reply(settings.relay_seconds);
        let (signals, next) = match (continues, waits) {
            (false, false) => (None, None),
            (continues, _) => {
                let (gate, parent) = if continues {
                    (Gate::Pending, &token)
                } else {
                    (Gate::Detached, &relay)
                };
                let (ready, next) = prepare_ahead(
                    &app,
                    &shared,
                    &voice,
                    parent,
                    PrefetchPlan {
                        gate,
                        first_tag: tag,
                        first: who.clone(),
                        second: plan.last.id,
                        user: None,
                        user_name: settings.user_name.clone(),
                        quick: settings.quick_replies,
                        spontaneous: false,
                        speak: plan.speak,
                    },
                );
                (Some(ready), Some(next))
            }
        };
        let priority = app
            .state::<crate::illustrations::Illustrations>()
            .priority(app.clone(), shared.clone());
        let user_turn = if plan.user.is_some() {
            plan.user_turn
        } else {
            tag.turn_id
        };
        let kind = if plan.state.replies == 0 {
            "first"
        } else if silence {
            "silence"
        } else {
            "chained"
        };
        voice.diagnose(&shared, "trio_relay_fired", Some(kind));
        let end = run_turn(
            app.clone(),
            shared.clone(),
            voice.clone(),
            tag,
            token,
            provider,
            plan.user.clone().unwrap_or_default(),
            plan.speak,
            priority,
            None,
            Some(TrioStep::Second {
                peer: trio::Peer {
                    name: plan.last.name.clone(),
                    second: true,
                    quick: settings.quick_replies,
                },
                follow,
                first_generation: plan.last_generation,
                user_turn,
                prefetch: prefetch.map(Box::new),
                signals,
                continues,
            }),
        )
        .await;
        for id in [plan.master.id, plan.guest.id] {
            let _ = app.emit("sherka:history", id);
        }
        match end {
            TurnEnd::Done { reply, .. } => {
                plan.state.replied();
                plan.user = None;
                plan.last = who;
                plan.last_reply = reply;
                plan.last_generation = tag.generation_id;
                open = continues.then_some(tag);
                carried = next;
            }
            TurnEnd::Passed => {
                // `discard_generation` ended the turn.
                plan.state.passed();
                open = None;
            }
            TurnEnd::Stopped => {
                open = None;
                // A provider or turn error is kept for the panel; a stop is not.
                let failed = shared.0.lock().is_ok_and(|c| c.live.error.is_some());
                break if failed { "failed" } else { "cancelled" };
            }
        }
    };
    // Back to the user: never leave the turn of a chain half open.
    if let Some(tag) = open {
        let _ = with_core(shared.clone(), move |c| c.session.complete(tag)).await;
    }
    emit(&app, &shared);
    voice.diagnose(&shared, "trio_relay_ended", Some(reason));
}

#[allow(clippy::too_many_arguments)]
async fn run_turn(
    app: AppHandle,
    shared: Shared,
    voice: Voice,
    tag: OperationTag,
    token: CancellationToken,
    mut provider: Providers,
    user: String,
    speak: bool,
    priority: crate::illustrations::Priority,
    event: Option<AppEvent>,
    mut trio: Option<TrioStep>,
) -> TurnEnd {
    let spontaneous = event.is_some();
    // Signals to, or the reply prefetched by, the other speaker of a trio turn.
    // A sender dropped unsent (any early return) tells the prefetch to give up.
    let (mut text_signal, mut synth_signal, mut prefetch) = match &mut trio {
        Some(TrioStep::First { signals, .. }) => match signals.take() {
            Some(FirstSignals { text, synthesized }) => (Some(text), Some(synthesized), None),
            None => (None, None, None),
        },
        Some(TrioStep::Second {
            prefetch, signals, ..
        }) => match signals.take() {
            Some(FirstSignals { text, synthesized }) => {
                (Some(text), Some(synthesized), prefetch.take())
            }
            None => (None, None, prefetch.take()),
        },
        None => (None, None, None),
    };
    let speech_end = voice.take_speech_end(tag);
    let clock = crate::turn_timing::TurnClock::new(speech_end);
    if speech_end.is_some() {
        clock.mark(crate::turn_timing::Mark::Transcribed);
    }
    tokio::select! {biased;_=token.cancelled()=>return TurnEnd::Stopped,_=priority.ready()=>{}}
    let agent = provider.connections.llm_provider.is_agent();
    let solo = trio.is_none();
    // In a three-way turn the orchestrator already handled the master's commands.
    if !agent
        && !spontaneous
        && solo
        && handle_maintenance(&app, &shared, &voice, tag, &user, speak).await
    {
        return TurnEnd::Stopped;
    }
    // A new utterance may arrive while the old run acknowledges cancellation.
    // Serialize remote runs and keep failed stops retryable instead of silently
    // starting overlapping agent work on the same server conversation.
    let _remote_gate = if agent {
        Some(
            tokio::select! { biased; _=token.cancelled()=>return TurnEnd::Stopped, guard=voice.hermes_gate.lock()=>guard },
        )
    } else {
        None
    };
    if agent && voice.hermes_run.lock().is_ok_and(|run| run.is_some()) {
        fail_turn(
            &app,
            &shared,
            tag,
            "L’arrêt Hermes n’est pas confirmé. Réessaie /stop avant de poursuivre.",
        )
        .await;
        return TurnEnd::Stopped;
    }
    let hermes = sherka_core::hermes::HermesRun::new(provider.clone());
    let mut remote_session = None;
    let preparation = async {
        if agent {
            voice
                .models
                .activate(&provider.connections, None, || !token.is_cancelled())
                .await
                .map_err(|e| e.to_string())?;
            let endpoint = provider.connections.api_base().map_err(|e| e.to_string())?;
            remote_session = Some(
                with_core(shared.clone(), move |c| {
                    if !c.session.accepts(tag) {
                        return Err(Error::StaleOperation);
                    }
                    let session = c.store.hermes_session(tag.profile_id, &endpoint)?;
                    let tip = c.store.hermes_session_tip(&session)?;
                    Ok((session, tip))
                })
                .await?,
            );
            Ok(None)
        } else if let Some(ahead) = &prefetch {
            // Prepared and already generating; the budget is published by the
            // orchestrator when this speaker was promoted.
            Ok(Some(ahead.chat.clone()))
        } else if let Some(TrioStep::Second {
            peer,
            follow,
            first_generation,
            user_turn,
            ..
        }) = &trio
        {
            prepare_peer(
                &app,
                &shared,
                &voice,
                tag,
                Gate::Current,
                &mut provider,
                follow,
                peer,
                *first_generation,
                *user_turn,
                spontaneous,
            )
            .await
            .map(Some)
        } else {
            prepare_turn(
                &app,
                &shared,
                &voice,
                tag,
                &mut provider,
                &user,
                event.as_ref(),
                trio.as_ref().map(TrioStep::peer),
            )
            .await
            .map(Some)
        }
    };
    let prepared = tokio::select! {
        biased;
        _=token.cancelled()=>return TurnEnd::Stopped,
        result=preparation=>result
    };
    let prepared = match prepared {
        Ok(value) => value,
        Err(e) => {
            fail_turn(&app, &shared, tag, &e).await;
            return TurnEnd::Stopped;
        }
    };
    clock.mark(crate::turn_timing::Mark::Prepared);
    if voice.playback.arm(token.clone()).is_err() {
        return TurnEnd::Stopped;
    }
    if agent && let Ok(mut active) = voice.hermes_run.lock() {
        *active = Some((tag, hermes.clone()));
    }
    let (tokens_tx, mut tokens_rx) = mpsc::channel::<String>(64);
    // A prefetched reply replays the tokens received so far, then the rest.
    let (mut pregenerated, mut cache) = (None, None);
    if let Some(ahead) = prefetch.take() {
        tokens_rx = ahead.tokens;
        pregenerated = Some(ahead.generation);
        cache = ahead.cache;
    }
    let web_reply = prepared
        .as_ref()
        .and_then(|p| p.web.as_ref())
        .is_some_and(|report| !report.sources.is_empty());
    let speech_text = SpeechText::shared();
    let (text_done_tx, text_done_rx) = tokio::sync::oneshot::channel::<()>();
    let (audio_tx, audio_rx) = mpsc::channel::<(String, Vec<i16>)>(1);
    let fixed_reply = prepared
        .as_ref()
        .and_then(|p| p.web.as_ref())
        .and_then(|report| report.unavailable_reply(&provider.connections.language));
    let unavailable = fixed_reply.is_some();
    let expressions =
        !agent && !unavailable && prepared.as_ref().is_some_and(|p| p.avatar_emotions);
    let generate = async {
        if let Some(text) = fixed_reply {
            tokens_tx
                .send(text)
                .await
                .map_err(|_| Error::StaleOperation)?;
            Ok(sherka_core::providers::ChatOutcome::default())
        } else if let Some((session, tip)) = &remote_session {
            hermes
                .chat(tip, session, &user, tokens_tx, |event| {
                    let needs_approval =
                        matches!(&event, sherka_core::hermes::AgentEvent::Approval(Some(_)));
                    let result = update(&app, &shared, tag, |c| {
                        match event {
                            sherka_core::hermes::AgentEvent::Session(tip) => {
                                c.store.update_hermes_session_tip(session, &tip)?
                            }
                            sherka_core::hermes::AgentEvent::Notice(text) => {
                                c.live.notice = Some(text.into())
                            }
                            sherka_core::hermes::AgentEvent::Approval(value) => {
                                c.live.hermes_approval = value
                            }
                        }
                        Ok(())
                    });
                    if result.is_ok() && needs_approval {
                        let _ = crate::windows::show_panel(&app);
                    }
                    result
                })
                .await
        } else if let Some(generation) = pregenerated.take() {
            generation
                .await
                .map_err(|_| Error::StaleOperation)
                .and_then(|outcome| outcome)
        } else {
            let prepared = prepared.ok_or(Error::StaleOperation)?;
            provider
                .chat_budgeted(
                    prepared.messages,
                    tokens_tx,
                    prepared.budget.response_tokens,
                )
                .await
        }
    };
    let second = matches!(trio, Some(TrioStep::Second { .. }));
    let consume = async {
        let mut web_speech = sherka_core::web::SpeechFilter::default();
        let mut paint = LivePaint::default();
        // A second speaker may answer `[[pass]]`: nothing is shown or spoken,
        // but the provider's stream is still drained to its end.
        let mut filter = ReplyFilter::new(expressions, second);
        let mut append =
            |text: String, emotion, paint: &mut LivePaint| -> sherka_core::Result<()> {
                quiet_update(&shared, tag, |c| {
                    c.live.emotion = emotion;
                    c.live.reply.push_str(&text);
                    Ok(())
                })?;
                paint.text(&app, &shared, emotion);
                if speak {
                    let text = if web_reply {
                        web_speech.push(&text)
                    } else {
                        text
                    };
                    if !text.is_empty() {
                        speech_text.push(&text)?;
                    }
                }
                Ok(())
            };
        loop {
            // Held-back text is shown even if the provider then pauses.
            let next = match paint.held() {
                Some((at, shown)) => tokio::select! {
                    text = tokens_rx.recv() => text,
                    _ = tokio::time::sleep(LIVE_TEXT_INTERVAL.saturating_sub(at.elapsed())) => {
                        paint.show(&app, &shared, shown);
                        continue;
                    }
                },
                None => tokens_rx.recv().await,
            };
            let Some(text) = next else { break };
            clock.mark(crate::turn_timing::Mark::FirstToken);
            if let Some(text) = filter.push(&text) {
                append(text, filter.emotion(), &mut paint)?;
            }
        }
        for text in filter.finish() {
            append(text, filter.emotion(), &mut paint)?;
        }
        if speak && web_reply {
            speech_text.push(&web_speech.finish())?;
        }
        // The last coalesced tokens.
        emit(&app, &shared);
        if expressions && !second {
            update(&app, &shared, tag, |c| {
                if c.live.reply.trim().is_empty() {
                    return Err(Error::Validation(
                        "Le fournisseur a renvoyé une réponse vide.".into(),
                    ));
                }
                Ok(())
            })?;
        }
        Ok::<_, Error>(filter.passed())
    };
    // Closing on errors matters too: never leave TTS waiting on a failed LLM.
    let consume = async {
        let result = consume.await;
        speech_text.finish();
        result
    };
    let prepare = async {
        let result = async {
            if speak {
                while let Some(phrase) = speech_text.next().await? {
                    clock.mark(crate::turn_timing::Mark::FirstPassage);
                    // The first passages of a prefetched reply are already synthesized.
                    let pcm = match PcmCache::take(&mut cache, &phrase).await {
                        Some(pcm) => pcm,
                        None => provider.speech(&phrase).await?,
                    };
                    audio_tx
                        .send((phrase, pcm))
                        .await
                        .map_err(|_| Error::StaleOperation)?;
                }
            }
            Ok::<_, Error>(())
        }
        .await;
        // Every passage is synthesized: VoiceStudio is free for the next speaker.
        if result.is_ok()
            && let Some(free) = synth_signal.take()
        {
            let _ = free.send(());
        }
        drop(audio_tx);
        let _ = text_done_rx.await;
        // TTS synthesis has finished; buffered PCM can play without holding the
        // GPU. Resume live images only after text and synthesis are both done.
        drop(priority);
        result
    };
    let play = play_prepared(
        &app,
        &shared,
        &voice.playback,
        tag,
        &token,
        provider.connections.volume,
        audio_rx,
        &clock,
    );
    let say = async {
        let (prepared, played) = tokio::join!(prepare, play);
        let result = prepared.and(played);
        if let Err(error) = &result {
            voice.diagnose(&shared, "speech_output_failed", None);
            let message = error.to_string();
            let _ = update(&app, &shared, tag, |c| {
                c.live.error = Some(format!("{message} La réponse écrite reste disponible."));
                Ok(())
            });
        }
        result
    };
    let result = tokio::select! {
        biased;
        _=token.cancelled()=>None,
        result=async {
            let text=async {
                let result=tokio::join!(generate,consume);
                if result.0.is_ok() && result.1.is_ok() {
                    clock.mark(crate::turn_timing::Mark::TextComplete);
                    let mut visible=String::new();
                    let mut image_receipt=None;
                    let complete=update(&app,&shared,tag,|c|{
                        c.live.text_complete=true;
                        visible=c.live.reply.clone();
                        image_receipt=c.live.image_receipt;
                        if !agent && !unavailable { c.complete_mood(tag, &user)?; }
                        if !agent && !unavailable && solo && c.profile(tag.profile_id)?.details.illustration.enabled {
                            c.store.save_response(tag,&c.live.reply,&c.live.heard,if speak {Delivery::Uncertain}else{Delivery::TextOnly})?;
                        }
                        Ok(())
                    });
                    // The language model is free: the second speaker may start.
                    if complete.is_ok() && !visible.trim().is_empty() && let Some(done)=text_signal.take() {
                        let _=done.send(visible);
                    }
                    if complete.is_ok() && !agent && !unavailable && solo && crate::image_control::illustrate_after_reply(image_receipt) {
                        app.state::<crate::illustrations::Illustrations>().request(app.clone(),shared.clone());
                    }
                }
                let _=text_done_tx.send(());
                result
            };
            let ((generated,consumed),spoken)=tokio::join!(text,say);
            (generated,consumed,spoken)
        }=>Some(result)
    };
    let end = match result {
        None => {
            voice.diagnose(&shared, "response_cancelled", None);
            if agent {
                finish_hermes_run(&app, &shared, &voice, tag, &hermes, true).await;
            }
            // The local audio worker can cancel before the desktop processes
            // Start. Persist only while this exact turn is still the live one.
            let _ = with_core(shared.clone(), move |c| {
                if c.live.tag == Some(tag) {
                    c.stop_response()?;
                }
                Ok(())
            })
            .await;
            TurnEnd::Stopped
        }
        Some((Ok(outcome), Ok(passed), spoken)) => {
            voice.diagnose(
                &shared,
                if spoken.is_ok() {
                    "response_completed"
                } else {
                    "response_text_only_after_audio_error"
                },
                None,
            );
            if agent {
                finish_hermes_run(&app, &shared, &voice, tag, &hermes, false).await;
            }
            if passed {
                // The second speaker had nothing to add: nothing is saved.
                let _ = with_core(shared.clone(), move |c| c.discard_generation(tag)).await;
                TurnEnd::Passed
            } else {
                let delivery = if !speak || provider.connections.volume <= 0.0 {
                    Delivery::TextOnly
                } else if spoken.is_ok() {
                    Delivery::Completed
                } else {
                    Delivery::Uncertain
                };
                // A speaker who is followed at once leaves the turn open.
                let continues = matches!(
                    trio,
                    Some(
                        TrioStep::First {
                            continues: true,
                            ..
                        } | TrioStep::Second {
                            continues: true,
                            ..
                        }
                    )
                );
                let saved = with_core(shared.clone(), move |c| {
                    if !c.session.accepts(tag) {return Err(Error::StaleOperation);}
                    if outcome.length_limited {c.live.notice=Some("La réponse a atteint la réserve de tokens. Tu peux demander la suite ou augmenter cette réserve dans les réglages.".into());}
                    c.live.usage=if unavailable {None} else {Some(outcome)};
                    let reply = c.live.reply.clone();
                    let saved = if continues {c.finish_generation(tag, delivery)} else {c.finish_response(tag, delivery)};
                    if !solo && saved.is_err() {
                        // Never leave a three-way turn half open.
                        let _ = c.stop_response();
                    }
                    saved.map(|()| reply)
                }).await;
                let memory = !unavailable && !agent;
                if memory && solo && !spontaneous {
                    schedule_memory(
                        app.clone(),
                        shared.clone(),
                        vec![tag.profile_id],
                        provider.clone(),
                    );
                }
                match saved {
                    Ok(reply) => TurnEnd::Done {
                        reply,
                        provider: Box::new(provider),
                        memory,
                    },
                    Err(_) => TurnEnd::Stopped,
                }
            }
        }
        Some((llm, consumed, _)) => {
            voice.diagnose(&shared, "response_generation_failed", None);
            if agent {
                finish_hermes_run(&app, &shared, &voice, tag, &hermes, true).await;
            }
            let error = llm
                .err()
                .or_else(|| consumed.err())
                .map_or_else(|| "La réponse a été interrompue.".into(), |e| e.to_string());
            fail_turn(&app, &shared, tag, &error).await;
            TurnEnd::Stopped
        }
    };
    let reply = shared
        .0
        .lock()
        .map(|c| c.live.reply.chars().count())
        .unwrap_or_default();
    voice.diagnostics.record_timing(reply, &clock.summary());
    emit(&app, &shared);
    let _ = app.emit("sherka:history", tag.profile_id);
    end
}

async fn finish_hermes_run(
    app: &AppHandle,
    shared: &Shared,
    voice: &Voice,
    tag: OperationTag,
    run: &sherka_core::hermes::HermesRun,
    stop: bool,
) {
    if stop && let Err(e) = run.stop().await {
        let message = format!(
            "{} L’arrêt Hermes n’est pas confirmé. Réessaie /stop avant de poursuivre.",
            e
        );
        if let Ok(mut c) = shared.0.lock()
            && c.session.snapshot().profile_id == tag.profile_id
        {
            c.live.error = Some(message);
        }
        emit(app, shared);
        return;
    }
    if let Ok(mut active) = voice.hermes_run.lock()
        && active.as_ref().is_some_and(|(current, _)| *current == tag)
    {
        *active = None;
    }
}

#[allow(clippy::too_many_arguments)]
async fn play_prepared(
    app: &AppHandle,
    shared: &Shared,
    playback: &Playback,
    tag: OperationTag,
    token: &CancellationToken,
    volume: f32,
    mut audio: mpsc::Receiver<(String, Vec<i16>)>,
    clock: &crate::turn_timing::TurnClock,
) -> Result<()> {
    use futures_util::{StreamExt, stream::FuturesOrdered};
    // Next audio is queued before the current receipt completes. Dropping the
    // owned receiver on errors also releases a producer waiting on capacity.
    let mut receipts = FuturesOrdered::new();
    let mut closed = false;
    loop {
        tokio::select! {
            result = receipts.next(), if !receipts.is_empty() => {
                let phrase: String = result.unwrap()?;
                update(app, shared, tag, |c| {
                    if volume > 0.0 {
                        if !c.live.heard.is_empty() { c.live.heard.push(' '); }
                        c.live.heard.push_str(&phrase);
                    }
                    Ok(())
                })?;
            }
            next = audio.recv(), if !closed && receipts.len() < 2 => {
                let Some((phrase, pcm)) = next else { closed = true; continue; };
                quiet_update(shared, tag, |_| Ok(()))?;
                let receipt = playback.enqueue(pcm, volume, token.clone())?;
                receipts.push_back(async move {
                    receipt.started.await.map_err(|_| Error::StaleOperation)?;
                    clock.mark(crate::turn_timing::Mark::FirstAudio);
                    update(app, shared, tag, |c| c.session.playback_started(tag))?;
                    receipt.done.await.map_err(|_| Error::StaleOperation)?;
                    Ok::<_, Error>(phrase)
                });
            }
            else => break,
        }
    }
    Ok(())
}

/// Whether the master may react to `event` now. An event remark does not need
/// the initiative to be enabled, but the guest it greets must still be there.
fn check_event(
    core: &sherka_core::Companion,
    profile: &sherka_core::model::Profile,
    event: &AppEvent,
) -> sherka_core::Result<()> {
    match event {
        AppEvent::Idle => check_spontaneous(core, profile),
        // The Image button was just pressed: like a guest arrival, the remark
        // does not wait for the microphone, but needs a free conversation.
        AppEvent::ImagesOn { .. } => {
            let session = core.session.snapshot();
            if profile.details.connections.llm_provider.is_agent()
                || session.response != sherka_core::session::ResponseState::Idle
                || session.transcription != sherka_core::session::TranscriptionState::Idle
            {
                return Err(Error::StaleOperation);
            }
            Ok(())
        }
        // The user just invited the guest: a remark is welcome even before the
        // microphone was ever enabled, so `voice_paused` does not gate it.
        AppEvent::GuestJoined { id, .. } => {
            let session = core.session.snapshot();
            if profile.details.connections.llm_provider.is_agent()
                || core.guest() != Some(*id)
                || session.response != sherka_core::session::ResponseState::Idle
                || session.transcription != sherka_core::session::TranscriptionState::Idle
            {
                return Err(Error::StaleOperation);
            }
            Ok(())
        }
    }
}

fn check_spontaneous(
    core: &sherka_core::Companion,
    profile: &sherka_core::model::Profile,
) -> sherka_core::Result<()> {
    let session = core.session.snapshot();
    if !profile.details.initiative.enabled
        || profile.details.connections.llm_provider.is_agent()
        || core.session.initiative_paused()
        || session.response != sherka_core::session::ResponseState::Idle
        || session.transcription != sherka_core::session::TranscriptionState::Idle
    {
        return Err(Error::StaleOperation);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_transcription_keeps_microphone_consent_and_rejects_late_results() {
        let dir = tempfile::tempdir().unwrap();
        let mut core = sherka_core::Companion::open(dir.path()).unwrap();
        core.session.capture_started();
        let interrupted = core.session.begin_utterance().unwrap();
        core.reset_live(interrupted);
        let cancellation = core.session.cancellation();
        record_turn_failure(&mut core, interrupted, "fixture failure".into()).unwrap();
        assert!(cancellation.is_cancelled());
        assert_eq!(
            core.session.snapshot().microphone,
            MicrophoneState::Listening
        );
        assert!(!core.session.snapshot().voice_paused);
        assert!(
            core.session
                .accept_stt_final(interrupted, "discarded partial command")
                .is_err()
        );
        let next = core.session.begin_utterance().unwrap();
        core.reset_live(next);
        assert!(record_turn_failure(&mut core, interrupted, "late error".into()).is_err());
        assert!(core.session.accepts(next));
        assert_eq!(
            core.session
                .accept_stt_final(next, "fresh full command")
                .unwrap()
                .as_deref(),
            Some("fresh full command")
        );
        core.finish_response(next, Delivery::Completed).unwrap();
        assert_eq!(
            core.session.snapshot().microphone,
            MicrophoneState::Listening
        );
        core.pause_voice().unwrap();
        assert_eq!(core.session.snapshot().microphone, MicrophoneState::Off);
        assert!(record_turn_failure(&mut core, interrupted, "late error".into()).is_err());
        assert_eq!(core.session.snapshot().microphone, MicrophoneState::Off);
    }

    #[test]
    fn a_chained_speaker_takes_over_without_cancelling_the_relay_that_runs_it() {
        let dir = tempfile::tempdir().unwrap();
        let mut core = sherka_core::Companion::open(dir.path()).unwrap();
        let turn = core.session.begin_text_turn();
        core.reset_live(turn);
        let relay = core.arm_relay();
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let first = hand_over(&mut core, turn, a, None, true).unwrap();
        assert!(!relay.is_cancelled());
        assert_eq!((first.profile_id, core.live.tag), (a, Some(first)));
        assert!(core.session.accepts(first) && !core.session.accepts(turn));
        // The previous speaker is final once the next one has taken over.
        let second = hand_over(&mut core, first, b, None, true).unwrap();
        assert!(!relay.is_cancelled() && core.session.accepts(second));
        assert!(hand_over(&mut core, first, a, None, true).is_err());
        // A speaker that is not the relay's ends it, but keeps the memory job.
        let job = core.memory_job.clone();
        hand_over(&mut core, second, a, None, false).unwrap();
        assert!(relay.is_cancelled() && !job.is_cancelled());
    }

    #[test]
    fn an_extraction_waits_while_the_companions_talk_and_ignores_strangers() {
        let dir = tempfile::tempdir().unwrap();
        let mut core = sherka_core::Companion::open(dir.path()).unwrap();
        let master = core.session.snapshot().profile_id;
        assert_eq!(memory_turn(&core, master), MemoryTurn::Go);
        assert_eq!(memory_turn(&core, Uuid::new_v4()), MemoryTurn::Stale);
        // A relay or chained reply is running: wait for it instead of giving up.
        let turn = core.session.begin_text_turn();
        core.reset_live_keeping_relay(turn);
        assert_eq!(memory_turn(&core, master), MemoryTurn::Wait);
        core.stop_response().unwrap();
        assert_eq!(memory_turn(&core, master), MemoryTurn::Go);
    }

    fn who(name: &str) -> sherka_core::trio::Participant {
        sherka_core::trio::Participant {
            id: Uuid::new_v4(),
            name: name.into(),
        }
    }

    #[test]
    fn a_spontaneous_trio_opens_with_the_coin_and_one_companion_answers_the_user() {
        let (master, guest) = (who("Sherka"), who("Kit"));
        assert_eq!(
            speaking_order(None, &master, &guest, true, false),
            vec![master.id, guest.id]
        );
        assert_eq!(
            speaking_order(None, &master, &guest, false, false),
            vec![guest.id, master.id]
        );
        // The user is answered by one companion: the addressee, else the coin.
        // The other one waits for the cooldown (the relay).
        assert_eq!(
            speaking_order(Some("Kit, tu dors ?"), &master, &guest, true, false),
            vec![guest.id]
        );
        assert_eq!(
            speaking_order(Some("Salut vous deux"), &master, &guest, false, false),
            vec![guest.id]
        );
        assert_eq!(
            speaking_order(Some("Salut vous deux"), &master, &guest, true, false),
            vec![master.id]
        );
    }

    #[test]
    fn a_detached_preparation_lives_and_dies_with_the_relay_token() {
        let dir = tempfile::tempdir().unwrap();
        let mut core = sherka_core::Companion::open(dir.path()).unwrap();
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let turn = core.session.begin_text_turn();
        core.reset_live(turn);
        let answer = core.session.begin_peer(turn, a).unwrap();
        core.reset_live(answer);
        let relay = core.arm_relay();
        let prepared = detached_tag(answer, b);
        // Same session and turn as the answer it reacts to, a generation of its own.
        assert_eq!(
            (prepared.profile_id, prepared.session_id, prepared.turn_id),
            (b, answer.session_id, answer.turn_id)
        );
        assert_ne!(prepared.generation_id, answer.generation_id);
        // The session never accepts it: it only answers to the relay token.
        assert!(!core.session.accepts(prepared) && !core.session.accepts_pending(prepared));
        assert!(Gate::Detached.accepts(&core, prepared));
        assert!(!Gate::Pending.accepts(&core, prepared) && !Gate::Current.accepts(&core, prepared));
        // The answer finishes: the session turn closes so the user can speak,
        // and the prepared reaction is still wanted.
        core.session.complete(answer).unwrap();
        assert_eq!(
            core.session.snapshot().response,
            sherka_core::session::ResponseState::Idle
        );
        assert!(!relay.is_cancelled() && Gate::Detached.accepts(&core, prepared));
        // Typing, speaking or any new turn of the user cancels it at once.
        let user = core.session.begin_text_turn();
        core.reset_live(user);
        assert!(relay.is_cancelled() && !Gate::Detached.accepts(&core, prepared));
        // A child token, as `prepare_ahead` hands out, follows its parent.
        let relay = core.arm_relay();
        let halt = relay.child_token();
        assert!(!halt.is_cancelled());
        core.cancel_relay();
        assert!(halt.is_cancelled());
    }

    #[test]
    fn an_event_remark_in_a_trio_is_the_masters_alone() {
        let (master, guest) = (who("Sherka"), who("Kit"));
        for coin in [true, false] {
            assert_eq!(
                speaking_order(None, &master, &guest, coin, true),
                vec![master.id]
            );
        }
        let joined = AppEvent::GuestJoined {
            id: guest.id,
            name: guest.name,
            description: None,
        };
        assert!(joined.master_only());
        assert!(!AppEvent::Idle.master_only());
        // The silence relay follows a user turn and the arrival remark only.
        assert!(relay_follows(Some("Salut"), None));
        assert!(relay_follows(None, Some(&joined)));
        assert!(!relay_follows(None, Some(&AppEvent::Idle)));
        assert!(!relay_follows(None, None));
    }

    #[test]
    fn the_second_speaker_does_not_see_the_turn_twice() {
        let generation = Uuid::new_v4();
        assert_eq!(
            peer_exclusions(Some("user-row".into()), generation),
            vec!["user-row".to_string(), generation.to_string()]
        );
        // A spontaneous turn has no user row.
        assert_eq!(
            peer_exclusions(None, generation),
            vec![generation.to_string()]
        );
    }

    #[test]
    fn only_a_second_speaker_can_pass_and_an_empty_reply_is_a_pass() {
        // Solo and first speakers: text flows untouched, nothing is held back.
        let mut solo = PassGate::new(false);
        assert_eq!(solo.push("[[pass]]".into()).as_deref(), Some("[[pass]]"));
        assert_eq!(solo.finish(), None);
        assert!(!solo.passed);

        let mut speaker = PassGate::new(true);
        assert_eq!(speaker.push("[".into()), None);
        assert_eq!(speaker.push("Bonjour".into()).as_deref(), Some("[Bonjour"));
        assert_eq!(speaker.push(" toi".into()).as_deref(), Some(" toi"));
        assert_eq!(speaker.finish(), None);
        assert!(!speaker.passed);

        let mut silent = PassGate::new(true);
        assert_eq!(silent.push("[[pa".into()), None);
        assert_eq!(silent.push("ss]]".into()), None);
        assert_eq!(silent.push(" ignored".into()), None);
        assert_eq!(silent.finish(), None);
        assert!(silent.passed);

        let mut empty = PassGate::new(true);
        assert_eq!(empty.push(String::new()), None);
        assert_eq!(empty.finish(), None);
        assert!(empty.passed);
    }

    /// The visible text of a whole reply, chunk by chunk, as the listener gets it.
    fn visible(filter: &mut ReplyFilter, chunks: &[&str]) -> String {
        let mut text = String::new();
        for chunk in chunks {
            text.extend(filter.push(chunk));
        }
        text.extend(filter.finish());
        text
    }

    #[test]
    fn the_reply_filter_hides_the_header_the_pass_marker_and_emoji() {
        use sherka_core::avatar::Emotion;
        let mut first = ReplyFilter::new(true, false);
        let text = visible(
            &mut first,
            &["[[emo", "tion: joy]]\nBon", "jour 😊 toi", " !"],
        );
        assert_eq!(
            text,
            "
Bonjour  toi !"
        );
        assert_eq!(first.emotion(), Emotion::Joy);
        assert!(!first.passed());
        // The same text however it is cut, and without a header when the avatar has none.
        let mut once = ReplyFilter::new(true, true);
        assert_eq!(
            visible(&mut once, &["[[emotion: joy]]\nBonjour 😊 toi !"]),
            text
        );
        let mut plain = ReplyFilter::new(false, false);
        assert_eq!(
            visible(&mut plain, &["[[emotion: joy]]\nSalut"]),
            "[[emotion: joy]]\nSalut"
        );
        assert_eq!(plain.emotion(), Emotion::Neutral);
        // A second speaker passing says nothing, whatever the header.
        let mut pass = ReplyFilter::new(true, true);
        assert_eq!(
            visible(&mut pass, &["[[emotion: calm]]\n[[pa", "ss]] ok"]),
            ""
        );
        assert!(pass.passed());
        let mut early = ReplyFilter::new(false, true);
        assert_eq!(visible(&mut early, &["[[pass]]"]), "");
        assert!(early.passed());
        // A first speaker is never held back: the marker is plain text for it.
        let mut solo = ReplyFilter::new(false, false);
        assert_eq!(visible(&mut solo, &["[[pass]]"]), "[[pass]]");
        assert!(!solo.passed());
        // An empty reply is silence for a second speaker.
        let mut empty = ReplyFilter::new(true, true);
        assert_eq!(visible(&mut empty, &[]), "");
        assert!(empty.passed());
        let mut held = ReplyFilter::new(false, true);
        assert_eq!(visible(&mut held, &["[", "Oui"]), "[Oui");
        assert!(!held.passed());
    }

    #[test]
    fn prepared_passages_serve_matching_phrases_only() {
        tauri::async_runtime::block_on(async {
            let passages = |items: &[(&str, i16)]| {
                let (tx, rx) = mpsc::channel::<Passage>(4);
                for (phrase, sample) in items {
                    tx.try_send(((*phrase).to_owned(), vec![*sample])).unwrap();
                }
                Some(PcmCache {
                    passages: rx,
                    _halt: None,
                })
            };
            // Hits, in order; then the cache is exhausted and dropped.
            let mut cache = passages(&[("Un.", 1), ("Deux.", 2)]);
            assert_eq!(PcmCache::take(&mut cache, "Un.").await, Some(vec![1]));
            assert!(cache.is_some());
            assert_eq!(PcmCache::take(&mut cache, "Deux.").await, Some(vec![2]));
            assert_eq!(PcmCache::take(&mut cache, "Trois.").await, None);
            assert!(cache.is_none());
            // A different phrase drops the rest: nothing stale can play later.
            let mut cache = passages(&[("Un.", 1), ("Deux.", 2)]);
            assert_eq!(PcmCache::take(&mut cache, "Autre.").await, None);
            assert!(cache.is_none());
            assert_eq!(PcmCache::take(&mut cache, "Deux.").await, None);
            // No cache at all, or an empty one.
            assert_eq!(PcmCache::take(&mut None, "Un.").await, None);
            let mut empty = passages(&[]);
            assert_eq!(PcmCache::take(&mut empty, "Un.").await, None);
            assert!(empty.is_none());
        });
    }
}
