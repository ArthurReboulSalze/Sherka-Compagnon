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
use std::sync::{Arc, Mutex};
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
    {
        let mut core = shared
            .0
            .lock()
            .map_err(|_| Error::Validation("Moteur indisponible.".into()))?;
        if !core.session.accepts(tag) {
            return Err(Error::StaleOperation);
        }
        f(&mut core)?;
    }
    emit(app, shared);
    Ok(())
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
        let p=c.store.profile(c.session.snapshot().profile_id)?;
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
    crate::windows::ensure_running()?;
    let gate = voice.gate.lock().await;
    crate::windows::ensure_running()?;
    if text.trim().is_empty() || text.len() > 128_000 {
        return Err("Écris un message de 1 à 128 000 octets.".into());
    }
    if text.trim().eq_ignore_ascii_case("/stop") {
        let agent = with_core(shared.clone(), move |c| {
            Ok(c.session.snapshot().profile_id == profile_id
                && c.store
                    .profile(profile_id)?
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
    let (connections, clearing) = with_core(shared.clone(), move |core| {
        if core.session.snapshot().profile_id != profile_id {
            return Err(Error::StaleOperation);
        }
        let profile = core.store.profile(profile_id)?;
        let clearing = !profile.details.connections.llm_provider.is_agent()
            && sherka_core::privacy::clear_conversation_intent(&command, &profile.details.name);
        if !clearing
            && !profile.details.connections.llm_provider.is_agent()
            && profile.details.connections.llm_model.is_none()
        {
            return Err(Error::Validation(
                "Choisis un modèle de conversation dans Compagnons.".into(),
            ));
        }
        if !clearing
            && speak
            && (profile.details.connections.tts_engine.is_none()
                || profile.details.connections.tts_voice.is_none())
        {
            return Err(Error::Validation(
                "Choisis un moteur vocal et une voix, ou désactive la lecture de la réponse."
                    .into(),
            ));
        }
        Ok((profile.details.connections, clearing))
    })
    .await?;
    if clearing {
        // A local privacy command also works with an offline or unconfigured
        // provider. Speech is only an optional, non-persisted acknowledgement.
        drop(gate);
        let session = clear(&app, shared.clone(), &voice, profile_id, false, None).await?;
        if speak
            && connections.tts_engine.is_some()
            && connections.tts_voice.is_some()
            && let Ok(provider) = Providers::new(connections)
        {
            tauri::async_runtime::spawn(async move {
                speak_clear_receipt(&app, shared, &voice, &provider, profile_id, session).await;
            });
        }
        return Ok(());
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
    voice.playback.interrupt();
    with_core(shared.clone(), |c| c.stop_response()).await?;
    if speak {
        voice.ensure_audio(&shared, false).await?;
    }
    let user = text.trim().to_owned();
    let recorded = user.clone();
    let (tag, token) = with_core(shared.clone(), move |c| {
        if c.session.snapshot().profile_id != profile_id {
            return Err(Error::StaleOperation);
        }
        let tag = c.session.begin_text_turn();
        c.reset_live(tag);
        c.record_message(tag, MessageRole::User, &recorded, Delivery::TextOnly)?;
        Ok((tag, c.session.cancellation()))
    })
    .await?;
    app.state::<crate::illustrations::Illustrations>()
        .cancel(&app, false);
    emit(&app, &shared);
    let _ = app.emit("sherka:history", profile_id);
    tauri::async_runtime::spawn(run_turn(
        app,
        shared,
        voice.clone(),
        tag,
        token,
        provider,
        user,
        speak,
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
                    let provider =
                        Providers::new(c.store.profile(tag.profile_id)?.details.connections)?;
                    Ok((tag, c.session.cancellation(), provider, previous))
                })
                .await;
                if let Ok((tag, token, provider, previous)) = prepared {
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
                    run_turn(app, shared, voice, tag, token, provider, messages, true).await;
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

pub async fn clear(
    app: &AppHandle,
    shared: Shared,
    voice: &Voice,
    profile_id: Uuid,
    erase_saved_facts: bool,
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
            c.store
                .profile(profile_id)?
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
        c.clear_conversation(profile_id, erase_saved_facts)?;
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
async fn speak_clear_receipt(
    app: &AppHandle,
    shared: Shared,
    voice: &Voice,
    provider: &Providers,
    profile_id: Uuid,
    session_id: Uuid,
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
    let text = sherka_core::i18n::text_in(
        language,
        "Conversation effacée. Les faits sauvegardés sont conservés.",
    );
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
        voice.playback.arm(token.clone())?;
        let pcm = provider.speech(&text).await?;
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

async fn prepare_turn(
    app: &AppHandle,
    shared: &Shared,
    voice: &Voice,
    tag: OperationTag,
    provider: &mut Providers,
    user: &str,
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
    let text = user.to_owned();
    let (profile, source, guard, visible) = with_core(shared.clone(), move |c| {
        if !c.session.accepts(tag) {
            return Err(Error::StaleOperation);
        }
        let profile = c.store.profile(tag.profile_id)?;
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
            .filter(|m| m.approved)
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
            !profile.emotion_avatars.is_empty(),
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
                Ok(c.store.profile(tag.profile_id)?.details.search)
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
        Ok(c) => match c.store.profile(c.session.snapshot().profile_id) {
            Ok(p) => (p.id, p.revision, p.details.connections),
            Err(_) => return,
        },
        Err(_) => return,
    };
    tauri::async_runtime::spawn(async move {
        let valid = || {
            shared.0.lock().is_ok_and(|c| {
                c.session.snapshot().profile_id == profile
                    && c.store
                        .profile(profile)
                        .is_ok_and(|p| p.revision == revision)
            })
        };
        let result = models.activate(&next, Some(&previous), valid).await;
        if let Ok(mut c) = shared.0.lock()
            && c.session.snapshot().profile_id == profile
            && c.store
                .profile(profile)
                .is_ok_and(|p| p.revision == revision)
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
) {
    let agent = provider.connections.llm_provider.is_agent();
    let clear_requested = !agent
        && shared.0.lock().is_ok_and(|c| {
            c.store.profile(tag.profile_id).is_ok_and(|p| {
                sherka_core::privacy::clear_conversation_intent(&user, &p.details.name)
            })
        });
    if clear_requested {
        match clear(
            &app,
            shared.clone(),
            &voice,
            tag.profile_id,
            false,
            Some(tag),
        )
        .await
        {
            Ok(session) if speak => {
                speak_clear_receipt(&app, shared, &voice, &provider, tag.profile_id, session).await
            }
            Ok(_) => {}
            Err(error) => fail_turn(&app, &shared, tag, &error).await,
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
            prepare_turn(&app, &shared, &voice, tag, &mut provider, &user)
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
        let mut append = |text: String, emotion| -> sherka_core::Result<()> {
            update(&app, &shared, tag, |c| {
                c.live.emotion = emotion;
                c.live.reply.push_str(&text);
                Ok(())
            })?;
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
        while let Some(text) = tokens_rx.recv().await {
            let text = if expressions {
                expression.push(&text)
            } else {
                text
            };
            append(text, expression.emotion)?;
        }
        if expressions {
            append(expression.finish(), expression.emotion)?;
        }
        if speak && web_reply {
            speech_text.push(&web_speech.finish())?;
        }
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
                    let complete=update(&app,&shared,tag,|c|{
                        c.live.text_complete=true;
                        if !agent && !unavailable && c.store.profile(tag.profile_id)?.details.illustration.enabled {
                            c.store.save_response(tag,&c.live.reply,&c.live.heard,if speak {Delivery::Uncertain}else{Delivery::TextOnly})?;
                        }
                        Ok(())
                    });
                    if complete.is_ok() && !agent && !unavailable {
                        app.state::<crate::illustrations::Illustrations>().request(app.clone(),shared.clone());
                    }
                }
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
            if !unavailable && !agent {
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

async fn play_prepared(
    app: &AppHandle,
    shared: &Shared,
    playback: &Playback,
    tag: OperationTag,
    token: &CancellationToken,
    volume: f32,
    mut audio: mpsc::Receiver<(String, Vec<i16>)>,
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
                update(app, shared, tag, |_| Ok(()))?;
                let receipt = playback.enqueue(pcm, volume, token.clone())?;
                receipts.push_back(async move {
                    receipt.started.await.map_err(|_| Error::StaleOperation)?;
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
