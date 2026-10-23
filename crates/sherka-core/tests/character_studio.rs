use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use sherka_core::{
    Companion,
    character_studio::{Draft, EMOTIONS},
    i18n::Language,
    illustration::{self, Config, Scene},
    image_pipeline::{self, Reference, Resolution},
    presets,
};
use std::collections::BTreeMap;

fn config() -> Config {
    Config {
        checkpoint: Some("fixture.safetensors".into()),
        turbo: false,
        resolution: Some(Resolution {
            width: 768,
            height: 1024,
            latent_scale: 1.5,
            upscale_model: Some("2x.pth".into()),
        }),
        reference: Reference {
            enabled: true,
            model: Some("ip-adapter_sdxl.safetensors".into()),
            clip_vision: Some("bigG.safetensors".into()),
            weight: 0.7,
        },
        physical_description: "Silver mechanical robot".into(),
        appearance_negative_prompt: "human face, hair".into(),
        ..Default::default()
    }
}
fn info() -> Value {
    let mut out = json!({});
    for node in illustration::default_workflow()
        .as_object()
        .unwrap()
        .values()
    {
        out[node["class_type"].as_str().unwrap()] = json!({});
    }
    for class in image_pipeline::NODE_CLASSES {
        out[*class] = json!({});
    }
    out["CheckpointLoaderSimple"] =
        json!({"input":{"required":{"ckpt_name":[["fixture.safetensors"]]}}});
    out["LoraLoader"] = json!({"input":{"required":{"lora_name":[[]]}}});
    out["IPAdapterModelLoader"] =
        json!({"input":{"required":{"ipadapter_file":[["ip-adapter_sdxl.safetensors"]]}}});
    out["CLIPVisionLoader"] = json!({"input":{"required":{"clip_name":[["bigG.safetensors"]]}}});
    out["UpscaleModelLoader"] =
        json!({"input":{"required":{"model_name":["COMBO",{"options":["2x.pth"]}]}}});
    out
}
fn scene() -> Scene {
    Scene {
        positive: "A silver mechanical robot in a garden".into(),
        negative: "ignored automatic negative".into(),
    }
}
fn draft() -> Draft {
    Draft {
        name: "Vega".into(),
        personality: "Curieuse et calme.".into(),
        instructions: "Réponds naturellement.".into(),
        accent: "teal".into(),
        language: Language::Fr,
        physical_description: "Silver mechanical robot, green optical eyes".into(),
        appearance_negative_prompt: "human face, hair".into(),
        portrait_prompt: "full isolated bust, empty surroundings".into(),
        emotion_prompts: EMOTIONS[1..]
            .iter()
            .map(|e| {
                (
                    *e,
                    format!("{} expressed through mechanical eye shapes", e.as_str()),
                )
            })
            .collect(),
        prompt_overrides: Default::default(),
    }
}
fn png(transparent: bool) -> Vec<u8> {
    let mut pixels = image::RgbaImage::from_pixel(64, 64, image::Rgba([100, 180, 220, 255]));
    if transparent {
        for y in 0..32 {
            for x in 0..64 {
                pixels.put_pixel(x, y, image::Rgba([0, 0, 0, 0]));
            }
        }
    }
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(pixels)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    bytes.into_inner()
}
#[test]
fn studio_portraits_keep_close_framing_through_both_samplers_for_every_emotion() {
    for family in [
        illustration::ModelFamily::Sdxl,
        illustration::ModelFamily::Illustrious,
    ] {
        let mut c = config();
        c.model_family = family;
        let mut d = draft();
        // A previously generated sheet must also benefit when retried.
        d.portrait_prompt =
            "Full_body, entire silhouette inside the frame, side profile, three-quarter view, (red armor:1.2)".into();
        let original = d.clone();
        for emotion in EMOTIONS {
            let scene = d.scene(&c, emotion).unwrap();
            let graph = image_pipeline::workflow(
                &c,
                scene,
                &info(),
                42,
                true,
                Some("%reference_image%"),
                true,
            )
            .unwrap();
            let positive = graph["4"]["inputs"]["text"].as_str().unwrap();
            assert!(positive.contains("close-up"));
            assert!(positive.contains("bust portrait"));
            assert!(positive.contains("front view"));
            assert!(positive.contains("entire head"));
            assert!(!positive.to_lowercase().contains("full_body"));
            assert!(!positive.contains("entire silhouette"));
            assert!(!positive.contains("side profile"));
            assert!(!positive.contains("three-quarter view"));
            for detail in [
                "Silver mechanical robot",
                "green optical eyes",
                "(red armor:1.2)",
            ] {
                assert!(positive.contains(detail), "{emotion:?}: lost {detail}");
            }
            if emotion != sherka_core::avatar::Emotion::Neutral {
                assert!(positive.contains(&d.emotion_prompts[&emotion]));
            }
            assert_eq!(
                graph["21"]["inputs"]["positive"],
                graph["25"]["inputs"]["positive"]
            );
            let negative = graph["5"]["inputs"]["text"].as_str().unwrap();
            assert!(negative.starts_with(&c.scene_negative(true)));
            for excluded in [
                "full body",
                "cropped head",
                "side view",
                "three-quarter view",
            ] {
                assert!(negative.contains(excluded));
            }
            assert_eq!(
                graph["21"]["inputs"]["negative"],
                graph["25"]["inputs"]["negative"]
            );
        }
        assert_eq!(d, original);
        // Studio framing must not leak into a created companion's normal scenes.
        let profile = d
            .profile(&presets::sherka(Language::Fr), c.clone())
            .unwrap();
        let graph = image_pipeline::workflow(
            &profile.illustration,
            scene(),
            &info(),
            42,
            true,
            Some("%reference_image%"),
            false,
        )
        .unwrap();
        assert_eq!(graph["4"]["inputs"]["text"], scene().positive);
        assert_eq!(graph["5"]["inputs"]["text"], c.scene_negative(true));
        assert_eq!(profile.illustration.negative_prompt, c.negative_prompt);
    }
}

#[test]
fn studio_allows_head_only_portraits_and_keeps_framing_with_long_descriptions() {
    for family in [
        illustration::ModelFamily::Sdxl,
        illustration::ModelFamily::Illustrious,
    ] {
        let c = Config {
            model_family: family,
            ..config()
        };
        let mut d = draft();
        d.portrait_prompt = "HEAD-ONLY, three-quarter view".into();
        for emotion in EMOTIONS {
            let scene = d.scene(&c, emotion).unwrap().validated_for(family).unwrap();
            assert!(scene.positive.contains("close-up"));
            assert!(!scene.positive.contains("bust"));
            assert!(!scene.positive.contains("shoulders"));
            assert!(scene.positive.contains("front view"));
            assert!(!scene.positive.contains("three-quarter view"));
        }
        d.physical_description = "silver metal armor, ".repeat(100);
        let scene = d
            .scene(&c, EMOTIONS[0])
            .unwrap()
            .validated_for(family)
            .unwrap();
        assert!(scene.positive.contains("close-up"));
        assert!(scene.positive.contains("head"));
    }
}

#[test]
fn reference_is_conditional_and_negative_reaches_both_samplers() {
    for family in [
        illustration::ModelFamily::Sdxl,
        illustration::ModelFamily::Illustrious,
    ] {
        let mut c = config();
        c.model_family = family;
        for include in [false, true] {
            let g = image_pipeline::workflow(
                &c,
                scene(),
                &info(),
                1,
                include,
                Some("%reference_image%"),
                false,
            )
            .unwrap();
            assert_eq!(g.get("sherka_ip").is_some(), include);
            assert_eq!(g["18"]["inputs"]["width"], 768);
            assert_eq!(g["18"]["inputs"]["height"], 1024);
            assert_eq!(
                g["25"]["inputs"]["latent_image"],
                json!(["sherka_latent", 0])
            );
            assert_eq!(g["sherka_output_size"]["inputs"]["width"], 2304);
            assert_eq!(g["sherka_output_size"]["inputs"]["height"], 3072);
            assert_eq!(g["21"]["inputs"]["negative"], g["25"]["inputs"]["negative"]);
            assert_eq!(
                g["5"]["inputs"]["text"]
                    .as_str()
                    .unwrap()
                    .contains("human face"),
                include
            );
        }
    }
}
#[test]
fn disabling_reference_removes_conditioning_even_from_a_previously_exported_workflow() {
    let mut c = config();
    c.workflow = Some(image_pipeline::template(&c, false).unwrap());
    c.reference.enabled = false;
    // The option must also work without IP-Adapter installed on the server.
    let mut catalog = info();
    for class in [
        "IPAdapterModelLoader",
        "CLIPVisionLoader",
        "IPAdapterAdvanced",
        "LoadImage",
    ] {
        catalog.as_object_mut().unwrap().remove(class);
    }
    let graph = image_pipeline::workflow(&c, scene(), &catalog, 7, true, None, false).unwrap();
    assert!(!graph.as_object().unwrap().values().any(|n| {
        n["class_type"].as_str().is_some_and(|s| {
            s.starts_with("IPAdapter") || ["CLIPVisionLoader", "LoadImage"].contains(&s)
        })
    }));
    assert_ne!(graph["21"]["inputs"]["model"], json!(["sherka_ip", 0]));
    assert_eq!(
        graph["21"]["inputs"]["model"],
        graph["25"]["inputs"]["model"]
    );
    assert!(
        graph["5"]["inputs"]["text"]
            .as_str()
            .unwrap()
            .contains("human face")
    );
}
#[test]
fn transparent_upscale_keeps_alpha_and_export_round_trips_without_duplicate_nodes() {
    let c = config();
    let original = image_pipeline::workflow(
        &c,
        scene(),
        &info(),
        7,
        true,
        Some("%reference_image%"),
        true,
    )
    .unwrap();
    assert_eq!(
        original["sherka_rgba"]["inputs"]["samples"],
        json!(["25", 0])
    );
    assert_eq!(
        original["sherka_split"]["inputs"]["image"],
        json!(["sherka_rgba", 0])
    );
    assert_eq!(
        original["33"]["inputs"]["images"],
        json!(["sherka_join", 0])
    );
    let exported = image_pipeline::template(&c, true).unwrap();
    let imported = Config {
        workflow: Some(exported),
        ..c
    };
    let again = image_pipeline::workflow(
        &imported,
        scene(),
        &info(),
        7,
        true,
        Some("%reference_image%"),
        true,
    )
    .unwrap();
    assert_eq!(again, original);
}
#[test]
fn invalid_capabilities_dimensions_and_reference_paths_fail_before_submission() {
    let c = config();
    for missing in [
        "IPAdapterAdvanced",
        "LayeredDiffusionApply",
        "JoinImageWithAlpha",
    ] {
        let mut info = info();
        info.as_object_mut().unwrap().remove(missing);
        assert!(
            image_pipeline::workflow(&c, scene(), &info, 0, true, Some("%reference_image%"), true)
                .is_err()
        );
    }
    assert!(!image_pipeline::reference_path("../private.png"));
    assert!(!image_pipeline::supported_adapter(
        "ip-adapter-faceid_sdxl.bin"
    ));
    let mut c = c;
    c.resolution.as_mut().unwrap().width = 576;
    assert!(
        image_pipeline::workflow(
            &c,
            scene(),
            &info(),
            0,
            true,
            Some("%reference_image%"),
            true
        )
        .is_err()
    );
    c.resolution.as_mut().unwrap().width = 2048;
    assert!(c.validated().is_err());
}
#[test]
fn opaque_portrait_is_rejected_and_complete_character_has_fresh_history_and_own_voice() {
    let url = |bytes: Vec<u8>| format!("data:image/png;base64,{}", STANDARD.encode(bytes));
    assert!(sherka_core::character_studio::transparent_png(&url(png(false))).is_err());
    let bytes = sherka_core::character_studio::transparent_png(&url(png(true))).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let app = Companion::open(dir.path()).unwrap();
    let mut source = presets::sherka(Language::Fr);
    source.connections.tts_voice = Some("original-voice".into());
    let details = draft().profile(&source, config()).unwrap();
    assert!(details.connections.tts_voice.is_none());
    assert!(!details.illustration.enabled);
    let images: BTreeMap<_, _> = EMOTIONS[1..]
        .iter()
        .map(|e| (*e, Some(bytes.clone())))
        .collect();
    let p = app
        .store
        .create_with_avatars(details, Some(&bytes), images)
        .unwrap();
    assert_eq!(p.emotion_avatars.len(), 6);
    assert!(p.avatar.is_some());
    assert_eq!((p.message_count, p.memory_count), (0, 0));
    assert!(app.store.profile(p.id).unwrap().avatar.is_some());
}
#[test]
fn legacy_web_provider_migrates_once_to_global_and_remains_global_after_switch() {
    use sherka_core::search_provider::{Config, Provider};
    let dir = tempfile::tempdir().unwrap();
    let app = Companion::open(dir.path()).unwrap();
    let mut s = app.store.settings().unwrap();
    s.search = None;
    app.store.save_settings(&s).unwrap();
    let id = s.active_profile.unwrap();
    let p = app.store.profile(id).unwrap();
    let mut d = p.details;
    d.search = Config {
        provider: Provider::Brave,
        ..Default::default()
    };
    app.store.update(id, p.revision, d).unwrap();
    drop(app);
    let mut app = Companion::open(dir.path()).unwrap();
    assert_eq!(app.search_config().provider, Provider::Brave);
    app.set_search_config(Config {
        provider: Provider::Tavily,
        ..Default::default()
    })
    .unwrap();
    let next = app
        .store
        .list()
        .unwrap()
        .into_iter()
        .find(|p| p.id != id)
        .unwrap();
    app.switch_profile(next.id).unwrap();
    assert_eq!(app.search_config().provider, Provider::Tavily);
    drop(app);
    assert_eq!(
        Companion::open(dir.path())
            .unwrap()
            .search_config()
            .provider,
        Provider::Tavily
    );
}

#[test]
fn an_imported_picture_becomes_a_flat_gray_bounded_png_portrait() {
    // A transparent 2000×1000 picture with an opaque red square in the middle.
    let mut image = image::RgbaImage::new(2000, 1000);
    for x in 900..1100 {
        for y in 400..600 {
            image.put_pixel(x, y, image::Rgba([255, 0, 0, 255]));
        }
    }
    let mut png = std::io::Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).unwrap();
    let data = format!(
        "data:image/png;base64,{}",
        STANDARD.encode(png.into_inner())
    );
    let portrait = sherka_core::character_studio::imported_portrait(&data).unwrap();
    let bytes = sherka_core::character_studio::portrait_png(&portrait).unwrap();
    let decoded = image::load_from_memory(&bytes).unwrap().to_rgba8();
    assert_eq!((decoded.width(), decoded.height()), (1024, 512));
    let gray = sherka_core::character_studio::PORTRAIT_BACKGROUND;
    assert_eq!(decoded.get_pixel(0, 0).0, [gray[0], gray[1], gray[2], 255]);
    assert_eq!(decoded.get_pixel(512, 256).0, [255, 0, 0, 255]);
    for invalid in [
        "data:image/gif;base64,R0lGODlh",
        "data:image/png;base64,not-base64",
        "https://example.com/a.png",
    ] {
        assert!(sherka_core::character_studio::imported_portrait(invalid).is_err());
    }
}

#[test]
fn a_reference_image_can_replace_the_written_idea() {
    let (messages, _) = sherka_core::character_studio::request(
        "",
        Language::Fr,
        illustration::ModelFamily::Sdxl,
        true,
    )
    .unwrap();
    assert!(
        messages[0]
            .content
            .contains("reference image of the character is attached")
    );
    assert!(messages[1].content.contains("\"reference_image\":true"));
    assert!(
        sherka_core::character_studio::request(
            "",
            Language::Fr,
            illustration::ModelFamily::Sdxl,
            false
        )
        .is_err()
    );
    let (messages, _) = sherka_core::character_studio::request(
        "Un robot.",
        Language::Fr,
        illustration::ModelFamily::Sdxl,
        false,
    )
    .unwrap();
    assert!(!messages[0].content.contains("reference image"));
}

#[test]
fn every_bundled_companion_can_be_retouched_from_its_current_images() {
    for preset in [
        sherka_core::avatar::AvatarPreset::Sherka,
        sherka_core::avatar::AvatarPreset::Botsy,
        sherka_core::avatar::AvatarPreset::Alba,
        sherka_core::avatar::AvatarPreset::Kit,
        sherka_core::avatar::AvatarPreset::Minerva,
    ] {
        let details = presets::companion(preset, Language::En);
        let draft = Draft::for_existing(&details);
        draft.validate().unwrap();
        assert_eq!(draft.name, details.name);
        assert!(!draft.physical_description.trim().is_empty());
        let file = format!("../../public{}", preset.neutral());
        let portrait =
            sherka_core::character_studio::portrait_on_gray(&std::fs::read(file).unwrap()).unwrap();
        assert!(portrait.starts_with("data:image/png;base64,"));
    }
    // A custom companion without appearance text still gets a usable sheet.
    let mut details = presets::companion(sherka_core::avatar::AvatarPreset::Kit, Language::Fr);
    details.avatar_preset = None;
    details.illustration.physical_description.clear();
    Draft::for_existing(&details).validate().unwrap();
}
