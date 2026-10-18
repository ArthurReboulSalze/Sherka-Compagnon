//! Opt-in real ComfyUI smoke check with synthetic scene data only.
//! cargo run -p sherka-core --example check_illustration -- URL CHECKPOINT OUTPUT.png [PROFILE.sqlite3] [--sdxl] [--normal] [--scene SCENE.json]
use base64::{Engine, engine::general_purpose::STANDARD};
use sherka_core::illustration::{Client, Config, Scene, workflow};
use tokio_util::sync::CancellationToken;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<_> = std::env::args().skip(1).collect();
    let mut flag = |name: &str| {
        args.iter()
            .position(|s| s == name)
            .map(|i| args.remove(i))
            .is_some()
    };
    let sdxl = flag("--sdxl");
    let normal = flag("--normal");
    let scene_file = if let Some(i) = args.iter().position(|s| s == "--scene") {
        args.remove(i);
        if i >= args.len() {
            return Err("--scene requires a JSON path".into());
        }
        Some(args.remove(i))
    } else {
        None
    };
    if ![3, 4].contains(&args.len()) {
        return Err(
            "Expected: ComfyUI URL, installed Illustrious checkpoint, output PNG path, optional profile database".into(),
        );
    }
    let mut config = Config {
        server_url: args[0].clone(),
        checkpoint: Some(args[1].clone()),
        physical_description: "Black sports car with a red scanner light".into(),
        model_family: if sdxl {
            sherka_core::illustration::ModelFamily::Sdxl
        } else {
            Default::default()
        },
        turbo: !normal,
        ..Default::default()
    };
    let client = Client::new(&config.server_url)?;
    let (catalog, info) = client.catalog(&config).await?;
    if !catalog.missing_nodes.is_empty() {
        return Err(format!("Missing nodes: {}", catalog.missing_nodes.join(", ")).into());
    }
    let scene = if let Some(path) = scene_file {
        let scene: Scene = serde_json::from_str(&std::fs::read_to_string(path)?)?;
        config.negative_prompt = scene.negative.clone();
        scene
    } else if let Some(profile) = args.get(3) {
        // Read configuration only. Never open/migrate a personal Store or read
        // its history. This optional inference uses synthetic dialogue.
        let db = rusqlite::Connection::open_with_flags(
            profile,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        let value: String = db.query_row(
            "SELECT json_extract(details,'$.connections') FROM profile",
            [],
            |r| r.get(0),
        )?;
        drop(db);
        let connections: sherka_core::connections::Connections = serde_json::from_str(&value)?;
        if connections.llm_provider.is_agent() {
            return Err("Hermes is excluded from local illustrations".into());
        }
        let mut provider = sherka_core::providers::Providers::new(connections)?;
        if !provider.connections.llm_provider.is_local() {
            let model = provider.remote_model().await?;
            provider.use_api_model(&model);
        }
        let (messages, schema) = sherka_core::illustration::configured_scene_request(
            &config,
            sherka_core::illustration::default_description(Some(
                sherka_core::avatar::AvatarPreset::Kit,
            )),
            "Ce soir, on part camper au bord d’un lac dans la forêt.",
            "Une tente au bord du lac, des étoiles et ma carrosserie noire. J’espère que les moustiques apprécient aussi mon humour sec.",
        );
        let limit = provider.connections.context_limit.unwrap_or(8192) as u64;
        let scene: Scene = provider.structured(messages, schema, limit).await?;
        let scene = scene.validated()?;
        std::fs::write(
            std::path::Path::new(&args[2]).with_extension("json"),
            serde_json::to_string_pretty(&scene)?,
        )?;
        println!("Scene prompt prepared by the configured provider using synthetic dialogue.");
        scene
    } else {
        Scene{positive:"masterpiece, best quality, black sports car, red scanner light, glossy bodywork, forest clearing, camping tent, warm lantern light, starry night, peaceful atmosphere, detailed background, cinematic lighting, wide view".into(),negative:"worst quality, low quality, blurry, text, watermark, bad perspective".into()}
    };
    let graph = workflow(&config, scene, &info, 1776723)?;
    let data = client.render(graph, CancellationToken::new()).await?;
    std::fs::write(
        &args[2],
        STANDARD.decode(
            data.strip_prefix("data:image/png;base64,")
                .ok_or("Invalid image")?,
        )?,
    )?;
    println!("Real ComfyUI generation completed with synthetic scene data; PNG saved.");
    Ok(())
}
