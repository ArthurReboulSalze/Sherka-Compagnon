//! Opt-in check of the three bundled creation workflows on a local ComfyUI.
//! Arguments: URL CHECKPOINT OUTPUT_DIRECTORY [NEUTRAL.png]
use base64::{Engine, engine::general_purpose::STANDARD};
use sherka_core::{
    character_studio::{Draft, EMOTIONS, portrait_png, transparent_png},
    i18n::Language,
    illustration::{Client, ModelFamily},
    studio_workflows::{self, Config, Stage},
};
use tokio_util::sync::CancellationToken;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !(3..=4).contains(&args.len()) {
        return Err("Expected URL CHECKPOINT OUTPUT_DIRECTORY [NEUTRAL.png]".into());
    }
    let directory = std::path::Path::new(&args[2]);
    std::fs::create_dir_all(directory)?;
    let config = Config {
        server_url: args[0].clone(),
        checkpoint: Some(args[1].clone()),
        model_family: ModelFamily::Sdxl,
        mood_model: Some("flux-2-klein-4b-fp8.safetensors".into()),
        text_encoder: Some("qwen3vl_4b_fp8_scaled.safetensors".into()),
        vae: Some("flux2-vae.safetensors".into()),
    };
    let draft=Draft {
        name:"Vega Test".into(), personality:"Une exploratrice mécanique calme et curieuse.".into(),
        instructions:"Réponds naturellement.".into(), accent:"teal".into(),language:Language::Fr,
        physical_description:"A fully mechanical silver robot with a rigid metal faceplate, green optical eyes and red armor accents.".into(),
        appearance_negative_prompt:"human face, human skin, hair, flesh, human lips".into(),
        portrait_prompt:"Front view bust portrait, head and shoulders, solid dark gray background.".into(),
        emotion_prompts:EMOTIONS[1..].iter().map(|e|(*e,
            "Delighted joyful expression, smiling crescent shaped mechanical optical eyes, preserve the rigid metal faceplate.".into())).collect(),
        prompt_overrides:Default::default(),
    };
    let client = Client::new(&config.server_url)?;
    let (catalog, info) = studio_workflows::catalog(&client).await?;
    if catalog.missing_nodes.values().any(|v| !v.is_empty()) {
        return Err(format!("Missing nodes: {:?}", catalog.missing_nodes).into());
    }
    let mut neutral = args.get(3).map(std::fs::read).transpose()?;
    for (stage, emotion, name) in [
        (Stage::Neutral, EMOTIONS[0], "neutral"),
        (Stage::Mood, EMOTIONS[1], "joy"),
        (Stage::Transparency, EMOTIONS[0], "neutral-alpha"),
        (Stage::Transparency, EMOTIONS[1], "joy-alpha"),
    ] {
        if stage == Stage::Neutral
            && let Some(bytes) = &neutral
        {
            std::fs::write(directory.join("neutral.png"), bytes)?;
            continue;
        }
        let input = match stage {
            Stage::Neutral => None,
            Stage::Mood => Some(client.upload_image(neutral.as_ref().unwrap()).await?),
            Stage::Transparency => {
                let bytes = std::fs::read(directory.join(format!("{}.png", emotion.as_str())))?;
                Some(client.upload_image(&bytes).await?)
            }
        };
        let graph = studio_workflows::workflow(
            &config,
            &draft,
            stage,
            emotion,
            &info,
            420042,
            input.as_deref(),
        )?;
        std::fs::write(
            directory.join(format!("{name}.json")),
            serde_json::to_vec_pretty(&graph)?,
        )?;
        println!("Starting {name}");
        let image = client.render(graph, CancellationToken::new()).await?;
        let bytes = if stage == Stage::Transparency {
            transparent_png(&image)?
        } else {
            portrait_png(&image)?
        };
        let decoded = image::load_from_memory(&bytes)?.to_rgba8();
        let alpha = decoded.pixels().filter(|p| p[3] < 32).count();
        std::fs::write(directory.join(format!("{name}.png")), &bytes)?;
        println!(
            "{name}: {}x{}, {:.1}% transparent pixels",
            decoded.width(),
            decoded.height(),
            100.0 * alpha as f64 / (decoded.width() as f64 * decoded.height() as f64)
        );
        if stage == Stage::Neutral {
            neutral = Some(bytes);
        }
        // Validate that the same PNG transport is accepted by the studio.
        let _ = portrait_png(&format!(
            "data:image/png;base64,{}",
            STANDARD.encode(std::fs::read(directory.join(format!("{name}.png")))?)
        ))?;
    }
    Ok(())
}
