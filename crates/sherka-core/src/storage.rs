use crate::{
    Error, Result,
    model::{Profile, ProfileDraft, ProfileExport, Settings},
    session::OperationTag,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use image::{ImageFormat, ImageReader, Limits};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

const SCHEMA_VERSION: i64 = 9;
const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;
const MAX_EXPORT_BYTES: usize = 48 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Delivery {
    TextOnly,
    Completed,
    Interrupted,
    Uncertain,
}

#[derive(Debug, Clone, Serialize)]
pub struct Message {
    pub web: Option<crate::web::Report>,
    pub id: String,
    pub role: MessageRole,
    pub content: String,
    pub delivery: Delivery,
    pub heard_content: String,
    pub created_at: i64,
}

#[derive(Clone)]
pub struct Store {
    root: PathBuf,
}

fn connect(path: &Path, create: bool) -> Result<Connection> {
    let mut flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    if create {
        flags |= OpenFlags::SQLITE_OPEN_CREATE;
    }
    let db = Connection::open_with_flags(path, flags)?;
    db.busy_timeout(Duration::from_secs(3))?;
    db.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
    let version: i64 = db.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version > SCHEMA_VERSION {
        return Err(Error::FutureSchema);
    }
    Ok(db)
}

impl Store {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let store = Self { root: root.into() };
        Self::preflight(&store.root)?;
        fs::create_dir_all(store.root.join("profiles"))?;
        let mut db = connect(&store.root.join("settings.sqlite3"), true)?;
        let tx = db.transaction()?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS settings (id INTEGER PRIMARY KEY CHECK(id=1), value TEXT NOT NULL); PRAGMA user_version = 9;")?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS hermes_sessions (companion TEXT NOT NULL, endpoint TEXT NOT NULL, session TEXT NOT NULL, PRIMARY KEY(companion, endpoint)); CREATE TABLE IF NOT EXISTS hermes_session_tips (session TEXT PRIMARY KEY, tip TEXT NOT NULL); CREATE TABLE IF NOT EXISTS hermes_profiles (server TEXT NOT NULL, profile TEXT NOT NULL, PRIMARY KEY(server, profile));")?;
        tx.execute(
            "INSERT OR IGNORE INTO settings(id,value) VALUES(1,?1)",
            [serde_json::to_string(&Settings::default())?],
        )?;
        tx.commit()?;
        Ok(store)
    }

    /// Remote conversation identifiers are device-local, never profile exports.
    pub fn hermes_session(&self, companion: Uuid, endpoint: &str) -> Result<String> {
        self.profile(companion)?;
        let db = connect(&self.root.join("settings.sqlite3"), false)?;
        db.execute(
            "INSERT OR IGNORE INTO hermes_sessions(companion,endpoint,session) VALUES(?1,?2,?3)",
            params![
                companion.to_string(),
                endpoint,
                format!("sherka_{}", Uuid::new_v4().simple())
            ],
        )?;
        Ok(db.query_row(
            "SELECT session FROM hermes_sessions WHERE companion=?1 AND endpoint=?2",
            params![companion.to_string(), endpoint],
            |r| r.get(0),
        )?)
    }

    pub fn hermes_session_tip(&self, session: &str) -> Result<String> {
        let db = connect(&self.root.join("settings.sqlite3"), false)?;
        let tip: Option<String> = db
            .query_row(
                "SELECT tip FROM hermes_session_tips WHERE session=?1",
                [session],
                |r| r.get(0),
            )
            .optional()?;
        Ok(tip.unwrap_or_else(|| session.to_owned()))
    }
    pub fn update_hermes_session_tip(&self, session: &str, tip: &str) -> Result<()> {
        let db = connect(&self.root.join("settings.sqlite3"), false)?;
        db.execute("INSERT INTO hermes_session_tips(session,tip) VALUES(?1,?2) ON CONFLICT(session) DO UPDATE SET tip=excluded.tip",params![session,tip])?;
        Ok(())
    }

    pub fn hermes_profiles(&self, server: &str) -> Result<Vec<String>> {
        let db = connect(&self.root.join("settings.sqlite3"), false)?;
        let mut query =
            db.prepare("SELECT profile FROM hermes_profiles WHERE server=?1 ORDER BY profile")?;
        let mut names = query
            .query_map([server], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if !names.iter().any(|s| s == "default") {
            names.insert(0, "default".into());
        }
        Ok(names)
    }

    pub fn hermes_servers(&self) -> Result<Vec<String>> {
        let db = connect(&self.root.join("settings.sqlite3"), false)?;
        let mut query =
            db.prepare("SELECT DISTINCT server FROM hermes_profiles ORDER BY server")?;
        Ok(query
            .query_map([], |r| r.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn new_hermes_session(&self, companion: Uuid) -> Result<()> {
        let config = self.profile(companion)?.details.connections;
        if !config.llm_provider.is_agent() {
            return Err(Error::StaleOperation);
        }
        let db = connect(&self.root.join("settings.sqlite3"), false)?;
        db.execute(
            "DELETE FROM hermes_sessions WHERE companion=?1 AND endpoint=?2",
            params![companion.to_string(), config.api_base()?],
        )?;
        self.hermes_session(companion, &config.api_base()?)?;
        Ok(())
    }

    pub fn clear_hermes_sessions(&self, companion: Uuid) -> Result<()> {
        let mut db = connect(&self.root.join("settings.sqlite3"), false)?;
        let tx = db.transaction()?;
        tx.execute("DELETE FROM hermes_session_tips WHERE session IN (SELECT session FROM hermes_sessions WHERE companion=?1)", [companion.to_string()])?;
        tx.execute(
            "DELETE FROM hermes_sessions WHERE companion=?1",
            [companion.to_string()],
        )?;
        tx.commit()?;
        Ok(())
    }

    fn transcript_session(&self, id: Uuid) -> Result<Option<String>> {
        let config = self.profile(id)?.details.connections;
        if config.llm_provider.is_agent() {
            self.hermes_session(id, &config.api_base()?).map(Some)
        } else {
            Ok(None)
        }
    }

    pub fn remember_hermes_profile(&self, server: &str, profile: &str) -> Result<()> {
        let server = crate::hermes::server_url(server)?;
        let profile = crate::hermes::profile_name(profile)?;
        let db = connect(&self.root.join("settings.sqlite3"), false)?;
        db.execute(
            "INSERT OR IGNORE INTO hermes_profiles(server,profile) VALUES(?1,?2)",
            params![server, profile],
        )?;
        Ok(())
    }

    /// Check compatibility before migrations or saving any settings. A newer
    /// portable copy can share this directory with an older installed copy.
    fn preflight(root: &Path) -> Result<()> {
        let settings = root.join("settings.sqlite3");
        if settings.is_file() {
            let db = Self::read_only(&settings)?;
            let exists: bool = db.query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='settings')",
                [],
                |row| row.get(0),
            )?;
            if exists {
                let json: Option<String> = db
                    .query_row("SELECT value FROM settings WHERE id=1", [], |row| {
                        row.get(0)
                    })
                    .optional()?;
                if let Some(json) = json {
                    let settings: Settings = serde_json::from_str(&json)?;
                    settings.audio.validate()?;
                    if !(65..=160).contains(&settings.widget_scale) {
                        return Err(Error::Validation(
                            "La taille enregistrée du widget est invalide.".into(),
                        ));
                    }
                }
            }
        }
        let profiles = root.join("profiles");
        if profiles.is_dir() {
            for entry in fs::read_dir(profiles)? {
                let entry = entry?;
                if entry.file_type()?.is_dir()
                    && Uuid::parse_str(&entry.file_name().to_string_lossy()).is_ok()
                    && entry.path().join("profile.sqlite3").is_file()
                {
                    Self::read_only(&entry.path().join("profile.sqlite3"))?;
                }
            }
        }
        Ok(())
    }

    fn read_only(path: &Path) -> Result<Connection> {
        let db = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        db.busy_timeout(Duration::from_secs(3))?;
        let version: i64 = db.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(Error::FutureSchema);
        }
        Ok(db)
    }

    /// The recovery interface needs the saved UI language even when unknown
    /// fields prevent loading the rest of the configuration.
    pub fn saved_language(root: &Path) -> crate::i18n::Language {
        let read = || -> Option<crate::i18n::Language> {
            let db = Connection::open_with_flags(
                root.join("settings.sqlite3"),
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .ok()?;
            let json: String = db
                .query_row("SELECT value FROM settings WHERE id=1", [], |row| {
                    row.get(0)
                })
                .ok()?;
            let value: serde_json::Value = serde_json::from_str(&json).ok()?;
            serde_json::from_value(value.get("ui_language")?.clone()).ok()
        };
        read().unwrap_or_default()
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn settings(&self) -> Result<Settings> {
        let db = connect(&self.root.join("settings.sqlite3"), false)?;
        let json: String = db.query_row("SELECT value FROM settings WHERE id=1", [], |row| {
            row.get(0)
        })?;
        let settings: Settings = serde_json::from_str(&json)?;
        settings.audio.validate()?;
        if !(65..=160).contains(&settings.widget_scale) {
            return Err(Error::Validation(
                "La taille enregistrée du widget est invalide.".into(),
            ));
        }
        Ok(settings)
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        let db = connect(&self.root.join("settings.sqlite3"), false)?;
        db.execute(
            "UPDATE settings SET value=?1 WHERE id=1",
            [serde_json::to_string(settings)?],
        )?;
        Ok(())
    }

    fn profile_dir(&self, id: Uuid) -> PathBuf {
        self.root.join("profiles").join(id.to_string())
    }

    pub(crate) fn db(&self, id: Uuid) -> Result<Connection> {
        let path = self.profile_dir(id).join("profile.sqlite3");
        if !path.is_file() {
            return Err(Error::ProfileNotFound);
        }
        let mut db = connect(&path, false)?;
        let actual: String =
            db.query_row("SELECT id FROM profile WHERE singleton=1", [], |row| {
                row.get(0)
            })?;
        if actual != id.to_string() {
            return Err(Error::Validation(
                "L'identité de la base ne correspond pas à son dossier.".into(),
            ));
        }
        let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version < 2 {
            let tx = db.transaction()?;
            tx.execute_batch("ALTER TABLE messages ADD COLUMN heard_content TEXT NOT NULL DEFAULT ''; PRAGMA user_version = 2;")?;
            tx.commit()?;
        }
        if version < 3 {
            let tx = db.transaction()?;
            tx.execute_batch(include_str!("memory_schema.sql"))?;
            tx.commit()?;
        }
        if version < 4 {
            let tx = db.transaction()?;
            tx.execute_batch(
                "ALTER TABLE messages ADD COLUMN web_sources TEXT; PRAGMA user_version = 4;",
            )?;
            tx.commit()?;
        }
        if version < 5 {
            let tx = db.transaction()?;
            let json: String =
                tx.query_row("SELECT details FROM profile WHERE singleton=1", [], |row| {
                    row.get(0)
                })?;
            let mut details: serde_json::Value = serde_json::from_str(&json)?;
            // Upgrade the previously shipped default once. Other explicit
            // budgets and all identity, voice, history and memory data stay intact.
            if details
                .pointer("/connections/response_tokens")
                .and_then(|n| n.as_u64())
                == Some(1024)
            {
                details["connections"]["response_tokens"] =
                    crate::connections::DEFAULT_RESPONSE_TOKENS.into();
                tx.execute(
                    "UPDATE profile SET details=?1,revision=revision+1,updated_at=?2 WHERE singleton=1",
                    params![serde_json::to_string(&details)?, now()],
                )?;
            }
            tx.execute_batch("PRAGMA user_version = 5;")?;
            tx.commit()?;
        }
        if version < 6 {
            let tx = db.transaction()?;
            tx.execute_batch(include_str!("avatar_schema.sql"))?;
            let (json, has_avatar): (String, bool) = tx.query_row(
                "SELECT details,avatar IS NOT NULL FROM profile WHERE singleton=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            let mut draft: ProfileDraft = serde_json::from_str(&json)?;
            // Adopt the supplied pack only for an untouched Sherka portrait.
            // Custom portraits and every other character keep their own images.
            if draft.name == "Sherka" && !has_avatar && draft.avatar_preset.is_none() {
                draft.avatar_preset = Some(crate::avatar::AvatarPreset::Sherka);
                tx.execute("UPDATE profile SET details=?1,revision=revision+1,updated_at=?2 WHERE singleton=1", params![serde_json::to_string(&draft)?, now()])?;
            }
            tx.commit()?;
        }
        if version < 7 {
            db.execute_batch("PRAGMA user_version = 7;")?;
        }
        if version < 8 {
            db.execute_batch("PRAGMA user_version = 8;")?;
        }
        if version < 9 {
            db.execute_batch(include_str!("mood_schema.sql"))?;
        }
        Ok(db)
    }

    pub fn list(&self) -> Result<Vec<Profile>> {
        let mut profiles = Vec::new();
        for entry in fs::read_dir(self.root.join("profiles"))? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            if let Ok(id) = Uuid::parse_str(&entry.file_name().to_string_lossy()) {
                profiles.push(self.profile(id)?);
            }
        }
        profiles.sort_by(|a, b| {
            a.created_at
                .cmp(&b.created_at)
                .then(a.details.name.cmp(&b.details.name))
        });
        Ok(profiles)
    }

    pub fn profile(&self, id: Uuid) -> Result<Profile> {
        let db = self.db(id)?;
        let (revision, created_at, updated_at, json, avatar): (
            i64,
            i64,
            i64,
            String,
            Option<Vec<u8>>,
        ) = db.query_row(
            "SELECT revision,created_at,updated_at,details,avatar FROM profile WHERE singleton=1",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )?;
        let details = serde_json::from_str::<ProfileDraft>(&json)?.validated()?;
        let mut emotion_avatars: BTreeMap<_, _> = details
            .avatar_preset
            .map(|preset| {
                preset
                    .images()
                    .into_iter()
                    .map(|(emotion, path)| (emotion, path.to_owned()))
                    .collect()
            })
            .unwrap_or_default();
        for (emotion, bytes) in self.emotion_avatar_bytes(id)? {
            if let Some(bytes) = bytes {
                emotion_avatars.insert(
                    emotion,
                    format!("data:image/png;base64,{}", STANDARD.encode(bytes)),
                );
            } else {
                // An explicit reset suppresses a built-in slot as well.
                emotion_avatars.remove(&emotion);
            }
        }
        Ok(Profile {
            id,
            revision,
            created_at,
            updated_at,
            details,
            avatar: avatar.map(|bytes| format!("data:image/png;base64,{}", STANDARD.encode(bytes))),
            emotion_avatars,
            message_count: db.query_row("SELECT count(*) FROM messages", [], |r| r.get(0))?,
            memory_count: db.query_row(
                "SELECT count(*) FROM memories WHERE status='active' AND approved=1",
                [],
                |r| r.get(0),
            )?,
        })
    }

    pub fn create(&self, draft: ProfileDraft, avatar: Option<&[u8]>) -> Result<Profile> {
        self.create_with_avatars(draft, avatar, BTreeMap::new())
    }

    pub fn create_with_avatars(
        &self,
        draft: ProfileDraft,
        avatar: Option<&[u8]>,
        images: BTreeMap<crate::avatar::Emotion, Option<Vec<u8>>>,
    ) -> Result<Profile> {
        let draft = draft.validated()?;
        let avatar = avatar.map(normalize_image).transpose()?;
        let images = images
            .into_iter()
            .map(|(emotion, bytes)| {
                if emotion == crate::avatar::Emotion::Neutral {
                    return Err(Error::Validation("L'image du profil est invalide.".into()));
                }
                Ok((emotion, bytes.as_deref().map(normalize_image).transpose()?))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
        let id = Uuid::new_v4();
        // Unfinished creations never enter the profile list. The rename occurs
        // only after the transaction has committed and all handles are closed.
        let staging = self.root.join("profiles").join(format!(".creating-{id}"));
        fs::create_dir(&staging)?;
        let result = (|| -> Result<()> {
            let mut db = connect(&staging.join("profile.sqlite3"), true)?;
            let tx = db.transaction()?;
            tx.execute_batch(include_str!("schema.sql"))?;
            tx.execute_batch(include_str!("memory_schema.sql"))?;
            tx.execute_batch(
                "ALTER TABLE messages ADD COLUMN web_sources TEXT; PRAGMA user_version = 5;",
            )?;
            tx.execute("INSERT INTO profile(singleton,id,revision,created_at,updated_at,details,avatar) VALUES(1,?1,1,?2,?2,?3,?4)",
                params![id.to_string(), now(), serde_json::to_string(&draft)?, avatar])?;
            tx.execute_batch(include_str!("avatar_schema.sql"))?;
            for (emotion, bytes) in images {
                tx.execute(
                    "INSERT INTO avatar_emotions(emotion,image) VALUES(?1,?2)",
                    params![emotion.as_str(), bytes],
                )?;
            }
            tx.commit()?;
            db.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
            Ok(())
        })();
        if let Err(error) = result {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
        fs::rename(staging, self.profile_dir(id))?;
        self.profile(id)
    }

    pub fn update(&self, id: Uuid, revision: i64, draft: ProfileDraft) -> Result<Profile> {
        let draft = draft.validated()?;
        if self.profile(id)?.details.connections.language != draft.connections.language {
            return Err(Error::Validation("La langue du compagnon est fixée à sa création. Crée un autre compagnon pour utiliser une autre langue.".into()));
        }
        self.write_profile(id, revision, draft)
    }

    pub(crate) fn localize_default_companion(
        &self,
        id: Uuid,
        language: crate::i18n::Language,
    ) -> Result<Option<Profile>> {
        let previous = self.profile(id)?;
        if !crate::presets::is_builtin_sherka(&previous.details) {
            return Ok(None);
        }
        if previous.details.connections.language == language.code() {
            return Ok(Some(previous));
        }
        let character = crate::presets::sherka(language);
        let mut draft = previous.details;
        draft.personality = character.personality;
        draft.instructions = character.instructions;
        draft.connections.language = language.code().into();
        self.write_profile(id, previous.revision, draft).map(Some)
    }

    fn write_profile(&self, id: Uuid, revision: i64, draft: ProfileDraft) -> Result<Profile> {
        let db = self.db(id)?;
        let count = db.execute("UPDATE profile SET details=?1,revision=revision+1,updated_at=?2 WHERE singleton=1 AND revision=?3",
            params![serde_json::to_string(&draft)?, now(), revision])?;
        if count != 1 {
            return Err(Error::Conflict);
        }
        drop(db);
        self.profile(id)
    }

    pub fn set_avatar(&self, id: Uuid, revision: i64, bytes: Option<&[u8]>) -> Result<Profile> {
        self.set_emotion_avatar(id, revision, crate::avatar::Emotion::Neutral, bytes)
    }

    pub fn set_emotion_avatar(
        &self,
        id: Uuid,
        revision: i64,
        emotion: crate::avatar::Emotion,
        bytes: Option<&[u8]>,
    ) -> Result<Profile> {
        let png = bytes.map(normalize_image).transpose()?;
        let mut db = self.db(id)?;
        let tx = db.transaction()?;
        if tx.execute("UPDATE profile SET revision=revision+1,updated_at=?1 WHERE singleton=1 AND revision=?2", params![now(), revision])? != 1 {
            return Err(Error::Conflict);
        }
        if emotion == crate::avatar::Emotion::Neutral {
            tx.execute(
                "UPDATE profile SET avatar=?1 WHERE singleton=1",
                params![png],
            )?;
        } else {
            tx.execute("INSERT INTO avatar_emotions(emotion,image) VALUES(?1,?2) ON CONFLICT(emotion) DO UPDATE SET image=excluded.image", params![emotion.as_str(), png])?;
        }
        tx.commit()?;
        drop(db);
        self.profile(id)
    }

    pub fn duplicate(&self, id: Uuid) -> Result<Profile> {
        let source = self.profile(id)?;
        let mut details = source.details;
        details.name = format!(
            "{} · {}",
            details.name.chars().take(40).collect::<String>(),
            crate::i18n::text("copie")
        );
        let avatar = self.avatar_bytes(id)?;
        self.create_with_avatars(details, avatar.as_deref(), self.emotion_avatar_bytes(id)?)
    }

    pub fn delete(&self, id: Uuid) -> Result<()> {
        self.profile(id)?;
        if self.list()?.len() <= 1 {
            return Err(Error::LastProfile);
        }
        if self.settings()?.active_profile == Some(id) {
            return Err(Error::Validation(
                "Active un autre compagnon avant de supprimer celui-ci.".into(),
            ));
        }
        // IDs are UUIDs, never arbitrary frontend paths. Renaming first makes
        // the deletion immediately absent from discovery, even after a crash.
        let tombstone = self.root.join("profiles").join(format!(".deleted-{id}"));
        fs::rename(self.profile_dir(id), &tombstone)?;
        fs::remove_dir_all(tombstone)?;
        Ok(())
    }

    fn avatar_bytes(&self, id: Uuid) -> Result<Option<Vec<u8>>> {
        Ok(self
            .db(id)?
            .query_row("SELECT avatar FROM profile WHERE singleton=1", [], |row| {
                row.get(0)
            })?)
    }

    fn emotion_avatar_bytes(
        &self,
        id: Uuid,
    ) -> Result<BTreeMap<crate::avatar::Emotion, Option<Vec<u8>>>> {
        let db = self.db(id)?;
        let mut query = db.prepare("SELECT emotion,image FROM avatar_emotions")?;
        let rows = query.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<Vec<u8>>>(1)?))
        })?;
        rows.map(|row| {
            let (emotion, bytes) = row?;
            Ok((
                serde_json::from_value(serde_json::Value::String(emotion))?,
                bytes,
            ))
        })
        .collect()
    }

    pub fn export_profile(&self, id: Uuid) -> Result<String> {
        let profile = self.profile(id)?;
        let export = ProfileExport {
            format: "sherka-profile".into(),
            version: 5,
            profile: profile.details.for_export(),
            avatar_png_base64: self.avatar_bytes(id)?.map(|bytes| STANDARD.encode(bytes)),
            emotion_avatars_png_base64: self
                .emotion_avatar_bytes(id)?
                .into_iter()
                .map(|(emotion, bytes)| (emotion, bytes.map(|b| STANDARD.encode(b))))
                .collect(),
        };
        Ok(serde_json::to_string_pretty(&export)?)
    }

    pub fn import_profile(&self, json: &str) -> Result<Profile> {
        if json.len() > MAX_EXPORT_BYTES {
            return Err(Error::Validation(
                "Ce fichier dépasse la limite de 48 Mio.".into(),
            ));
        }
        let export: ProfileExport = serde_json::from_str(json)?;
        if export.format != "sherka-profile"
            || ![1, 2, 3, 4, 5].contains(&export.version)
            || (export.version == 1 && !export.emotion_avatars_png_base64.is_empty())
        {
            return Err(Error::Validation(
                "Le format ou la version de ce profil n'est pas pris en charge.".into(),
            ));
        }
        let avatar = export
            .avatar_png_base64
            .map(|s| {
                STANDARD
                    .decode(s)
                    .map_err(|_| Error::Validation("L'image du profil est invalide.".into()))
            })
            .transpose()?;
        // A fresh identity and empty database are always allocated. This format
        // contains no API key, external asset path, history or memory.
        let images = export
            .emotion_avatars_png_base64
            .into_iter()
            .map(|(emotion, encoded)| {
                encoded
                    .map(|s| {
                        STANDARD.decode(s).map_err(|_| {
                            Error::Validation("L'image du profil est invalide.".into())
                        })
                    })
                    .transpose()
                    .map(|bytes| (emotion, bytes))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
        self.create_with_avatars(export.profile, avatar.as_deref(), images)
    }

    pub fn append_message(
        &self,
        tag: OperationTag,
        role: MessageRole,
        content: &str,
        delivery: Delivery,
    ) -> Result<()> {
        if content.trim().is_empty() || content.len() > 256_000 {
            return Err(Error::Validation(
                "Le message est vide ou trop long.".into(),
            ));
        }
        let db = self.db(tag.profile_id)?;
        db.execute("INSERT INTO messages(id,session_id,turn_id,generation_id,role,content,delivery,created_at,extraction_allowed) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![Uuid::new_v4().to_string(), self.transcript_session(tag.profile_id)?.unwrap_or_else(||tag.session_id.to_string()), tag.turn_id.to_string(), tag.generation_id.to_string(), serde_json::to_string(&role)?, content, serde_json::to_string(&delivery)?, now(), self.transcript_session(tag.profile_id)?.is_none()])?;
        Ok(())
    }

    pub fn history(&self, id: Uuid, limit: u32) -> Result<Vec<Message>> {
        let db = self.db(id)?;
        let mut query = db.prepare("SELECT id,role,content,delivery,created_at,heard_content,web_sources FROM (SELECT seq,id,role,content,delivery,created_at,heard_content,web_sources FROM messages WHERE (?2 IS NULL OR session_id=?2) ORDER BY seq DESC LIMIT ?1) ORDER BY seq")?;
        let raw = query.query_map(params![limit.min(100), self.transcript_session(id)?], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, Option<String>>(6)?,
            ))
        })?;
        raw.map(|row| {
            let (id, role, content, delivery, created_at, heard_content, web) = row?;
            Ok(Message {
                web: web.map(|text| serde_json::from_str(&text)).transpose()?,
                id,
                role: serde_json::from_str(&role)?,
                content,
                delivery: serde_json::from_str(&delivery)?,
                created_at,
                heard_content,
            })
        })
        .collect()
    }

    pub fn save_response(
        &self,
        tag: OperationTag,
        content: &str,
        heard: &str,
        delivery: Delivery,
    ) -> Result<()> {
        if content.trim().is_empty() {
            return Ok(());
        }
        if content.len() > 512_000 || heard.len() > 512_000 {
            return Err(Error::Validation("Réponse trop longue.".into()));
        }
        self.db(tag.profile_id)?.execute(
            "INSERT INTO messages(id,session_id,turn_id,generation_id,role,content,delivery,created_at,heard_content,extraction_allowed) VALUES(?1,?2,?3,?1,?4,?5,?6,?7,?8,?9) ON CONFLICT(id) DO UPDATE SET content=excluded.content,delivery=excluded.delivery,heard_content=excluded.heard_content",
            params![tag.generation_id.to_string(), self.transcript_session(tag.profile_id)?.unwrap_or_else(||tag.session_id.to_string()), tag.turn_id.to_string(), serde_json::to_string(&MessageRole::Assistant)?,content,serde_json::to_string(&delivery)?,now(),heard,self.transcript_session(tag.profile_id)?.is_none()])?;
        Ok(())
    }

    pub fn save_web_sources(&self, tag: OperationTag, report: &crate::web::Report) -> Result<()> {
        self.db(tag.profile_id)?.execute(
            "UPDATE messages SET web_sources=?1 WHERE id=?2",
            params![
                serde_json::to_string(report)?,
                tag.generation_id.to_string()
            ],
        )?;
        Ok(())
    }

    pub fn memory_count_for(&self, id: Uuid) -> Result<i64> {
        Ok(self
            .db(id)?
            .query_row(
                "SELECT count(*) FROM memories WHERE status='active' AND approved=1",
                [],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0))
    }
}

pub fn normalize_image(bytes: &[u8]) -> Result<Vec<u8>> {
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(Error::Validation(
            "L'image dépasse la limite de 10 Mio.".into(),
        ));
    }
    let format = image::guess_format(bytes)?;
    if !matches!(format, ImageFormat::Png | ImageFormat::WebP) {
        return Err(Error::Validation("Choisis une image PNG ou WebP.".into()));
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(80 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader.decode()?;
    let normalized = decoded.thumbnail(1024, 1024).to_rgba8();
    let mut output = Cursor::new(Vec::new());
    normalized.write_to(&mut output, ImageFormat::Png)?;
    Ok(output.into_inner())
}

pub(crate) fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
