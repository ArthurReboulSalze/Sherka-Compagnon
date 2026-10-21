use crate::{Shared, with_core};
use serde::Serialize;
use sherka_core::{
    Error, Result,
    audio::{AudioController, AudioEvent, Playback},
    conversation::{LiveConversation, SpeechText},
    providers::Providers,
    session::{MicrophoneState, OperationTag, SessionSnapshot},
    storage::{Delivery, MessageRole},
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
    diagnostics: Arc<crate::audio_diagnostics::AudioDiagnostics>,
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

pub async fn toggle(
    app: AppHandle,
    shared: Shared,
    voice: Voice,
) -> std::result::Result<(), String> {
    crate::windows::ensure_running()?;
    let gate = voice.gate.lock().await;
    crate::windows::ensure_running()?;
    if live(&shared)?.session.microphone == MicrophoneState::Listening {
        return pause(&app, shared, &voice, true).await;
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
        return pause(&app, shared, &voice, true).await;
    }
    // Read-only preflight before opening the microphone. No model downloads.
    let catalog = sherka_core::catalogs::CatalogClient::new(&connections.voice_studio_url)
        .map_err(|e| e.to_string())?
        .voices()
        .await
        .map_err(|e| e.to_string())?;
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
        false,
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
        "Idle invitation".into(),
        true,
        vec![],
        true,
    )
    .await
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
    spontaneous: bool,
) -> std::result::Result<(), String> {
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
    let (connections, maintenance) = with_core(shared.clone(), move |core| {
        if core.session.snapshot().profile_id != profile_id {
            return Err(Error::StaleOperation);
        }
        let profile = core.profile(profile_id)?;
        if spontaneous {
            check_spontaneous(core, &profile)?;
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
                || profile.details.connections.tts_voice.is_none())
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
    if spontaneous {
        if !app
            .state::<crate::initiative::Initiative>()
            .invitation_current(profile_id)
        {
            return Err(Error::StaleOperation.to_string());
        }
        with_core(shared.clone(), move |c| {
            check_spontaneous(c, &c.profile(profile_id)?)
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
    let (tag, token) = with_core(shared.clone(), move |c| {
        if c.session.snapshot().profile_id != profile_id {
            return Err(Error::StaleOperation);
        }
        if spontaneous {
            check_spontaneous(c, &c.profile(profile_id)?)?;
        }
        let tag = c.session.begin_text_turn();
        c.reset_live(tag);
        if !spontaneous {
            c.record_message(tag, MessageRole::User, &recorded, Delivery::TextOnly)?;
        }
        Ok((tag, c.session.cancellation()))
    })
    .await?;
    app.state::<crate::illustrations::Illustrations>()
        .cancel(&app, false);
    emit(&app, &shared);
    let _ = app.emit("sherka:history", profile_id);
    let priority = app
        .state::<crate::illustrations::Illustrations>()
        .priority(app.clone(), shared.clone());
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
        spontaneous,
    ));
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
                c.record_message(tag, MessageRole::User, &text, Delivery::TextOnly)?;
                c.live.transcript = text.clone();
                Ok(Some(text))
            })
            .await;
            voice
                .speech_input
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .finish(tag);
            match result {
                Ok(Some(messages)) => {
                    emit(&app, &shared);
                    let _ = app.emit("sherka:history", tag.profile_id);
                    run_turn(
                        app, shared, voice, tag, token, provider, messages, true, priority, false,
                    )
                    .await;
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

#[allow(clippy::too_many_arguments)]
async fn prepare_turn(
    app: &AppHandle,
    shared: &Shared,
    voice: &Voice,
    tag: OperationTag,
    provider: &mut Providers,
    user: &str,
    spontaneous: bool,
) -> std::result::Result<sherka_core::conversation::PreparedChat, String> {
    let models = voice.models.clone();
    let config = provider.connections.clone();
    let state = shared.clone();
    // Keep load/unload reconciliation alive even if the spoken turn is
    // interrupted while LM Studio is still loading a model.
    let model = tauri::async_runtime::spawn(async move {
        models
            .activate(&config, None, || {
                state.0.lock().is_ok_and(|c| c.session.accepts(tag))
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
    if spontaneous {
        return with_core(shared.clone(),move |c| {
            if !c.session.accepts(tag) { return Err(Error::StaleOperation); }
            let mut profile=c.profile(tag.profile_id)?;
            profile.details.connections.response_tokens=256;
            let budget=sherka_core::conversation::budget(&profile.details,&model)?;
            let history=c.store.context_history(tag.profile_id)?;
            let recall=c.store.recall(tag.profile_id, "")?;
            let mut chat=sherka_core::conversation::prepare_context_with_avatar(&profile.details,&history,
                "[Application idle event, not a user message] Take the initiative with one brief, natural invitation to chat in your character's language. At most two short sentences and one question. You may refer to an established topic, but do not invent recent events, user activity or new facts. Do not mention this timer or repeat your identity. No tools, web search or memory changes. Wait for a reply afterwards.",
                &recall,budget,None,None,!profile.emotion_avatars.is_empty()||profile.details.mood_persistence)?;
            // A bounded invitation uses the normal cancellable TTS pipeline.
            chat.budget.response_tokens=chat.budget.response_tokens.min(256);
            c.live.budget=Some(chat.budget.clone());
            Ok(chat)
        }).await;
    }
    let text = user.to_owned();
    let (profile, source, guard, visible) = with_core(shared.clone(), move |c| {
        if !c.session.accepts(tag) {
            return Err(Error::StaleOperation);
        }
        let profile = c.profile(tag.profile_id)?;
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
        Ok((profile, source, guard, visible))
    })
    .await?;
    let mut budget =
        sherka_core::conversation::budget(&profile.details, &model).map_err(|e| e.to_string())?;
    budget.safety_tokens += provider.images.len() * 4096;
    let mode = with_core(shared.clone(), |c| c.web_search_mode()).await?;
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
        )?;
        c.live.budget = Some(prepared.budget.clone());
        c.live.web = prepared.web.clone();
        Ok(prepared)
    })
    .await?;
    emit(app, shared);
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

fn schedule_memory(app: AppHandle, shared: Shared, profile: Uuid, provider: Providers) {
    let token = match shared.0.lock() {
        Ok(c) => c.memory_job.clone(),
        Err(_) => return,
    };
    tauri::async_runtime::spawn(async move {
        let job = async {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            let batch = with_core(shared.clone(), move |c| {
                if c.session.snapshot().profile_id != profile || c.live.tag.is_some() {
                    return Err(Error::StaleOperation);
                }
                c.store.extraction_batch(profile)
            })
            .await
            .ok()
            .flatten()?;
            let catalog = provider.catalog().await.ok()?;
            let model = sherka_core::models::selected(
                &catalog.models,
                provider.connections.llm_model.as_deref()?,
            )
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
            let check = token.clone();
            with_core(shared.clone(), move |c| {
                if check.is_cancelled()
                    || c.session.snapshot().profile_id != profile
                    || c.live.tag.is_some()
                {
                    return Err(Error::StaleOperation);
                }
                c.store.consolidate(&batch, value)
            })
            .await
            .ok()?;
            let _ = app.emit("sherka:memory", profile);
            Some(())
        };
        tokio::select! {biased; _=token.cancelled()=>{}, _=job=>{}}
    });
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
    spontaneous: bool,
) {
    let speech_end = voice.take_speech_end(tag);
    let clock = crate::turn_timing::TurnClock::new(speech_end);
    if speech_end.is_some() {
        clock.mark(crate::turn_timing::Mark::Transcribed);
    }
    tokio::select! {biased;_=token.cancelled()=>return,_=priority.ready()=>{}}
    let agent = provider.connections.llm_provider.is_agent();
    let maintenance = if agent || spontaneous {
        None
    } else {
        shared.0.lock().ok().and_then(|mut c| {
            let command = c.profile(tag.profile_id).ok().and_then(|p| {
                sherka_core::privacy::contextual_command(
                    &user,
                    &p.details.name,
                    c.pending_clear.valid(tag.profile_id, tag.session_id),
                )
            });
            if command.is_none() {
                c.pending_clear = Default::default();
            }
            command
        })
    };
    if let Some(command) = maintenance {
        if let Err(error) = maintenance_command(
            &app,
            shared.clone(),
            &voice,
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
            emit(&app, &shared);
        }
        return;
    }
    // A new utterance may arrive while the old run acknowledges cancellation.
    // Serialize remote runs and keep failed stops retryable instead of silently
    // starting overlapping agent work on the same server conversation.
    let _remote_gate = if agent {
        Some(
            tokio::select! { biased; _=token.cancelled()=>return, guard=voice.hermes_gate.lock()=>guard },
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
        return;
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
        } else {
            prepare_turn(
                &app,
                &shared,
                &voice,
                tag,
                &mut provider,
                &user,
                spontaneous,
            )
            .await
            .map(Some)
        }
    };
    let prepared = tokio::select! {
        biased;
        _=token.cancelled()=>return,
        result=preparation=>result
    };
    let prepared = match prepared {
        Ok(value) => value,
        Err(e) => {
            fail_turn(&app, &shared, tag, &e).await;
            return;
        }
    };
    clock.mark(crate::turn_timing::Mark::Prepared);
    if voice.playback.arm(token.clone()).is_err() {
        return;
    }
    if agent && let Ok(mut active) = voice.hermes_run.lock() {
        *active = Some((tag, hermes.clone()));
    }
    let (tokens_tx, mut tokens_rx) = mpsc::channel::<String>(64);
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
    let consume = async {
        let mut web_speech = sherka_core::web::SpeechFilter::default();
        let mut expression = sherka_core::avatar::ExpressionStream::default();
        let mut paint = LivePaint::default();
        let mut append =
            |text: String, emotion, paint: &mut LivePaint| -> sherka_core::Result<()> {
                let text = sherka_core::conversation::plain_reply(&text);
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
            let text = if expressions {
                expression.push(&text)
            } else {
                text
            };
            append(text, expression.emotion, &mut paint)?;
        }
        if expressions {
            append(expression.finish(), expression.emotion, &mut paint)?;
        }
        if speak && web_reply {
            speech_text.push(&web_speech.finish())?;
        }
        // The last coalesced tokens.
        emit(&app, &shared);
        if expressions {
            update(&app, &shared, tag, |c| {
                if c.live.reply.trim().is_empty() {
                    return Err(Error::Validation(
                        "Le fournisseur a renvoyé une réponse vide.".into(),
                    ));
                }
                Ok(())
            })?;
        }
        Ok::<_, Error>(())
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
                    let pcm = provider.speech(&phrase).await?;
                    audio_tx
                        .send((phrase, pcm))
                        .await
                        .map_err(|_| Error::StaleOperation)?;
                }
            }
            Ok::<_, Error>(())
        }
        .await;
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
                    let complete=update(&app,&shared,tag,|c|{
                        c.live.text_complete=true;
                        if !agent && !unavailable { c.complete_mood(tag, &user)?; }
                        if !agent && !unavailable && c.profile(tag.profile_id)?.details.illustration.enabled {
                            c.store.save_response(tag,&c.live.reply,&c.live.heard,if speak {Delivery::Uncertain}else{Delivery::TextOnly})?;
                        }
                        Ok(())
                    });
                    if complete.is_ok() && !agent && !unavailable {
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
    match result {
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
        }
        Some((Ok(outcome), Ok(()), spoken)) => {
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
            let delivery = if !speak || provider.connections.volume <= 0.0 {
                Delivery::TextOnly
            } else if spoken.is_ok() {
                Delivery::Completed
            } else {
                Delivery::Uncertain
            };
            let _ = with_core(shared.clone(), move |c| {
                if !c.session.accepts(tag) {return Err(Error::StaleOperation);}
                if outcome.length_limited {c.live.notice=Some("La réponse a atteint la réserve de tokens. Tu peux demander la suite ou augmenter cette réserve dans les réglages.".into());}
                c.live.usage=if unavailable {None} else {Some(outcome)};
                c.finish_response(tag, delivery)
            }).await;
            if !unavailable && !agent && !spontaneous {
                schedule_memory(
                    app.clone(),
                    shared.clone(),
                    tag.profile_id,
                    provider.clone(),
                );
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
        }
    }
    let reply = shared
        .0
        .lock()
        .map(|c| c.live.reply.chars().count())
        .unwrap_or_default();
    voice.diagnostics.record_timing(reply, &clock.summary());
    emit(&app, &shared);
    let _ = app.emit("sherka:history", tag.profile_id);
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
}
