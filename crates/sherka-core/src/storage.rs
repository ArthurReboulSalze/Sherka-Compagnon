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
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

const SCHEMA_VERSION: i64 = 4;
const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;
const MAX_EXPORT_BYTES: usize = 15 * 1024 * 1024;

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
        fs::create_dir_all(store.root.join("profiles"))?;
        let mut db = connect(&store.root.join("settings.sqlite3"), true)?;
        let tx = db.transaction()?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS settings (id INTEGER PRIMARY KEY CHECK(id=1), value TEXT NOT NULL); PRAGMA user_version = 1;")?;
        tx.execute(
            "INSERT OR IGNORE INTO settings(id,value) VALUES(1,?1)",
            [serde_json::to_string(&Settings::default())?],
        )?;
        tx.commit()?;
        Ok(store)
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
        Ok(Profile {
            id,
            revision,
            created_at,
            updated_at,
            details: serde_json::from_str::<ProfileDraft>(&json)?.validated()?,
            avatar: avatar.map(|bytes| format!("data:image/png;base64,{}", STANDARD.encode(bytes))),
            message_count: db.query_row("SELECT count(*) FROM messages", [], |r| r.get(0))?,
            memory_count: db.query_row(
                "SELECT count(*) FROM memories WHERE status='active' AND approved=1",
                [],
                |r| r.get(0),
            )?,
        })
    }

    pub fn create(&self, draft: ProfileDraft, avatar: Option<&[u8]>) -> Result<Profile> {
        let draft = draft.validated()?;
        let avatar = avatar.map(normalize_image).transpose()?;
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
            tx.execute("INSERT INTO profile(singleton,id,revision,created_at,updated_at,details,avatar) VALUES(1,?1,1,?2,?2,?3,?4)",
                params![id.to_string(), now(), serde_json::to_string(&draft)?, avatar])?;
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
        let png = bytes.map(normalize_image).transpose()?;
        let db = self.db(id)?;
        if db.execute("UPDATE profile SET avatar=?1,revision=revision+1,updated_at=?2 WHERE singleton=1 AND revision=?3", params![png, now(), revision])? != 1 {
            return Err(Error::Conflict);
        }
        drop(db);
        self.profile(id)
    }

    pub fn duplicate(&self, id: Uuid) -> Result<Profile> {
        let source = self.profile(id)?;
        let mut details = source.details;
        details.name = format!(
            "{} · copie",
            details.name.chars().take(50).collect::<String>()
        );
        let avatar = self.avatar_bytes(id)?;
        self.create(details, avatar.as_deref())
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

    pub fn export_profile(&self, id: Uuid) -> Result<String> {
        let profile = self.profile(id)?;
        let export = ProfileExport {
            format: "sherka-profile".into(),
            version: 1,
            profile: profile.details,
            avatar_png_base64: self.avatar_bytes(id)?.map(|bytes| STANDARD.encode(bytes)),
        };
        Ok(serde_json::to_string_pretty(&export)?)
    }

    pub fn import_profile(&self, json: &str) -> Result<Profile> {
        if json.len() > MAX_EXPORT_BYTES {
            return Err(Error::Validation(
                "Ce fichier dépasse la limite de 15 Mio.".into(),
            ));
        }
        let export: ProfileExport = serde_json::from_str(json)?;
        if export.format != "sherka-profile" || export.version != 1 {
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
        self.create(export.profile, avatar.as_deref())
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
        db.execute("INSERT INTO messages(id,session_id,turn_id,generation_id,role,content,delivery,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![Uuid::new_v4().to_string(), tag.session_id.to_string(), tag.turn_id.to_string(), tag.generation_id.to_string(), serde_json::to_string(&role)?, content, serde_json::to_string(&delivery)?, now()])?;
        Ok(())
    }

    pub fn history(&self, id: Uuid, limit: u32) -> Result<Vec<Message>> {
        let db = self.db(id)?;
        let mut query = db.prepare("SELECT id,role,content,delivery,created_at,heard_content,web_sources FROM (SELECT seq,id,role,content,delivery,created_at,heard_content,web_sources FROM messages ORDER BY seq DESC LIMIT ?1) ORDER BY seq")?;
        let raw = query.query_map([limit.min(100)], |r| {
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
        if content.len() > 64_000 || heard.len() > 64_000 {
            return Err(Error::Validation("Réponse trop longue.".into()));
        }
        self.db(tag.profile_id)?.execute(
            "INSERT INTO messages(id,session_id,turn_id,generation_id,role,content,delivery,created_at,heard_content) VALUES(?1,?2,?3,?1,?4,?5,?6,?7,?8) ON CONFLICT(id) DO UPDATE SET content=excluded.content,delivery=excluded.delivery,heard_content=excluded.heard_content",
            params![tag.generation_id.to_string(), tag.session_id.to_string(), tag.turn_id.to_string(), serde_json::to_string(&MessageRole::Assistant)?,content,serde_json::to_string(&delivery)?,now(),heard])?;
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
