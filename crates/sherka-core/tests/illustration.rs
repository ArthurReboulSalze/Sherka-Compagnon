use serde_json::{Value, json};
use sherka_core::{
    Companion,
    illustration::{self, Config, Lora, Scene},
    model::{ProfileDraft, Theme},
};

fn config() -> Config {
    Config {
        checkpoint: Some("Illustrious/fixture.safetensors".into()),
        physical_description: "A black car with a red scanner light".into(),
        loras: vec![Lora {
            name: "style/fixture.safetensors".into(),
            strength: 0.65,
        }],
        ..Default::default()
    }
}

#[test]
fn advanced_sampling_keeps_both_passes_independent_of_turbo_and_node_names() {
    let mut c = config();
    c.advanced = illustration::AdvancedSampling {
        enabled: true,
        size: 1024,
        steps_first: 23,
        steps_second: 11,
        cfg_first: 4.2,
        cfg_second: 6.7,
        sampler_name: "dpmpp_2m".into(),
    };
    c.live = true;
    let mut catalog = info();
    catalog["KSampler"] =
        json!({"input":{"required":{"sampler_name":[["euler_ancestral","dpmpp_2m"]]}}});
    let mut custom = illustration::default_workflow();
    // Reversed lexical IDs make workflow dependency order observable.
    let first = custom.as_object_mut().unwrap().remove("21").unwrap();
    let second = custom.as_object_mut().unwrap().remove("25").unwrap();
    custom["z_first"] = first;
    custom["a_second"] = second;
    for node in custom.as_object_mut().unwrap().values_mut() {
        for input in node["inputs"].as_object_mut().unwrap().values_mut() {
            if let Some(link) = input.as_array_mut().filter(|a| a.len() == 2) {
                if link[0] == "21" {
                    link[0] = json!("z_first");
                }
                if link[0] == "25" {
                    link[0] = json!("a_second");
                }
            }
        }
    }
    c.workflow = Some(custom);
    let scene = Scene {
        positive: "moon, stars, sky".into(),
        negative: String::new(),
    };
    for turbo in [true, false] {
        c.turbo = turbo;
        let g = illustration::workflow(&c, scene.clone(), &catalog, 42).unwrap();
        assert_eq!(g["z_first"]["inputs"]["steps"], 23);
        assert_eq!(g["18"]["inputs"]["width"], 1024);
        assert_eq!(g["18"]["inputs"]["height"], 1024);
        assert_eq!(g["a_second"]["inputs"]["steps"], 11);
        assert_eq!(g["z_first"]["inputs"]["cfg"], 4.2);
        assert_eq!(g["a_second"]["inputs"]["cfg"], 6.7);
        assert_eq!(g["z_first"]["inputs"]["sampler_name"], "dpmpp_2m");
        assert_eq!(g["a_second"]["inputs"]["sampler_name"], "dpmpp_2m");
    }
    let roundtrip: Config = serde_json::from_value(serde_json::to_value(&c).unwrap()).unwrap();
    assert_eq!(roundtrip, c);
    c.advanced.size = 770;
    assert!(c.clone().validated().is_err());
    c.advanced.size = 1024;
    c.advanced.steps_second = 0;
    assert!(c.clone().validated().is_err());
    c.advanced.steps_second = 11;
    c.advanced.cfg_first = f64::NAN;
    assert!(c.clone().validated().is_err());
    c.advanced.cfg_first = 4.2;
    c.advanced.sampler_name = "absent".into();
    assert!(illustration::workflow(&c, scene, &catalog, 42).is_err());
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
    out["CheckpointLoaderSimple"] =
        json!({"input":{"required":{"ckpt_name":[["Illustrious/fixture.safetensors"]]}}});
    out["LoraLoader"] = json!({"input":{"required":{"lora_name":[["style/fixture.safetensors","Models/Turbo/PAseer-SDXL-LCM+Turbo.safetensors"]]}}});
    out
}
#[test]
fn service_scope_graphs_and_configuration_reject_unrelated_or_executable_inputs() {
    for endpoint in [
        "https://example.com",
        "http://user:password@localhost:8188",
        "file:///tmp/graph",
        "http://127.0.0.1:8188?secret=x",
        "http://127.0.0.1:8188/arbitrary",
    ] {
        assert!(illustration::server_url(endpoint).is_err());
    }
    for endpoint in [
        "http://127.0.0.1:8188",
        "http://192.168.1.2:8188",
        "http://[::1]:8188",
    ] {
        assert!(illustration::server_url(endpoint).is_ok());
    }
    let mut c = config();
    c.checkpoint = Some("../outside.safetensors".into());
    assert!(c.validated().is_err());
    let mut c = config();
    c.loras[0].strength = f32::NAN;
    assert!(c.validated().is_err());
    let mut c = config();
    c.loras[0].name = "../secret.safetensors".into();
    assert!(c.validated().is_err());
    let mut graph = illustration::default_workflow();
    graph["99"] = json!({"class_type":"ExecutePython","inputs":{"code":"pass"}});
    assert!(illustration::validate_workflow(&graph).is_err());
    assert!(illustration::validate_workflow(&json!({"nodes":[]})).is_err());
    let mut c = config();
    c.enabled = true;
    c.physical_description.clear();
    assert!(c.validated().is_err());
}
#[test]
fn supplied_graph_receives_real_clip_links_user_loras_hidden_turbo_and_square_preview() {
    let graph = illustration::workflow(
        &config(),
        Scene {
            positive:
                "masterpiece, best quality, black car, camping, moonlit forest, <lora:unmanaged:4>"
                    .into(),
            negative: "blurry, text".into(),
        },
        &info(),
        42,
    )
    .unwrap();
    assert_eq!(
        graph["1"]["inputs"]["ckpt_name"],
        "Illustrious/fixture.safetensors"
    );
    assert_eq!(
        graph["2"]["inputs"]["lora_1"]["lora"],
        "Models/Turbo/PAseer-SDXL-LCM+Turbo.safetensors"
    );
    assert!((graph["2"]["inputs"]["lora_2"]["strength"].as_f64().unwrap() - 0.65).abs() < 0.0001);
    assert_eq!(graph["5"]["inputs"]["clip"], json!(["34", 1]));
    assert_eq!(graph["19"]["inputs"]["text"], json!(["34", 2]));
    assert_eq!(graph["19"]["inputs"]["clip"], json!(["34", 1]));
    assert!(
        !graph["4"]["inputs"]["text"]
            .as_str()
            .unwrap()
            .contains("lora:")
    );
    assert!(
        graph["4"]["inputs"]["text"]
            .as_str()
            .unwrap()
            .contains("moonlit forest")
    );
    assert_eq!(
        graph["18"]["inputs"]["width"],
        graph["18"]["inputs"]["height"]
    );
    assert_eq!(graph["18"]["inputs"]["batch_size"], 1);
    assert_eq!(graph["21"]["inputs"]["seed"], 42);
    assert_eq!(graph["33"]["class_type"], "PreviewImage");
    assert_eq!(graph["25"]["inputs"]["steps"], 9);
    let mut missing = info();
    missing["LoraLoader"]["input"]["required"]["lora_name"] =
        json!([["style/fixture.safetensors"]]);
    assert!(
        illustration::workflow(
            &config(),
            Scene {
                positive: "black car, forest".into(),
                negative: "blurry".into()
            },
            &missing,
            1
        )
        .is_err()
    );
}
#[test]
fn normal_mode_removes_turbo_and_api_import_is_normalized_without_user_asset_paths() {
    let mut c = config();
    c.turbo = false;
    let mut original = illustration::default_workflow();
    original["33"] = json!({"class_type":"SaveImage","inputs":{"images":["29",0],"filename_prefix":"untrusted/path"}});
    c.workflow = Some(original);
    let graph = illustration::workflow(
        &c,
        Scene {
            positive: "best quality, car beside a tent".into(),
            negative: "bad anatomy".into(),
        },
        &info(),
        77,
    )
    .unwrap();
    assert!(!graph.to_string().contains("PAseer"));
    assert!(!graph.to_string().contains("untrusted/path"));
    assert_eq!(
        graph["2"]["inputs"]["lora_1"]["lora"],
        "style/fixture.safetensors"
    );
    assert_eq!(graph["21"]["inputs"]["sampler_name"], "euler_ancestral");
    assert_eq!(graph["21"]["inputs"]["steps"], 16);
    assert_eq!(graph["25"]["inputs"]["steps"], 16);
    assert_eq!(graph["21"]["inputs"]["cfg"], 3.8);
    assert_eq!(graph["25"]["inputs"]["cfg"], 7.0);
}

#[test]
fn sdxl_prose_user_negative_and_arbitrary_catalog_paths_are_supported() {
    let mut c = config();
    c.model_family = illustration::ModelFamily::Sdxl;
    c.checkpoint = Some("My collection/landscape-model.safetensors".into());
    c.checkpoint_folder = "My collection".into();
    c.lora_folder = "Styles/Nature".into();
    c.negative_prompt = "human face, portraits, captions\nwatermark".into();
    assert!(c.clone().validated().is_ok());
    let (messages, _) =
        illustration::configured_scene_request(&c, "", "Imagine a landscape", "A mountain lake.");
    assert!(messages[0].content.contains("fluent English visual prose"));
    let data: Value = serde_json::from_str(&messages[1].content).unwrap();
    assert_eq!(data["negative_prompt"], c.negative_prompt);
    assert_eq!(data["include_companion"], false);
    let mut nodes = info();
    nodes["CheckpointLoaderSimple"]["input"]["required"]["ckpt_name"] =
        json!([[c.checkpoint.clone().unwrap()]]);
    let scene = Scene {
        positive: "A mountain lake under a pink sunset, wide landscape composition".into(),
        negative: "the model must not override my prompt".into(),
    };
    let graph = illustration::workflow(&c, scene.clone(), &nodes, 1).unwrap();
    assert_eq!(graph["5"]["inputs"]["text"], c.negative_prompt);
    assert_eq!(graph["21"]["inputs"]["steps"], 5);
    assert_eq!(graph["25"]["inputs"]["cfg"], 3.5);
    c.negative_prompt.clear();
    c.turbo = false;
    let mut custom = illustration::default_workflow();
    custom["21"]["inputs"]["cfg"] = json!(2.2);
    custom["25"]["inputs"]["cfg"] = json!(4.1);
    c.workflow = Some(custom.clone());
    for _ in 0..2 {
        let graph = illustration::workflow(&c, scene.clone(), &nodes, 1).unwrap();
        assert_eq!(graph["5"]["inputs"]["text"], "");
        assert_eq!(graph["21"]["inputs"]["cfg"], 4.4);
        assert_eq!(graph["25"]["inputs"]["cfg"], 8.2);
    }
    assert_eq!(c.workflow, Some(custom));
}
#[test]
fn planned_character_count_keeps_solo_out_of_groups_and_landscapes() {
    use illustration::{ModelFamily, SceneCharacters, ScenePlan};
    for (characters, prompt, expected) in [
        (
            SceneCharacters::One,
            "best quality, robot dancing, pink tutu",
            "solo, best quality, robot dancing, pink tutu",
        ),
        (
            SceneCharacters::One,
            "solo, robot dancing, SOLO",
            "solo, robot dancing",
        ),
        (
            SceneCharacters::None,
            "best quality, SOLO, moon craters",
            "best quality, moon craters",
        ),
        (
            SceneCharacters::Several,
            "solo, robot, human in a blue coat, lakeside",
            "robot, human in a blue coat, lakeside",
        ),
    ] {
        let plan = ScenePlan {
            characters,
            description: "A visual description used only by the planner.".into(),
            positive: prompt.into(),
            negative: "bad anatomy".into(),
        };
        assert_eq!(
            plan.clone().into_scene(ModelFamily::Illustrious).positive,
            expected
        );
        let prose = plan.into_scene(ModelFamily::Sdxl);
        assert_eq!(prose.positive, prompt);
        assert_eq!(prose.negative, "bad anatomy");
    }
}

#[test]
fn sdxl_sentences_keep_requested_action_and_clothes_all_the_way_to_clip() {
    let mut c = config();
    c.model_family = illustration::ModelFamily::Sdxl;
    let prose = "A silver robot with a fully mechanical face and green eyes stands beneath a broad canopy of rainclouds wearing a yellow raincoat and rain boots while jumping into a puddle beside the pavement.\nSoft evening light reflects off its metallic face and the wet stones.";
    let scene = Scene {
        positive: format!("{prose}, <lora:unmanaged:4>, score_9, http://example.com"),
        negative: "blurry".into(),
    };
    let graph = illustration::workflow(&c, scene, &info(), 42).unwrap();
    let prompt = graph["4"]["inputs"]["text"].as_str().unwrap();
    assert_eq!(prompt, prose.replace('\n', " "));
    assert_eq!(graph["5"]["inputs"]["text"], c.negative_prompt);

    let bounded = Scene {
        positive: "A robot dancing in a raincoat. ".repeat(1000),
        negative: String::new(),
    }
    .validated_for(illustration::ModelFamily::Sdxl)
    .unwrap();
    assert_eq!(bounded.positive.split_whitespace().count(), 180);
    assert!(bounded.positive.chars().count() <= 4000);
}

#[test]
fn illustrious_scene_composition_is_freeform_bounded_and_uses_both_latest_sides() {
    let (messages, schema) = illustration::scene_request(
        "BLACK_CAR_MARKER",
        "USER_SCENE_MARKER",
        "COMPANION_SCENE_MARKER",
    );
    assert_eq!(messages.len(), 2);
    assert!(messages[0].content.contains("NO fixed tag dictionary"));
    assert!(messages[0].content.contains("Illustrious"));
    let data: Value = serde_json::from_str(&messages[1].content).unwrap();
    assert_eq!(data["appearance"], "BLACK_CAR_MARKER");
    assert_eq!(data["latest_user"], "USER_SCENE_MARKER");
    assert_eq!(data["latest_companion"], "COMPANION_SCENE_MARKER");
    assert_eq!(schema["additionalProperties"], false);
    let s = Scene {
        positive: "intricate futuristic locomotive, custom made visual idea, score_9, <lora:bad:2>"
            .into(),
        negative: "blur".into(),
    }
    .validated()
    .unwrap();
    assert!(s.positive.contains("custom made visual idea"));
    assert!(!s.positive.contains("score_9"));
    assert!(!s.positive.contains("lora:"));
    let (_, schema2) = illustration::scene_request("", "", &"x".repeat(50_000));
    assert_eq!(schema, schema2);
}

#[test]
fn unrelated_scenes_do_not_receive_the_avatar_appearance_and_minerva_migration_is_narrow() {
    use sherka_core::avatar::AvatarPreset;
    let (messages, _) =
        illustration::subject_request("Minerva", "Imagine un paysage", "Une montagne enneigée.");
    assert!(!messages.iter().any(|m| m.content.contains("armored")));
    let (messages, _) =
        illustration::scene_request("", "Imagine un paysage", "Une montagne enneigée.");
    let value: Value = serde_json::from_str(&messages[1].content).unwrap();
    assert_eq!(value["include_companion"], false);
    assert_eq!(value["appearance"], Value::Null);
    let old = "An adult feminine armored android, silver and dark gray angular armor with red accents, short pale hair, stern irritated expression, strong mechanical silhouette.";
    let mut draft = ProfileDraft {
        avatar_preset: Some(AvatarPreset::Minerva),
        ..Default::default()
    };
    draft.illustration.physical_description = old.into();
    assert!(illustration::upgrade_appearance(&mut draft));
    assert!(
        draft
            .illustration
            .physical_description
            .contains("Not Human Face . Robot female Face .")
    );
    assert!(!illustration::upgrade_appearance(&mut draft));
    draft.illustration.physical_description = "My custom robot design".into();
    assert!(!illustration::upgrade_appearance(&mut draft));
    assert_eq!(
        draft.illustration.physical_description,
        "My custom robot design"
    );
}
#[test]
fn per_character_config_and_global_theme_survive_import_copy_and_old_profile_migration() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = Companion::open(dir.path()).unwrap();
    let id = app.session.snapshot().profile_id;
    let p = app.store.profile(id).unwrap();
    let mut draft = p.details;
    draft.illustration = config();
    app.update_profile(id, p.revision, draft).unwrap();
    app.set_theme(Theme::Ocean).unwrap();
    let copy = app.store.duplicate(id).unwrap();
    assert_eq!(copy.details.illustration, config());
    let exported = app.store.export_profile(id).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&exported).unwrap()["version"],
        5
    );
    let imported = app.store.import_profile(&exported).unwrap();
    assert_eq!(imported.details.illustration, config());
    let path = dir
        .path()
        .join("profiles")
        .join(id.to_string())
        .join("profile.sqlite3");
    let db = rusqlite::Connection::open(path).unwrap();
    let mut old = serde_json::to_value(ProfileDraft::default()).unwrap();
    old.as_object_mut().unwrap().remove("illustration");
    db.execute("UPDATE profile SET details=?1", [old.to_string()])
        .unwrap();
    db.execute_batch("PRAGMA user_version=6;").unwrap();
    drop(db);
    drop(app);
    let app = Companion::open(dir.path()).unwrap();
    assert!(!app.store.profile(id).unwrap().details.illustration.enabled);
    assert_eq!(app.snapshot().unwrap().settings.theme, Theme::Ocean);
}

#[test]
fn positive_suffix_is_appended_once_verbatim_in_both_families() {
    for family in [
        illustration::ModelFamily::Illustrious,
        illustration::ModelFamily::Sdxl,
    ] {
        let mut c = config();
        c.model_family = family;
        c.positive_prompt_suffix = "(warm light:1.3), watercolor\nsoft shadows".into();
        let scene = Scene {
            positive: "mountain lake, wide view".into(),
            negative: "blur".into(),
        };
        for seed in 1..=2 {
            let graph = illustration::workflow(&c, scene.clone(), &info(), seed).unwrap();
            let texts: Vec<&str> = graph
                .as_object()
                .unwrap()
                .values()
                .filter_map(|node| node["inputs"]["text"].as_str())
                .collect();
            let positive = texts
                .iter()
                .find(|text| text.starts_with("mountain lake"))
                .unwrap();
            assert!(positive.ends_with(&c.positive_prompt_suffix));
            assert_eq!(positive.matches("warm light").count(), 1);
            assert!(texts.contains(&c.negative_prompt.as_str()));
        }
        c.positive_prompt_suffix = "x".repeat(4001);
        assert!(c.validated().is_err());
    }
    let mut old = serde_json::to_value(config()).unwrap();
    old.as_object_mut()
        .unwrap()
        .remove("positive_prompt_suffix");
    let upgraded: Config = serde_json::from_value(old).unwrap();
    assert!(upgraded.positive_prompt_suffix.is_empty());
}
