use sherka_core::{
    Companion,
    avatar::Emotion,
    companion_archive::EMOTIONS,
    memory::MemoryDraft,
    storage::{Delivery, MessageRole},
};
use std::{
    collections::BTreeMap,
    io::{Cursor, Read, Write},
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

fn png(color: u8) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(4, 4, image::Rgba([color, 50, 90, 190]))
        .write_to(&mut out, image::ImageFormat::Png)
        .unwrap();
    out.into_inner()
}
fn files(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut zip = ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..zip.len())
        .map(|i| {
            let mut file = zip.by_index(i).unwrap();
            let name = file.name().to_owned();
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).unwrap();
            (name, bytes)
        })
        .collect()
}
fn archive(files: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in files {
        zip.start_file(
            name,
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated),
        )
        .unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[test]
fn zip_roundtrip_preserves_appearance_and_settings_without_private_state() {
    let root = tempfile::tempdir().unwrap();
    let mut app = Companion::open(root.path()).unwrap();
    let id = app.session.snapshot().profile_id;
    let mut p = app.store.profile(id).unwrap();
    p.details.connections.tts_voice = Some("voice-reference".into());
    p.details.connections.voice_studio_url = "http://private-voice-server:3900".into();
    p.details.connections.stt_model = Some("device-transcription".into());
    p.details.illustration.positive_prompt_suffix = "(golden light:1.3)".into();
    app.update_profile(id, p.revision, p.details).unwrap();
    p = app.store.profile(id).unwrap();
    p = app
        .store
        .set_emotion_avatar(id, p.revision, Emotion::Joy, Some(&png(250)))
        .unwrap();
    p = app
        .store
        .set_emotion_avatar(id, p.revision, Emotion::Sadness, None)
        .unwrap();
    let tag = app.session.begin_text_turn();
    app.record_message(
        tag,
        MessageRole::User,
        "PRIVATE_CONVERSATION",
        Delivery::TextOnly,
    )
    .unwrap();
    app.store
        .write_memory(
            &app.store.memory_guard(id).unwrap(),
            None,
            &MemoryDraft {
                content: "PRIVATE_SAVED_FACT".into(),
                category: "personal".into(),
                kind: "essential".into(),
            },
        )
        .unwrap();
    let copy = app.store.duplicate(id).unwrap();
    assert_eq!(copy.details.connections, p.details.connections);
    assert_eq!(copy.details.illustration, p.details.illustration);
    assert_eq!(copy.emotion_avatars, p.emotion_avatars);
    assert_eq!((copy.message_count, copy.memory_count), (0, 0));

    let bytes = app
        .store
        .export_companion_archive(id, |_| Some(png(10)))
        .unwrap();
    let packed = files(&bytes);
    assert_eq!(packed.len(), 8);
    assert_eq!(packed["images/sadness.png"], packed["images/neutral.png"]);
    assert_ne!(packed["images/joy.png"], packed["images/neutral.png"]);
    let json = String::from_utf8(packed["companion.json"].clone()).unwrap();
    for forbidden in [
        "private-voice-server",
        "device-transcription",
        "PRIVATE_CONVERSATION",
        "PRIVATE_SAVED_FACT",
        "api_key",
        "session_id",
        "profile_id",
    ] {
        assert!(!json.contains(forbidden));
    }
    let imported = app
        .store
        .import_companion_archive(&archive(&packed))
        .unwrap();
    assert_ne!(imported.id, id);
    assert_eq!(app.store.settings().unwrap().active_profile, Some(id));
    assert_eq!((imported.message_count, imported.memory_count), (0, 0));
    assert_eq!(
        imported.details.connections,
        p.details.clone().for_export().connections
    );
    assert_eq!(imported.details.illustration, p.details.illustration);
    assert_eq!(imported.details.personality, p.details.personality);
    assert!(imported.details.avatar_preset.is_none());
    assert_eq!(imported.emotion_avatars.len(), 6);
    assert!(
        imported
            .emotion_avatars
            .values()
            .all(|v| v.starts_with("data:image/png;base64,"))
    );
    let again = app
        .store
        .export_companion_archive(imported.id, |_| panic!("must be self-contained"))
        .unwrap();
    assert_eq!(files(&again), packed);
    assert_eq!(app.store.profile(id).unwrap().message_count, 1);
    assert_eq!(app.store.profile(id).unwrap().memory_count, 1);
}

#[test]
fn malformed_archives_cannot_write_files_or_allocate_profiles() {
    let root = tempfile::tempdir().unwrap();
    let app = Companion::open(root.path()).unwrap();
    let id = app.session.snapshot().profile_id;
    let base = files(
        &app.store
            .export_companion_archive(id, |_| Some(png(1)))
            .unwrap(),
    );
    let before = app.store.list().unwrap().len();
    for case in 0..8 {
        let mut entries = base.clone();
        match case {
            0 => {
                entries.remove("images/joy.png");
                entries.insert("../escape.png".into(), png(1));
            }
            1 => {
                entries.remove("images/joy.png");
            }
            2 => {
                entries.insert("images/joy.png".into(), b"not a PNG".to_vec());
            }
            3 => {
                entries.insert("images/joy.png".into(), vec![0; 10 * 1024 * 1024 + 1]);
            }
            _ => {
                let mut m: serde_json::Value =
                    serde_json::from_slice(&entries["companion.json"]).unwrap();
                match case {
                    4 => m["version"] = 999.into(),
                    5 => m["images"]["joy"] = "../../elsewhere.png".into(),
                    6 => m["profile"]["connections"]["api_key"] = "secret".into(),
                    _ => {
                        m["profile"]["illustration"]["workflow"] =
                            serde_json::json!({"1":{"class_type":"ExecutePython","inputs":{}}})
                    }
                }
                entries.insert("companion.json".into(), serde_json::to_vec(&m).unwrap());
            }
        }
        assert!(
            app.store
                .import_companion_archive(&archive(&entries))
                .is_err(),
            "case {case}"
        );
        assert_eq!(app.store.list().unwrap().len(), before);
    }
    for emotion in EMOTIONS {
        assert!(base.contains_key(&format!("images/{}.png", emotion.as_str())));
    }
    assert!(!root.path().join("escape.png").exists());
    assert!(app.store.import_companion_archive(b"not a ZIP").is_err());
}
