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

/// Access is serialized by the desktop adapter. A profile switch loads and
/// persists the target before changing the session, all under that same lock.
pub struct Companion {
    pub store: Store,
    pub session: Session,
    pub live: crate::conversation::LiveConversation,
    pub memory_job: tokio_util::sync::CancellationToken,
    settings: Settings,
}

impl Companion {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let store = Store::open(root)?;
        let mut settings = store.settings()?;
        let mut profiles = store.list()?;
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
                AvatarPreset::Candis,
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
        let active = settings
            .active_profile
            .filter(|id| profiles.iter().any(|p| p.id == *id))
            .unwrap_or(profiles[0].id);
        settings.active_profile = Some(active);
        store.save_settings(&settings)?;
        Ok(Self {
            store,
            session: Session::new(active),
            live: Default::default(),
            memory_job: tokio_util::sync::CancellationToken::new(),
            settings,
        })
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
            profiles: self.store.list()?,
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
        let mut next = self.settings.clone();
        next.active_profile = Some(id);
        self.store.save_settings(&next)?;
        self.stop_response()?;
        self.session.switch_profile(id);
        self.live = Default::default();
        self.settings = next;
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
        if self.settings.active_profile == Some(id)
            && (previous.details.personality != draft.personality
                || previous.details.instructions != draft.instructions
                || previous.details.connections != draft.connections)
        {
            self.stop_response()?;
        }
        let updated = self.store.update(id, revision, draft)?;
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
        self.store.save_settings(&next)?;
        self.settings = next;
        Ok(())
    }

    pub fn audio_settings(&self) -> crate::voice::AudioSettings {
        self.settings.audio.clone()
    }

    pub fn web_search_mode(&self) -> Result<crate::web::SearchMode> {
        let profile = self.store.profile(self.session.snapshot().profile_id)?;
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
            if let Some(report) = &self.live.web {
                self.store.save_web_sources(tag, report)?;
            }
        }
        Ok(())
    }

    pub fn pause_voice(&mut self) -> Result<()> {
        self.session.pause_voice();
        self.stop_response()
    }

    pub fn reset_live(&mut self, tag: crate::session::OperationTag) {
        self.memory_job.cancel();
        self.memory_job = tokio_util::sync::CancellationToken::new();
        self.live = crate::conversation::LiveConversation {
            tag: Some(tag),
            ..Default::default()
        };
    }

    pub fn finish_response(
        &mut self,
        tag: crate::session::OperationTag,
        delivery: crate::storage::Delivery,
    ) -> Result<()> {
        if !self.session.accepts(tag) {
            return Err(Error::StaleOperation);
        }
        self.store
            .save_response(tag, &self.live.reply, &self.live.heard, delivery)?;
        if let Some(report) = &self.live.web {
            self.store.save_web_sources(tag, report)?;
        }
        self.session.complete(tag)?;
        self.live.emotion = Default::default();
        self.live.hermes_approval = None;
        self.live.tag = None;
        self.live.transcript.clear();
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
