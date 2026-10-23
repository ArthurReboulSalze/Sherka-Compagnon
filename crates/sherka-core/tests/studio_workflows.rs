use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use sherka_core::{
    avatar::Emotion,
    character_studio::{Draft, EMOTIONS, portrait_png, transparent_png},
    i18n::Language,
    illustration::ModelFamily,
    studio_workflows::{self, Config, STAGES, Stage},
};

fn draft() -> Draft {
    Draft {
        name: "Robot".into(),
        personality: "Curious".into(),
        instructions: "Be natural".into(),
        accent: "teal".into(),
        language: Language::En,
        physical_description: "A silver mechanical robot with green optical eyes".into(),
        appearance_negative_prompt: "human face, hair".into(),
        portrait_prompt: "full body, side profile, red armor".into(),
        emotion_prompts: EMOTIONS[1..]
            .iter()
            .map(|e| (*e, format!("{} with mechanical eyes", e.as_str())))
            .collect(),
        prompt_overrides: Default::default(),
    }
}
fn config() -> Config {
    Config {
        checkpoint: Some("folder/portrait.safetensors".into()),
        mood_model: Some("klein.safetensors".into()),
        text_encoder: Some("qwen.safetensors".into()),
        vae: Some("flux2.safetensors".into()),
        ..Default::default()
    }
}
fn info() -> Value {
    let mut info = json!({});
    for stage in STAGES {
        for node in studio_workflows::template(stage)
            .as_object()
            .unwrap()
            .values()
        {
            info[node["class_type"].as_str().unwrap()] = json!({});
        }
    }
    for (class, field, name) in [
        (
            "CheckpointLoaderSimple",
            "ckpt_name",
            "folder/portrait.safetensors",
        ),
        ("UNETLoader", "unet_name", "klein.safetensors"),
        ("CLIPLoader", "clip_name", "qwen.safetensors"),
        ("VAELoader", "vae_name", "flux2.safetensors"),
        ("RMBG", "model", "RMBG-2.0"),
    ] {
        info[class] = json!({"input":{"required":{field:[[name]]}}});
    }
    info
}
fn graph(stage: Stage, emotion: Emotion) -> Value {
    studio_workflows::workflow(
        &config(),
        &draft(),
        stage,
        emotion,
        &info(),
        42,
        Some("sherka/reference-00000000-0000-4000-8000-000000000042.png [temp]"),
    )
    .unwrap()
}
#[test]
fn neutral_keeps_supplied_quality_sampling_and_flat_gray_front_portrait() {
    for family in [ModelFamily::Sdxl, ModelFamily::Illustrious] {
        let c = Config {
            model_family: family,
            ..config()
        };
        let g = studio_workflows::workflow(
            &c,
            &draft(),
            Stage::Neutral,
            Emotion::Neutral,
            &info(),
            42,
            None,
        )
        .unwrap();
        assert_eq!(g["1"]["inputs"]["ckpt_name"], "folder/portrait.safetensors");
        assert_eq!(g["18"]["inputs"]["width"], 1024);
        assert_eq!(g["55"]["inputs"]["steps"], 20);
        assert_eq!(g["56"]["inputs"]["steps"], 20);
        assert_eq!(g["55"]["inputs"]["end_at_step"], 15);
        assert_eq!(g["56"]["inputs"]["start_at_step"], 15);
        assert_eq!(g["55"]["inputs"]["cfg"], 8);
        assert_eq!(g["67"]["inputs"]["scale_by"], 1.5);
        assert_eq!(g["66"]["inputs"]["steps"], 25);
        assert_eq!(g["66"]["inputs"]["cfg"], 6);
        assert_eq!(g["66"]["inputs"]["denoise"], 0.55);
        assert_eq!(g["79"]["inputs"]["size"], 1024);
        assert_eq!(g["72"]["inputs"]["images"], json!(["79", 0]));
        let positive = g["4"]["inputs"]["text"].as_str().unwrap();
        for term in [
            "front view",
            "bust portrait",
            "solid dark gray background",
            "green optical eyes",
            "red armor",
        ] {
            assert!(positive.contains(term), "{positive}");
        }
        assert!(!positive.contains("full body"));
        assert!(!positive.contains("side profile"));
        let negative = g["5"]["inputs"]["text"].as_str().unwrap();
        assert!(negative.contains("human face"));
        assert!(negative.contains("gradient background"));
        assert!(!negative.split(',').any(|s| s.trim() == "background"));
    }
}
#[test]
fn moods_edit_the_same_neutral_without_ip_adapter_or_new_portrait_prompt() {
    for emotion in EMOTIONS[1..].iter().copied() {
        let g = graph(Stage::Mood, emotion);
        assert_eq!(g["128"]["inputs"]["unet_name"], "klein.safetensors");
        assert_eq!(g["129"]["inputs"]["clip_name"], "qwen.safetensors");
        assert_eq!(g["129"]["inputs"]["type"], "flux2");
        assert_eq!(g["132"]["inputs"]["vae_name"], "flux2.safetensors");
        assert_eq!(
            g["76"]["inputs"]["image"],
            "sherka/reference-00000000-0000-4000-8000-000000000042.png [temp]"
        );
        assert_eq!(g["139"]["inputs"]["latent"], json!(["138", 0]));
        assert_eq!(g["137"]["inputs"]["latent"], json!(["138", 0]));
        assert_eq!(g["124"]["inputs"]["cfg"], 5);
        assert_eq!(g["123"]["inputs"]["steps"], 20);
        assert_eq!(g["9"]["class_type"], "PreviewImage");
        let prompt = g["130"]["inputs"]["text"].as_str().unwrap();
        for term in [
            emotion.as_str(),
            "solid dark gray background",
            "straight-on front view",
            "same character",
            "face material",
        ] {
            assert!(prompt.contains(term));
        }
        assert_eq!(
            g["131"]["inputs"]["text"],
            draft().appearance_negative_prompt
        );
        assert!(!g.to_string().contains("IPAdapter"));
        assert!(!g.to_string().contains("LayeredDiffusion"));
    }
}
#[test]
fn transparency_keeps_exact_fixed_settings_and_joins_alpha_to_original_pixels() {
    let g = graph(Stage::Transparency, Emotion::Joy);
    assert_eq!(
        g["2"]["inputs"],
        json!({
            "model":"RMBG-2.0","sensitivity":1,"process_res":1024,"mask_blur":0,"mask_offset":-2,
            "invert_output":false,"refine_foreground":true,"unload_model":false,
            "background":"Color","background_color":["5",0],"image":["6",0]
        })
    );
    assert_eq!(g["5"]["inputs"], json!({"preset":"gray","color":""}));
    assert_eq!(g["3"]["inputs"]["mask"], json!(["2", 1]));
    assert_eq!(g["4"]["inputs"], json!({"image":["6",0],"alpha":["3",0]}));
    assert_eq!(g["1"]["inputs"]["images"], json!(["4", 0]));
    let unknown = json!({"background_model":"BEN2","mask_offset":2});
    assert!(serde_json::from_value::<Config>(unknown).is_err());
}
#[test]
fn stages_validate_only_their_own_dependencies_and_require_real_model_choices() {
    let mut info = info();
    info.as_object_mut().unwrap().remove("RMBG");
    assert!(
        studio_workflows::workflow(
            &config(),
            &draft(),
            Stage::Neutral,
            Emotion::Neutral,
            &info,
            42,
            None
        )
        .is_ok()
    );
    assert!(
        studio_workflows::workflow(
            &config(),
            &draft(),
            Stage::Transparency,
            Emotion::Neutral,
            &info,
            42,
            Some("%reference_image%")
        )
        .is_err()
    );
    for stage in STAGES {
        let g = studio_workflows::template(stage).to_string();
        assert!(!g.contains("C:\\\\"));
        assert!(!g.contains("SaveImage"));
        assert!(!g.contains("IPAdapter"));
    }
    let bad = Config {
        mood_model: Some("not-installed.safetensors".into()),
        ..config()
    };
    assert!(
        studio_workflows::workflow(
            &bad,
            &draft(),
            Stage::Mood,
            Emotion::Joy,
            &info,
            42,
            Some("%reference_image%")
        )
        .is_err()
    );
    for path in [
        "../secret.png",
        "C:\\private.png",
        "https://example.com/image.png",
    ] {
        assert!(
            studio_workflows::workflow(
                &config(),
                &draft(),
                Stage::Mood,
                Emotion::Joy,
                &info,
                42,
                Some(path)
            )
            .is_err()
        );
    }
}
#[test]
fn changing_metadata_or_mood_models_does_not_invalidate_neutral_identity() {
    let original = draft();
    let mut edit = original.clone();
    edit.name = "New name".into();
    edit.personality = "New temperament".into();
    edit.emotion_prompts.insert(Emotion::Joy, "Laughing".into());
    assert!(studio_workflows::same_portrait(&original, &edit));
    edit.physical_description = "A different body".into();
    assert!(!studio_workflows::same_portrait(&original, &edit));
    let a = config();
    let mut b = a.clone();
    b.vae = Some("new.safetensors".into());
    assert!(a.neutral_matches(&b));
    assert!(!a.mood_matches(&b));
    b.checkpoint = Some("new.safetensors".into());
    assert!(!a.neutral_matches(&b));
}
#[test]
fn opaque_intermediate_is_valid_but_final_avatar_requires_actual_alpha() {
    let pixels = image::RgbaImage::from_pixel(1024, 1024, image::Rgba([128, 128, 128, 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(pixels)
        .write_to(&mut out, image::ImageFormat::Png)
        .unwrap();
    let data = format!(
        "data:image/png;base64,{}",
        STANDARD.encode(out.into_inner())
    );
    let intermediate = portrait_png(&data).unwrap();
    assert_eq!(
        image::load_from_memory(&intermediate).unwrap().width(),
        1024
    );
    assert!(transparent_png(&data).is_err());
}

#[test]
fn edited_prompts_are_sent_verbatim_and_invalidate_only_their_image() {
    let mut d = draft();
    let automatic = studio_workflows::automatic_prompts(&config(), &d, Emotion::Neutral).unwrap();
    // The automatic view matches what the bundled graph receives.
    let g = graph(Stage::Neutral, Emotion::Neutral);
    assert_eq!(g["4"]["inputs"]["text"], json!(automatic.positive));
    assert_eq!(g["5"]["inputs"]["text"], json!(automatic.negative));
    let neutral = sherka_core::illustration::Scene {
        positive: "(robot portrait:1.2), my exact words".into(),
        negative: "photo, human".into(),
    };
    d.prompt_overrides.insert(Emotion::Neutral, neutral.clone());
    let joy = sherka_core::illustration::Scene {
        positive: "Make the optical eyes glow brighter with joy.".into(),
        negative: String::new(),
    };
    d.prompt_overrides.insert(Emotion::Joy, joy.clone());
    d.validate().unwrap();
    let edited = |stage, emotion| {
        studio_workflows::workflow(
            &config(),
            &d,
            stage,
            emotion,
            &info(),
            42,
            Some("sherka/reference-00000000-0000-4000-8000-000000000042.png [temp]"),
        )
        .unwrap()
    };
    let g = edited(Stage::Neutral, Emotion::Neutral);
    assert_eq!(g["4"]["inputs"]["text"], json!(neutral.positive));
    assert_eq!(g["5"]["inputs"]["text"], json!(neutral.negative));
    let g = edited(Stage::Mood, Emotion::Joy);
    assert_eq!(g["130"]["inputs"]["text"], json!(joy.positive));
    assert_eq!(g["131"]["inputs"]["text"], json!(""));
    // Other emotions keep their composed Klein instruction.
    let g = edited(Stage::Mood, Emotion::Sadness);
    assert_eq!(
        g["130"]["inputs"]["text"],
        json!(studio_workflows::mood_prompt(&d, Emotion::Sadness).unwrap())
    );
    let original = draft();
    assert!(!studio_workflows::same_portrait(&original, &d));
    assert!(!studio_workflows::same_mood(&original, &d, Emotion::Joy));
    assert!(studio_workflows::same_mood(&original, &d, Emotion::Sadness));
    // Edited prompts keep the same bounds as generated ones.
    d.prompt_overrides.insert(
        Emotion::Anger,
        sherka_core::illustration::Scene {
            positive: " ".into(),
            negative: String::new(),
        },
    );
    assert!(d.validate().is_err());
}

#[test]
fn sheets_round_trip_without_overrides_and_accept_them_from_the_panel() {
    let value = serde_json::to_value(draft()).unwrap();
    assert!(value.get("prompt_overrides").is_none());
    let mut with = value.clone();
    with["prompt_overrides"] = json!({"neutral":{"positive":"exact","negative":""}});
    let parsed: Draft = serde_json::from_value(with).unwrap();
    assert_eq!(
        parsed.prompt_override(Emotion::Neutral).unwrap().positive,
        "exact"
    );
}

#[test]
fn widget_prompt_edits_keep_syntax_and_reject_empty_or_control_text() {
    use sherka_core::illustration::exact_prompts;
    let scene = exact_prompts("  (moon:1.3), night sky\nwatercolor  ", "people").unwrap();
    assert_eq!(scene.positive, "(moon:1.3), night sky\nwatercolor");
    assert_eq!(scene.negative, "people");
    assert!(exact_prompts("", "x").is_err());
    assert!(exact_prompts("ok", "bad\u{0007}").is_err());
    assert!(exact_prompts(&"a".repeat(8001), "").is_err());
}
