//! Fixed three-stage character creation, independent of conversation images.
use crate::{
    Error, Result,
    avatar::Emotion,
    character_studio::Draft,
    illustration::{self, Client, ModelFamily, Scene},
    image_pipeline::catalog_names,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Neutral,
    Mood,
    Transparency,
}
pub const STAGES: [Stage; 3] = [Stage::Neutral, Stage::Mood, Stage::Transparency];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub server_url: String,
    pub checkpoint: Option<String>,
    pub model_family: ModelFamily,
    pub mood_model: Option<String>,
    pub text_encoder: Option<String>,
    pub vae: Option<String>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            server_url: "http://127.0.0.1:8188".into(),
            checkpoint: None,
            model_family: ModelFamily::Sdxl,
            mood_model: None,
            text_encoder: None,
            vae: None,
        }
    }
}
fn invalid() -> Error {
    Error::Validation("Choisis les modèles requis pour cette étape de création.".into())
}
impl Config {
    pub fn validated(mut self) -> Result<Self> {
        self.server_url = illustration::server_url(&self.server_url)?
            .as_str()
            .trim_end_matches('/')
            .into();
        for value in [
            &self.checkpoint,
            &self.mood_model,
            &self.text_encoder,
            &self.vae,
        ]
        .into_iter()
        .flatten()
        {
            if value.is_empty()
                || value.len() > 512
                || value.starts_with(['/', '\\'])
                || value.contains(':')
                || value.chars().any(char::is_control)
                || value.split(['/', '\\']).any(|s| s == "..")
            {
                return Err(invalid());
            }
        }
        Ok(self)
    }
    pub fn neutral_matches(&self, other: &Self) -> bool {
        self.server_url == other.server_url
            && self.checkpoint == other.checkpoint
            && self.model_family == other.model_family
    }
    /// The Klein models and server, independent of how the neutral portrait was made.
    pub fn mood_models_match(&self, other: &Self) -> bool {
        self.server_url == other.server_url
            && self.mood_model == other.mood_model
            && self.text_encoder == other.text_encoder
            && self.vae == other.vae
    }
    pub fn mood_matches(&self, other: &Self) -> bool {
        self.neutral_matches(other)
            && self.mood_model == other.mood_model
            && self.text_encoder == other.text_encoder
            && self.vae == other.vae
    }
    pub fn illustration(&self, draft: &Draft) -> illustration::Config {
        illustration::Config {
            server_url: self.server_url.clone(),
            checkpoint: self.checkpoint.clone(),
            model_family: self.model_family,
            physical_description: draft.physical_description.clone(),
            appearance_negative_prompt: draft.appearance_negative_prompt.clone(),
            turbo: false,
            ..Default::default()
        }
    }
}
pub fn same_portrait(a: &Draft, b: &Draft) -> bool {
    a.physical_description == b.physical_description
        && a.appearance_negative_prompt == b.appearance_negative_prompt
        && a.portrait_prompt == b.portrait_prompt
        && a.prompt_override(Emotion::Neutral) == b.prompt_override(Emotion::Neutral)
}
/// Whether two sheets produce the same prompts for one emotion edit.
pub fn same_mood(a: &Draft, b: &Draft, emotion: Emotion) -> bool {
    a.emotion_prompts.get(&emotion) == b.emotion_prompts.get(&emotion)
        && a.appearance_negative_prompt == b.appearance_negative_prompt
        && a.prompt_override(emotion) == b.prompt_override(emotion)
}
pub fn template(stage: Stage) -> Value {
    serde_json::from_str(match stage {
        Stage::Neutral => include_str!("../../../config/workflows/character-neutral.json"),
        Stage::Mood => include_str!("../../../config/workflows/character-mood.json"),
        Stage::Transparency => {
            include_str!("../../../config/workflows/character-transparency.json")
        }
    })
    .expect("bundled character workflow")
}
#[derive(Debug, Clone, Serialize)]
pub struct Catalog {
    pub checkpoints: Vec<String>,
    pub mood_models: Vec<String>,
    pub text_encoders: Vec<String>,
    pub vaes: Vec<String>,
    pub missing_nodes: BTreeMap<Stage, Vec<String>>,
}
pub async fn catalog(client: &Client) -> Result<(Catalog, Value)> {
    let mut classes = STAGES
        .into_iter()
        .flat_map(|s| {
            template(s)
                .as_object()
                .unwrap()
                .values()
                .map(|n| n["class_type"].as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    classes.sort();
    classes.dedup();
    let info = client.node_info(&classes).await?;
    let missing_nodes = STAGES
        .into_iter()
        .map(|s| {
            let mut missing = template(s)
                .as_object()
                .unwrap()
                .values()
                .filter_map(|n| n["class_type"].as_str())
                .filter(|c| info.get(*c).is_none())
                .map(str::to_owned)
                .collect::<Vec<_>>();
            missing.sort();
            missing.dedup();
            (s, missing)
        })
        .collect();
    Ok((
        Catalog {
            checkpoints: catalog_names(&info, "CheckpointLoaderSimple", "ckpt_name"),
            mood_models: catalog_names(&info, "UNETLoader", "unet_name"),
            text_encoders: catalog_names(&info, "CLIPLoader", "clip_name"),
            vaes: catalog_names(&info, "VAELoader", "vae_name"),
            missing_nodes,
        },
        info,
    ))
}
fn selected(value: &Option<String>, info: &Value, class: &str, field: &str) -> Result<String> {
    let value = value.as_ref().ok_or_else(invalid)?;
    if !catalog_names(info, class, field).contains(value) {
        return Err(invalid());
    }
    Ok(value.clone())
}
// i18n: data-start
pub fn mood_prompt(draft: &Draft, emotion: Emotion) -> Result<String> {
    let expression = draft.emotion_prompts.get(&emotion).ok_or_else(invalid)?;
    Ok(format!(
        "Edit the supplied portrait to express {} clearly. {}. Preserve the exact same character, anatomy, face material, clothing, colors, lighting and solid dark gray background. Keep the straight-on front view, original head-and-bust framing, position and scale. Change only the visible emotional expression; keep all character parts intact. One character, no text or additional subjects.",
        emotion.as_str(),
        expression
    ))
}
// i18n: data-end
/// The prompts composed from the sheet for one image, before any user edit.
pub fn automatic_prompts(config: &Config, draft: &Draft, emotion: Emotion) -> Result<Scene> {
    if emotion == Emotion::Neutral {
        let image_config = config.illustration(draft);
        let scene = draft
            .scene(&image_config, Emotion::Neutral)?
            .validated_for(config.model_family)?;
        let negative = template(Stage::Neutral)["5"]["inputs"]["text"]
            .as_str()
            .unwrap_or("%negative_prompt%")
            .replace("%negative_prompt%", &image_config.scene_negative(true));
        Ok(Scene {
            positive: scene.positive,
            negative: negative.trim().into(),
        })
    } else {
        Ok(Scene {
            positive: mood_prompt(draft, emotion)?,
            negative: draft.appearance_negative_prompt.clone(),
        })
    }
}
/// What ComfyUI receives for this image: the user's edit, or the composed prompts.
pub fn prompts(config: &Config, draft: &Draft, emotion: Emotion) -> Result<Scene> {
    match draft.prompt_override(emotion) {
        Some(scene) => crate::illustration::exact_prompts(&scene.positive, &scene.negative),
        None => automatic_prompts(config, draft, emotion),
    }
}
pub fn workflow(
    config: &Config,
    draft: &Draft,
    stage: Stage,
    emotion: Emotion,
    info: &Value,
    seed: u64,
    source_image: Option<&str>,
) -> Result<Value> {
    let c = config.clone().validated()?;
    draft.validate()?;
    let mut graph = template(stage);
    for node in graph.as_object().unwrap().values() {
        let class = node["class_type"].as_str().unwrap();
        if info.get(class).is_none() {
            return Err(Error::Validation(format!(
                "Un node ComfyUI requis est absent : {class}"
            )));
        }
    }
    match stage {
        Stage::Neutral => {
            if emotion != Emotion::Neutral {
                return Err(invalid());
            }
            graph["1"]["inputs"]["ckpt_name"] = json!(selected(
                &c.checkpoint,
                info,
                "CheckpointLoaderSimple",
                "ckpt_name"
            )?);
            let scene = prompts(&c, draft, Emotion::Neutral)?;
            graph["4"]["inputs"]["text"] = json!(scene.positive);
            graph["5"]["inputs"]["text"] = json!(scene.negative);
            graph["55"]["inputs"]["noise_seed"] = json!(seed);
            graph["66"]["inputs"]["seed"] = json!(seed.wrapping_add(1) & 0x7fff_ffff_ffff_ffff);
        }
        Stage::Mood => {
            graph["128"]["inputs"]["unet_name"] =
                json!(selected(&c.mood_model, info, "UNETLoader", "unet_name")?);
            graph["129"]["inputs"]["clip_name"] =
                json!(selected(&c.text_encoder, info, "CLIPLoader", "clip_name")?);
            graph["132"]["inputs"]["vae_name"] =
                json!(selected(&c.vae, info, "VAELoader", "vae_name")?);
            let scene = prompts(&c, draft, emotion)?;
            graph["130"]["inputs"]["text"] = json!(scene.positive);
            graph["131"]["inputs"]["text"] = json!(scene.negative);
            graph["127"]["inputs"]["noise_seed"] = json!(seed);
        }
        Stage::Transparency => {
            if !catalog_names(info, "RMBG", "model").contains(&"RMBG-2.0".to_owned()) {
                return Err(invalid());
            }
        }
    }
    if stage != Stage::Neutral {
        let path = source_image
            .filter(|p| crate::image_pipeline::reference_path(p))
            .ok_or_else(invalid)?;
        let node = if stage == Stage::Mood { "76" } else { "6" };
        graph[node]["inputs"]["image"] = json!(path);
    }
    Ok(graph)
}
