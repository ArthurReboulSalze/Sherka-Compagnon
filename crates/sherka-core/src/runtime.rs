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
    pub profiles: Vec<Profile>,
    pub settings: Settings,
    pub session: SessionSnapshot,
    pub data_directory: String,
    pub audio_available: bool,
    pub conversation_available: bool,
    pub live: crate::conversation::LiveConversation,
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
        let mut profiles = store.list()?;
        if profiles.is_empty() {
            profiles.push(store.create(ProfileDraft::default(), None)?);
        }
        let mut settings = store.settings()?;
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
        Ok(Snapshot {
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
        let updated = self.store.update(id, revision, draft)?;
        if self.settings.active_profile == Some(id)
            && (previous.details.personality != updated.details.personality
                || previous.details.instructions != updated.details.instructions
                || previous.details.connections != updated.details.connections)
        {
            self.stop_response()?;
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

    pub fn set_audio_settings(&mut self, audio: crate::voice::AudioSettings) -> Result<()> {
        audio.validate()?;
        let mut next = self.settings.clone();
        next.audio = audio;
        self.store.save_settings(&next)?;
        self.settings = next;
        self.pause_voice()
    }

    pub fn stop_response(&mut self) -> Result<()> {
        self.memory_job.cancel();
        // Invalidate first, even if disk persistence fails. Late workers cannot
        // append anything to this turn after the lock is released.
        let tag = self.live.tag;
        self.session.cancel();
        self.live.tag = None;
        self.live.transcript.clear();
        if let Some(tag) = tag {
            self.store.save_response(
                tag,
                &self.live.reply,
                &self.live.heard,
                crate::storage::Delivery::Interrupted,
            )?;
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
        self.session.complete(tag)?;
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
