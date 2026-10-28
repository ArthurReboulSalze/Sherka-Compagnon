use crate::{
    Error, Result,
    model::{Preferences, Profile, ProfileDraft, Settings},
    session::{Session, SessionSnapshot},
    storage::Store,
    window::Bounds,
};
use serde::Serialize;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub companion_capabilities: CompanionCapabilities,
    pub profiles: Vec<Profile>,
    pub settings: Settings,
    pub session: SessionSnapshot,
    pub data_directory: String,
    pub audio_available: bool,
    pub conversation_available: bool,
    pub live: crate::conversation::LiveConversation,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompanionCapabilities {
    pub local_personality: bool,
    pub local_memory: bool,
    /// Emotional avatars are local companions only; listening/speaking indicators
    /// remain available for remote agents.
    pub local_emotions: bool,
}

/// A companion that becomes the active one starts with its image generator
/// off, whatever it was left at: the user switches it on with the Image button
/// for the session. Saved like any edit, so its revision stays consistent.
fn illustrations_off(store: &Store, id: Uuid) -> Result<()> {
    let profile = store.profile(id)?;
    if profile.details.illustration.enabled {
        let mut draft = profile.details;
        draft.illustration.enabled = false;
        store.update(id, profile.revision, draft)?;
    }
    Ok(())
}

/// The guest of a three-way conversation. Session-only: never persisted.
struct Guest {
    id: Uuid,
    mood_epoch: i64,
}

/// Access is serialized by the desktop adapter. A profile switch loads and
/// persists the target before changing the session, all under that same lock.
pub struct Companion {
    pub store: Store,
    pub session: Session,
    pub live: crate::conversation::LiveConversation,
    pub memory_job: tokio_util::sync::CancellationToken,
    /// Pending relay of a three-way conversation; see `arm_relay`.
    relay: tokio_util::sync::CancellationToken,
    pub optimization: Option<crate::memory::OptimizationPreview>,
    pub pending_clear: crate::privacy::PendingClear,
    settings: Settings,
    mood_epoch: i64,
    guest: Option<Guest>,
}

impl Companion {
    pub fn search_config(&self) -> crate::search_provider::Config {
        self.settings.search.clone().unwrap_or_default()
    }
    pub fn set_search_config(&mut self, config: crate::search_provider::Config) -> Result<()> {
        let mut next = self.settings.clone();
        next.search = Some(config.validated()?);
        self.store.save_settings(&next)?;
        self.settings = next;
        Ok(())
    }
    pub fn trio_settings(&self) -> crate::model::TrioSettings {
        self.settings.trio.clone()
    }
    pub fn set_trio_settings(&mut self, config: crate::model::TrioSettings) -> Result<()> {
        let mut next = self.settings.clone();
        next.trio = config.validated()?;
        self.store.save_settings(&next)?;
        self.settings = next;
        Ok(())
    }
    /// Start waiting for a relay (and, after a second silence, the chain of
    /// replies that follows): the previous one is cancelled and the new token
    /// is returned. It is dropped by anything that changes the conversation: a
    /// new turn, a stop, a pause, a guest or profile change, or the user typing
    /// (see `cancel_relay`). The replies it produces use
    /// `reset_live_keeping_relay`, so they do not cancel it themselves.
    pub fn arm_relay(&mut self) -> tokio_util::sync::CancellationToken {
        self.relay.cancel();
        self.relay = tokio_util::sync::CancellationToken::new();
        self.relay.clone()
    }
    pub fn cancel_relay(&mut self) {
        self.relay.cancel();
    }
    /// The latest relay has not been cancelled: a reaction prepared for it
    /// during the answer (detached from the answer's turn) may keep working.
    pub fn relay_active(&self) -> bool {
        !self.relay.is_cancelled()
    }
    pub fn voice_service_config(&self) -> crate::model::VoiceServiceConfig {
        self.settings.voice_service.clone().unwrap_or_default()
    }
    fn apply_voice_service(&self, p: &mut Profile) {
        let config = self.voice_service_config();
        p.details.connections.voice_studio_url = config.url;
        p.details.connections.stt_model = config.stt_model;
    }
    pub fn profile(&self, id: Uuid) -> Result<Profile> {
        let mut p = self.store.profile(id)?;
        self.apply_voice_service(&mut p);
        Ok(p)
    }
    /// The profile a generation runs with. A guest borrows the master's LLM
    /// connection and keeps its own personality, voice, language and memory.
    pub fn speaking_profile(&self, id: Uuid) -> Result<Profile> {
        let mut p = self.profile(id)?;
        if self.guest.as_ref().is_some_and(|g| g.id == id) {
            let master = self.profile(self.session.snapshot().profile_id)?;
            p.details.connections = p
                .details
                .connections
                .with_llm_of(&master.details.connections);
        }
        Ok(p)
    }
    pub fn guest(&self) -> Option<Uuid> {
        self.guest.as_ref().map(|g| g.id)
    }
    /// (master, guest) while a three-way conversation is active.
    pub fn participants(&self) -> Option<(crate::trio::Participant, crate::trio::Participant)> {
        let guest = self.guest.as_ref()?;
        let who = |id| {
            Some(crate::trio::Participant {
                id,
                name: self.store.profile(id).ok()?.details.name,
            })
        };
        Some((who(self.session.snapshot().profile_id)?, who(guest.id)?))
    }
    /// The speaker and the other participant, when `speaker` is in the trio.
    fn pair_for(
        &self,
        speaker: Uuid,
    ) -> Option<(crate::trio::Participant, crate::trio::Participant)> {
        let (master, guest) = self.participants()?;
        if speaker == master.id {
            Some((master, guest))
        } else if speaker == guest.id {
            Some((guest, master))
        } else {
            None
        }
    }
    fn copy_to_peer(
        &self,
        tag: crate::session::OperationTag,
        delivery: crate::storage::Delivery,
    ) -> Result<()> {
        if let Some((speaker, other)) = self.pair_for(tag.profile_id) {
            let speaker = crate::storage::Speaker {
                id: speaker.id,
                name: speaker.name,
            };
            self.store.append_peer_reply(
                other.id,
                tag,
                &speaker,
                &self.live.reply,
                &self.live.heard,
                delivery,
            )?;
        }
        Ok(())
    }
    pub fn set_guest(&mut self, id: Option<Uuid>) -> Result<()> {
        let Some(id) = id else {
            self.cancel_relay();
            if self.guest.is_some() {
                self.stop_response()?;
                self.guest = None;
                self.session.set_guest(None);
                self.live.guest = None;
            }
            return Ok(());
        };
        if self.guest() == Some(id) {
            return Ok(());
        }
        let guest = self.store.profile(id)?;
        let master = self.store.profile(self.session.snapshot().profile_id)?;
        if guest.id == master.id {
            return Err(Error::Validation(
                "Ce compagnon est déjà ton compagnon principal.".into(),
            ));
        }
        if master.details.connections.llm_provider.is_agent()
            || guest.details.connections.llm_provider.is_agent()
        {
            return Err(Error::Validation(
                "Les compagnons Hermes ne participent pas aux conversations à trois.".into(),
            ));
        }
        if !has_voice(&guest) {
            return Err(Error::Validation(
                "Choisis d’abord une voix pour ce compagnon.".into(),
            ));
        }
        self.cancel_relay();
        self.stop_response()?;
        self.guest = Some(Guest { id, mood_epoch: 0 });
        self.session.set_guest(Some(id));
        self.load_guest_mood()?;
        let mut next = self.settings.clone();
        let recent = next.recent_guests.entry(master.id).or_default();
        recent.retain(|g| *g != id && *g != master.id);
        recent.insert(0, id);
        recent.truncate(3);
        self.store.save_settings(&next)?;
        self.settings = next;
        Ok(())
    }
    /// Delete a companion; a guest leaves the conversation first.
    pub fn delete_profile(&mut self, id: Uuid) -> Result<()> {
        if self.guest() == Some(id) {
            self.set_guest(None)?;
        }
        self.store.delete(id)?;
        let mut next = self.settings.clone();
        prune_recent_guests(&mut next, &self.store.list()?);
        if next != self.settings {
            self.store.save_settings(&next)?;
            self.settings = next;
        }
        Ok(())
    }
    pub fn set_voice_service(&mut self, config: crate::model::VoiceServiceConfig) -> Result<()> {
        let config = config.validated()?;
        self.pause_voice()?;
        let mut next = self.settings.clone();
        next.voice_service = Some(config);
        self.store.save_settings(&next)?;
        self.settings = next;
        Ok(())
    }
    pub fn save_widget_cell(
        &mut self,
        id: Uuid,
        image: Option<crate::model::ImagePlacement>,
        text_height: Option<f64>,
    ) -> Result<()> {
        if self.session.snapshot().profile_id != id {
            return Err(Error::StaleOperation);
        }
        if image.is_some_and(|p| {
            !p.x.is_finite()
                || !p.y.is_finite()
                || !p.size.is_finite()
                || !(-800.0..=900.0).contains(&p.x)
                || !(-600.0..=800.0).contains(&p.y)
                || !(140.0..=1600.0).contains(&p.size)
        }) || text_height.is_some_and(|h| !h.is_finite() || !(32.0..=600.0).contains(&h))
        {
            return Err(Error::Validation(
                "La taille du widget est invalide.".into(),
            ));
        }
        let mut next = self.settings.clone();
        let layout = next.desktop_layouts.entry(id).or_default();
        layout.image = image;
        layout.text_height = text_height;
        self.store.save_settings(&next)?;
        self.settings = next;
        Ok(())
    }

    pub fn comfy_startup_config(&self) -> crate::model::ComfyStartupConfig {
        self.settings.comfyui_startup.clone()
    }
    pub fn set_comfy_startup_config(
        &mut self,
        config: crate::model::ComfyStartupConfig,
    ) -> Result<()> {
        let mut next = self.settings.clone();
        next.comfyui_startup = config.validated()?;
        self.store.save_settings(&next)?;
        self.settings = next;
        Ok(())
    }
    pub fn set_theme(&mut self, theme: crate::model::Theme) -> Result<()> {
        let mut next = self.settings.clone();
        next.theme = theme;
        self.store.save_settings(&next)?;
        self.settings = next;
        Ok(())
    }
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let store = Store::open(root)?;
        let mut settings = store.settings()?;
        let mut profiles = store.list()?;
        for profile in &mut profiles {
            let mut draft = profile.details.clone();
            let appearance_changed = crate::illustration::upgrade_appearance(&mut draft);
            if crate::presets::upgrade_candis(&mut draft) || appearance_changed {
                *profile = store.update(profile.id, profile.revision, draft)?;
            }
        }
        if profiles.is_empty() {
            let profile = store.create(crate::presets::sherka(settings.ui_language), None)?;
            settings.default_companion = Some(profile.id);
            settings.default_companion_initialized = true;
            profiles.push(profile);
        } else if !settings.default_companion_initialized {
            let mut originals = profiles
                .iter()
                .filter(|profile| crate::presets::is_builtin_sherka(&profile.details));
            settings.default_companion = originals.next().map(|profile| profile.id);
            if originals.next().is_some() {
                settings.default_companion = None;
            }
            settings.default_companion_initialized = true;
        }
        if let Some(id) = settings.default_companion
            && (!profiles.iter().any(|profile| profile.id == id)
                || store
                    .localize_default_companion(id, settings.ui_language)?
                    .is_none())
        {
            settings.default_companion = None;
        }
        if !settings.companion_pack_initialized {
            use crate::avatar::AvatarPreset;
            for preset in [
                AvatarPreset::Botsy,
                AvatarPreset::Alba,
                AvatarPreset::Kit,
                AvatarPreset::Minerva,
            ] {
                if !profiles.iter().any(|p| {
                    p.details.avatar_preset == Some(preset) || p.details.name == preset.name()
                }) {
                    profiles.push(store.create(
                        crate::presets::companion(preset, settings.ui_language),
                        None,
                    )?);
                }
            }
            settings.companion_pack_initialized = true;
        }
        settings
            .desktop_layouts
            .retain(|id, _| profiles.iter().any(|p| p.id == *id));
        prune_recent_guests(&mut settings, &profiles);
        let active = settings
            .active_profile
            .filter(|id| profiles.iter().any(|p| p.id == *id))
            .unwrap_or(profiles[0].id);
        settings.active_profile = Some(active);
        illustrations_off(&store, active)?;
        if settings.search.is_none() {
            // Migrate once. Preserve the active companion's configured service,
            // or another configured service if the active one was still default.
            let active_search = store.profile(active)?.details.search;
            let config = if active_search != Default::default() {
                active_search
            } else {
                profiles
                    .iter()
                    .find(|p| p.details.search != Default::default())
                    .map(|p| p.details.search.clone())
                    .unwrap_or_default()
            };
            settings.search = Some(config.validated()?);
        }
        if settings.voice_service.is_none() {
            let connections = &store.profile(active)?.details.connections;
            settings.voice_service = Some(crate::model::VoiceServiceConfig {
                url: connections.voice_studio_url.clone(),
                stt_model: connections.stt_model.clone(),
                ..Default::default()
            });
        }
        let layout =
            settings
                .desktop_layouts
                .entry(active)
                .or_insert_with(|| crate::model::DesktopLayout {
                    bounds: settings.widget_bounds,
                    ..Default::default()
                });
        settings.widget_bounds = layout.bounds;
        store.save_settings(&settings)?;
        let mut companion = Self {
            store,
            session: Session::new(active),
            live: Default::default(),
            memory_job: tokio_util::sync::CancellationToken::new(),
            relay: tokio_util::sync::CancellationToken::new(),
            optimization: None,
            pending_clear: Default::default(),
            settings,
            mood_epoch: 0,
            guest: None,
        };
        companion.load_mood()?;
        Ok(companion)
    }

    pub fn snapshot(&self) -> Result<Snapshot> {
        let local = !self
            .store
            .profile(self.session.snapshot().profile_id)?
            .details
            .connections
            .llm_provider
            .is_agent();
        Ok(Snapshot {
            companion_capabilities: CompanionCapabilities {
                local_personality: local,
                local_memory: local,
                local_emotions: local,
            },
            profiles: self
                .store
                .list()?
                .into_iter()
                .map(|mut p| {
                    self.apply_voice_service(&mut p);
                    p
                })
                .collect(),
            settings: self.settings.clone(),
            session: self.session.snapshot(),
            data_directory: self.store.root().display().to_string(),
            audio_available: true,
            conversation_available: true,
            live: self.live.clone(),
        })
    }

    pub fn switch_profile(&mut self, id: Uuid) -> Result<()> {
        self.store.profile(id)?;
        if self.settings.active_profile == Some(id) {
            return Ok(());
        }
        illustrations_off(&self.store, id)?;
        let mut next = self.settings.clone();
        next.active_profile = Some(id);
        next.widget_bounds = next.desktop_layouts.get(&id).and_then(|v| v.bounds);
        self.store.save_settings(&next)?;
        self.cancel_relay();
        self.stop_response()?;
        self.session.switch_profile(id);
        self.optimization = None;
        self.pending_clear = Default::default();
        self.live = Default::default();
        self.guest = None;
        self.settings = next;
        self.load_mood()?;
        Ok(())
    }

    pub fn update_profile(&mut self, id: Uuid, revision: i64, draft: ProfileDraft) -> Result<()> {
        let previous = self.store.profile(id)?;
        if previous.details.connections.language != draft.connections.language {
            return Err(Error::Validation("La langue du compagnon est fixée à sa création. Crée un autre compagnon pour utiliser une autre langue.".into()));
        }
        let draft = draft.validated()?;
        if previous.revision != revision {
            return Err(Error::Conflict);
        }
        if (self.settings.active_profile == Some(id) || self.guest() == Some(id))
            && (previous.details.personality != draft.personality
                || previous.details.instructions != draft.instructions
                || previous.details.connections != draft.connections)
        {
            self.stop_response()?;
        }
        let updated = self.store.update(id, revision, draft)?;
        if self.settings.active_profile == Some(id)
            && (previous.details.mood_persistence != updated.details.mood_persistence
                || previous.details.connections.llm_provider
                    != updated.details.connections.llm_provider)
        {
            self.live.mood_updated = true;
            self.load_mood()?;
        }
        if self.guest() == Some(id)
            && previous.details.mood_persistence != updated.details.mood_persistence
        {
            self.reload_guest_mood()?;
        }
        if let Some(guest) = self.guest() {
            let unfit = |p: Profile| p.details.connections.llm_provider.is_agent();
            let master = self.store.profile(self.session.snapshot().profile_id)?;
            let guest = self.store.profile(guest)?;
            if !has_voice(&guest) || unfit(master) || unfit(guest) {
                self.set_guest(None)?;
            }
        }
        if self.settings.default_companion == Some(id)
            && (previous.details.name != updated.details.name
                || previous.details.personality != updated.details.personality
                || previous.details.instructions != updated.details.instructions)
        {
            let mut next = self.settings.clone();
            next.default_companion = None;
            self.store.save_settings(&next)?;
            self.settings = next;
        }
        Ok(())
    }

    pub fn set_preferences(&mut self, preferences: Preferences) -> Result<()> {
        preferences.validate()?;
        let mut next = self.settings.clone();
        next.always_on_top = preferences.always_on_top;
        next.widget_scale = preferences.widget_scale;
        self.store.save_settings(&next)?;
        self.settings = next;
        Ok(())
    }

    pub fn set_interface_language(&mut self, language: crate::i18n::Language) -> Result<()> {
        let mut next = self.settings.clone();
        next.ui_language = language;
        if let Some(id) = next.default_companion {
            if self.interface_language_changes_active_companion(language)? {
                self.pause_voice()?;
            }
            match self.store.localize_default_companion(id, language) {
                Ok(Some(_)) => {}
                Ok(None) | Err(Error::ProfileNotFound) => next.default_companion = None,
                Err(error) => return Err(error),
            }
        }
        self.store.save_settings(&next)?;
        self.settings = next;
        Ok(())
    }

    pub fn interface_language_changes_active_companion(
        &self,
        language: crate::i18n::Language,
    ) -> Result<bool> {
        if self.settings.default_companion != self.settings.active_profile {
            return Ok(false);
        }
        let profile = self.store.profile(self.session.snapshot().profile_id)?;
        Ok(crate::presets::is_builtin_sherka(&profile.details)
            && profile.details.connections.language != language.code())
    }

    pub fn save_bounds(&mut self, bounds: Bounds) -> Result<()> {
        if bounds.width == 0 || bounds.height == 0 {
            return Ok(());
        }
        let mut next = self.settings.clone();
        next.widget_bounds = Some(bounds);
        next.desktop_layouts
            .entry(self.session.snapshot().profile_id)
            .or_default()
            .bounds = Some(bounds);
        self.store.save_settings(&next)?;
        self.settings = next;
        Ok(())
    }

    /// Position of the guest's own widget window; never touches the master's.
    pub fn save_guest_bounds(&mut self, id: Uuid, bounds: Bounds) -> Result<()> {
        if self.guest() != Some(id) {
            return Err(Error::StaleOperation);
        }
        if bounds.width == 0 || bounds.height == 0 {
            return Ok(());
        }
        let mut next = self.settings.clone();
        next.desktop_layouts.entry(id).or_default().bounds = Some(bounds);
        self.store.save_settings(&next)?;
        self.settings = next;
        Ok(())
    }

    pub fn guest_bounds(&self, id: Uuid) -> Option<Bounds> {
        self.settings.desktop_layouts.get(&id)?.bounds
    }

    pub fn audio_settings(&self) -> crate::voice::AudioSettings {
        self.settings.audio.clone()
    }

    pub fn web_search_mode(&self) -> Result<crate::web::SearchMode> {
        self.web_search_mode_for(self.session.snapshot().profile_id)
    }

    pub fn web_search_mode_for(&self, profile_id: Uuid) -> Result<crate::web::SearchMode> {
        let profile = self.store.profile(profile_id)?;
        Ok(if profile.details.connections.llm_provider.is_agent() {
            crate::web::SearchMode::Off
        } else {
            profile
                .details
                .web_search
                .unwrap_or(self.settings.web_search)
        })
    }

    pub fn set_web_search_mode(&mut self, mode: crate::web::SearchMode) -> Result<()> {
        let profile = self.store.profile(self.session.snapshot().profile_id)?;
        self.set_profile_web_search(profile.id, profile.revision, mode)
    }

    pub fn set_profile_web_search(
        &mut self,
        id: Uuid,
        revision: i64,
        mode: crate::web::SearchMode,
    ) -> Result<()> {
        let mut draft = self.store.profile(id)?.details;
        draft.web_search = Some(mode);
        if self.session.snapshot().profile_id == id {
            self.stop_response()?;
        }
        self.update_profile(id, revision, draft)
    }

    pub fn set_audio_settings(&mut self, audio: crate::voice::AudioSettings) -> Result<()> {
        audio.validate()?;
        let mut next = self.settings.clone();
        next.audio = audio;
        self.store.save_settings(&next)?;
        self.settings = next;
        self.pause_voice()
    }

    pub fn stop_response(&mut self) -> Result<()> {
        self.cancel_relay();
        self.live.emotion = Default::default();
        self.live.hermes_approval = None;
        self.memory_job.cancel();
        // Invalidate first, even if disk persistence fails. Late workers cannot
        // append anything to this turn after the lock is released.
        let tag = self.live.tag;
        self.session.cancel();
        self.live.tag = None;
        self.live.transcript.clear();
        if let Some(report) = &mut self.live.web
            && report.searching
        {
            report.searching = false;
            report.error = Some("La recherche Internet a été interrompue.".into());
        }
        if let Some(tag) = tag {
            self.store.save_response(
                tag,
                &self.live.reply,
                &self.live.heard,
                crate::storage::Delivery::Interrupted,
            )?;
            self.copy_to_peer(tag, crate::storage::Delivery::Interrupted)?;
            if let Some(report) = &self.live.web {
                self.store.save_web_sources(tag, report)?;
            }
        }
        Ok(())
    }

    /// Turn the microphone off. A reply already being generated or spoken
    /// continues; only an utterance the user is still speaking is dropped.
    pub fn mute_microphone(&mut self) {
        let state = self.session.snapshot();
        if state.transcription == crate::session::TranscriptionState::Capturing
            && state.response == crate::session::ResponseState::Idle
        {
            self.session.cancel();
            self.live.tag = None;
            self.live.transcript.clear();
        }
        self.session.mute_microphone();
    }

    pub fn pause_voice(&mut self) -> Result<()> {
        self.cancel_relay();
        self.session.pause_voice();
        self.stop_response()
    }

    pub fn clear_conversation(
        &mut self,
        profile: uuid::Uuid,
        erase_saved_facts: bool,
    ) -> Result<()> {
        self.clear_conversation_with_mood(profile, erase_saved_facts, false)
    }

    pub fn clear_conversation_with_mood(
        &mut self,
        profile: Uuid,
        erase_saved_facts: bool,
        erase_mood: bool,
    ) -> Result<()> {
        self.store.profile(profile)?;
        let active = self.session.snapshot().profile_id == profile;
        let guest = self.guest() == Some(profile);
        if active || guest {
            // Cancel before deleting so a late reply or extraction cannot
            // restore the conversation. Microphone consent remains unchanged.
            self.stop_response()?;
        }
        self.store
            .clear_conversation_with_mood(profile, erase_saved_facts, erase_mood)?;
        self.store.clear_hermes_sessions(profile)?;
        if active {
            // The conversation restarts, but the guest stays invited.
            let guest = self.guest();
            self.session.switch_profile(profile);
            self.session.set_guest(guest);
            self.pending_clear = Default::default();
            self.optimization = None;
            self.live = Default::default();
            self.load_mood()?;
            self.load_guest_mood()?;
        } else if guest {
            self.reload_guest_mood()?;
        }
        Ok(())
    }

    /// A turn of the user: cancels the relay and a pending memory extraction
    /// (the new turn schedules its own).
    pub fn reset_live(&mut self, tag: crate::session::OperationTag) {
        self.memory_job.cancel();
        self.memory_job = tokio_util::sync::CancellationToken::new();
        self.reset_live_for_event(tag);
    }

    /// A turn the companions start on their own (idle invitation, arrival
    /// remark, second speaker of a spontaneous exchange): ends the relay, but a
    /// pending memory extraction of the user's last turn stays alive.
    pub fn reset_live_for_event(&mut self, tag: crate::session::OperationTag) {
        self.cancel_relay();
        self.reset_live_keeping_relay(tag);
    }

    /// `reset_live` for a reply the relay itself produces (a relay or a chained
    /// reply): the relay token that is running it must stay alive, so only the
    /// user's activity, a stop, a pause or a guest/profile change cancels it.
    /// Neither does it cancel a pending memory extraction.
    pub fn reset_live_keeping_relay(&mut self, tag: crate::session::OperationTag) {
        self.live = crate::conversation::LiveConversation {
            tag: Some(tag),
            mood: self.live.mood,
            guest: self.live.guest,
            ..Default::default()
        };
    }

    /// The mood epoch and the resting mood shown for a companion.
    fn resting_mood(&self, id: Uuid) -> Result<(i64, crate::avatar::Emotion)> {
        let profile = self.store.profile(id)?;
        let stored = self.store.mood(id)?;
        let mood = if profile.details.mood_persistence
            && !profile.details.connections.llm_provider.is_agent()
        {
            stored.state.dominant
        } else {
            Default::default()
        };
        Ok((stored.epoch, mood))
    }

    fn load_mood(&mut self) -> Result<()> {
        let (epoch, mood) = self.resting_mood(self.session.snapshot().profile_id)?;
        self.mood_epoch = epoch;
        self.live.mood = mood;
        Ok(())
    }

    fn load_guest_mood(&mut self) -> Result<()> {
        let Some(id) = self.guest() else {
            return Ok(());
        };
        let (epoch, mood) = self.resting_mood(id)?;
        self.guest = Some(Guest {
            id,
            mood_epoch: epoch,
        });
        self.live.guest = Some(crate::conversation::GuestLive {
            profile_id: id,
            mood,
        });
        Ok(())
    }

    /// Like `load_mood` for the guest. A reply the guest is producing cannot
    /// immediately restore the mood that was just reset.
    fn reload_guest_mood(&mut self) -> Result<()> {
        if let (Some(id), Some(tag)) = (self.guest(), self.live.tag)
            && tag.profile_id == id
        {
            self.live.mood_updated = true;
        }
        self.load_guest_mood()
    }

    pub fn reset_mood(&mut self, id: Uuid) -> Result<()> {
        self.store.reset_mood(id)?;
        if self.session.snapshot().profile_id == id {
            // A reply already in flight cannot immediately restore this mood.
            self.live.mood_updated = true;
            self.load_mood()?;
        } else if self.guest() == Some(id) {
            self.reload_guest_mood()?;
        }
        Ok(())
    }

    pub fn complete_mood(&mut self, tag: crate::session::OperationTag, user: &str) -> Result<()> {
        if !self.session.accepts(tag) {
            return Err(Error::StaleOperation);
        }
        if !self.live.mood_updated && !self.live.reply.trim().is_empty() {
            let guest = self.guest.as_ref().filter(|g| g.id == tag.profile_id);
            let epoch = guest.map_or(self.mood_epoch, |g| g.mood_epoch);
            let mood = self.store.advance_mood(
                tag.profile_id,
                epoch,
                tag.turn_id,
                user,
                self.live.emotion,
            )?;
            match (guest.is_some(), &mut self.live.guest) {
                (true, Some(live)) => live.mood = mood,
                _ => self.live.mood = mood,
            }
            self.live.mood_updated = true;
        }
        Ok(())
    }

    /// Save one speaker's reply (and its copy for the other participant of a
    /// three-way conversation) without ending the turn.
    pub fn finish_generation(
        &mut self,
        tag: crate::session::OperationTag,
        delivery: crate::storage::Delivery,
    ) -> Result<()> {
        if !self.session.accepts(tag) {
            return Err(Error::StaleOperation);
        }
        self.store
            .save_response(tag, &self.live.reply, &self.live.heard, delivery.clone())?;
        if let Some(report) = &self.live.web {
            self.store.save_web_sources(tag, report)?;
        }
        self.copy_to_peer(tag, delivery)?;
        self.live.emotion = Default::default();
        self.live.hermes_approval = None;
        // A barge-in between two speakers must not save this reply again.
        self.live.tag = None;
        self.live.transcript.clear();
        Ok(())
    }

    pub fn finish_response(
        &mut self,
        tag: crate::session::OperationTag,
        delivery: crate::storage::Delivery,
    ) -> Result<()> {
        self.finish_generation(tag, delivery)?;
        self.session.complete(tag)
    }

    /// A second speaker who chose to pass: the turn ends, nothing is saved.
    pub fn discard_generation(&mut self, tag: crate::session::OperationTag) -> Result<()> {
        if !self.session.accepts(tag) {
            return Err(Error::StaleOperation);
        }
        self.session.complete(tag)?;
        self.live.tag = None;
        self.live.reply.clear();
        self.live.heard.clear();
        self.live.emotion = Default::default();
        Ok(())
    }

    /// The user's message, written to every participant's database.
    pub fn record_user_message(
        &self,
        tag: crate::session::OperationTag,
        text: &str,
        delivery: crate::storage::Delivery,
    ) -> Result<()> {
        self.record_message(
            tag,
            crate::storage::MessageRole::User,
            text,
            delivery.clone(),
        )?;
        if let Some((_, other)) = self.pair_for(tag.profile_id) {
            self.store
                .append_user_message_to(other.id, tag, text, delivery)?;
        }
        Ok(())
    }

    pub fn record_message(
        &self,
        tag: crate::session::OperationTag,
        role: crate::storage::MessageRole,
        text: &str,
        delivery: crate::storage::Delivery,
    ) -> Result<()> {
        if !self.session.accepts(tag) {
            return Err(Error::StaleOperation);
        }
        self.store.append_message(tag, role, text, delivery)
    }
}

fn has_voice(profile: &Profile) -> bool {
    profile
        .details
        .connections
        .tts_voice
        .as_deref()
        .is_some_and(|v| !v.trim().is_empty())
}

fn prune_recent_guests(settings: &mut Settings, profiles: &[Profile]) {
    let exists = |id: &Uuid| profiles.iter().any(|p| p.id == *id);
    settings.recent_guests.retain(|master, _| exists(master));
    for list in settings.recent_guests.values_mut() {
        list.retain(exists);
    }
}
