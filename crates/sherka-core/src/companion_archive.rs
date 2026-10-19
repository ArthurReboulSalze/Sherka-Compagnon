//! Sherka Companion Archive v1: a manifest and seven self-contained PNGs.
//! Read in memory; archive paths are never extracted to the filesystem.
use crate::{
    Error, Result,
    avatar::Emotion,
    model::{Profile, ProfileDraft},
    storage::Store,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::{Cursor, Read, Write},
};
use uuid::Uuid;
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

pub const MAX_ARCHIVE_BYTES: usize = 48 * 1024 * 1024;
const MAX_MANIFEST: u64 = 512 * 1024;
const MAX_IMAGE: u64 = 10 * 1024 * 1024;
pub const EMOTIONS: [Emotion; 7] = [
    Emotion::Neutral,
    Emotion::Joy,
    Emotion::Sadness,
    Emotion::Anger,
    Emotion::Surprise,
    Emotion::Reflection,
    Emotion::Tenderness,
];

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format: String,
    pub version: u32,
    pub profile: ProfileDraft,
    pub images: BTreeMap<Emotion, String>,
}

fn invalid() -> Error {
    Error::Validation("Archive de compagnon invalide ou version non prise en charge.".into())
}
fn image_name(emotion: Emotion) -> String {
    format!("images/{}.png", emotion.as_str())
}

impl Store {
    /// Resolve only built-in application assets, never arbitrary paths from a pack.
    pub fn export_companion_archive(
        &self,
        id: Uuid,
        resolve: impl Fn(&str) -> Option<Vec<u8>>,
    ) -> Result<Vec<u8>> {
        let source = self.profile(id)?;
        let image_bytes = |value: &str| -> Result<Vec<u8>> {
            if let Some(encoded) = value.strip_prefix("data:image/png;base64,") {
                STANDARD.decode(encoded).map_err(|_| invalid())
            } else {
                resolve(value).ok_or_else(invalid)
            }
        };
        let neutral = source.avatar.as_deref().unwrap_or_else(|| {
            source
                .details
                .avatar_preset
                .map(|p| p.neutral())
                .unwrap_or("/assets/sherka.png")
        });
        let neutral_bytes = image_bytes(neutral)?;
        let mut profile = source.details.for_export();
        // Every visible slot is embedded. The receiving installation needs no preset.
        profile.avatar_preset = None;
        let manifest = Manifest {
            format: "sherka-companion".into(),
            version: 1,
            profile,
            images: EMOTIONS.into_iter().map(|e| (e, image_name(e))).collect(),
        };
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        zip.start_file("companion.json", options)
            .map_err(|_| invalid())?;
        let metadata = serde_json::to_vec_pretty(&manifest)?;
        if metadata.len() as u64 > MAX_MANIFEST {
            return Err(invalid());
        }
        zip.write_all(&metadata)?;
        let mut total = metadata.len();
        for emotion in EMOTIONS {
            let bytes = match source.emotion_avatars.get(&emotion) {
                Some(value) if emotion != Emotion::Neutral => image_bytes(value)?,
                _ => neutral_bytes.clone(),
            };
            // Validate/canonicalize built-ins too, including their decoded dimensions.
            let bytes = crate::storage::normalize_image(&bytes)?;
            total += bytes.len();
            if total > MAX_ARCHIVE_BYTES {
                return Err(invalid());
            }
            zip.start_file(image_name(emotion), options)
                .map_err(|_| invalid())?;
            zip.write_all(&bytes)?;
        }
        let bytes = zip.finish().map_err(|_| invalid())?.into_inner();
        if bytes.len() > MAX_ARCHIVE_BYTES {
            return Err(invalid());
        }
        Ok(bytes)
    }

    pub fn import_companion_archive(&self, bytes: &[u8]) -> Result<Profile> {
        if bytes.len() > MAX_ARCHIVE_BYTES {
            return Err(invalid());
        }
        let mut zip = ZipArchive::new(Cursor::new(bytes)).map_err(|_| invalid())?;
        if zip.len() != 8 {
            return Err(invalid());
        }
        let mut files = BTreeMap::new();
        let mut total = 0usize;
        for i in 0..zip.len() {
            let file = zip.by_index(i).map_err(|_| invalid())?;
            let name = file.name().to_owned();
            let limit = if name == "companion.json" {
                MAX_MANIFEST
            } else if EMOTIONS.into_iter().any(|e| image_name(e) == name) {
                MAX_IMAGE
            } else {
                return Err(invalid());
            };
            if file.encrypted()
                || file.is_symlink()
                || file.is_dir()
                || file.size() > limit
                || files.contains_key(&name)
            {
                return Err(invalid());
            }
            let mut content = Vec::new();
            file.take(limit.min((MAX_ARCHIVE_BYTES - total) as u64) + 1)
                .read_to_end(&mut content)
                .map_err(|_| invalid())?;
            total += content.len();
            if content.len() as u64 > limit || total > MAX_ARCHIVE_BYTES {
                return Err(invalid());
            }
            files.insert(name, content);
        }
        let mut manifest: Manifest =
            serde_json::from_slice(files.get("companion.json").ok_or_else(invalid)?)
                .map_err(|_| invalid())?;
        if manifest.format != "sherka-companion"
            || manifest.version != 1
            || manifest.images.len() != 7
        {
            return Err(invalid());
        }
        let mut images = BTreeMap::new();
        for emotion in EMOTIONS {
            let name = image_name(emotion);
            if manifest.images.get(&emotion) != Some(&name) {
                return Err(invalid());
            }
            let bytes = files.remove(&name).ok_or_else(invalid)?;
            // The v1 archive requires PNG, unlike the PNG/WebP image chooser.
            if image::guess_format(&bytes).ok() != Some(image::ImageFormat::Png) {
                return Err(invalid());
            }
            images.insert(emotion, Some(bytes));
        }
        manifest.profile.avatar_preset = None;
        let neutral = images
            .remove(&Emotion::Neutral)
            .flatten()
            .ok_or_else(invalid)?;
        // Creation validates everything before allocating a fresh, empty profile.
        // Import never activates it or runs its workflows/instructions.
        self.create_with_avatars(manifest.profile, Some(&neutral), images)
    }
}
