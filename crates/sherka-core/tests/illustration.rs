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
    c.checkpoint = Some("Flux/flux.safetensors".into());
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
    assert_eq!(graph["21"]["inputs"]["steps"], 28);
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
        4
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
