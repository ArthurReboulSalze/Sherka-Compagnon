//! Image conditioning and output sizing, shared by scenes and the character studio.
use crate::{
    Error, Result,
    illustration::{self, Config, Scene},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const NODE_CLASSES: &[&str] = &[
    "IPAdapterModelLoader",
    "IPAdapterAdvanced",
    "CLIPVisionLoader",
    "LoadImage",
    "LayeredDiffusionApply",
    "LayeredDiffusionDecodeRGBA",
    "UpscaleModelLoader",
    "ImageUpscaleWithModel",
    "ImageScale",
    "SplitImageWithAlpha",
    "JoinImageWithAlpha",
    "MaskToImage",
    "ImageToMask",
    "LatentUpscaleBy",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
    pub latent_scale: f64,
    pub upscale_model: Option<String>,
}
impl Default for Resolution {
    fn default() -> Self {
        Self {
            width: 768,
            height: 768,
            latent_scale: 1.0,
            upscale_model: None,
        }
    }
}
impl Resolution {
    pub fn sampled_size(&self) -> (u32, u32) {
        let scale = |n: u32| ((n as f64 * self.latent_scale / 8.0).round() as u32) * 8;
        (scale(self.width), scale(self.height))
    }
    pub fn output_size(&self) -> (u32, u32) {
        let (w, h) = self.sampled_size();
        let scale = if self.upscale_model.is_some() { 2 } else { 1 };
        (w * scale, h * scale)
    }
    pub fn validate(&self) -> Result<()> {
        if ![1.0, 1.5].contains(&self.latent_scale)
            || [self.width, self.height]
                .iter()
                .any(|n| !(256..=2048).contains(n) || !n.is_multiple_of(8))
            || self.upscale_model.as_ref().is_some_and(|s| !filename(s))
        {
            return Err(invalid_size());
        }
        let (w, h) = self.output_size();
        if w > 4096 || h > 4096 || u64::from(w) * u64::from(h) > 16_777_216 {
            return Err(invalid_size());
        }
        Ok(())
    }
}
fn invalid_size() -> Error {
    Error::Validation("La résolution finale doit rester entre 256 et 4096 pixels par côté. Réduis le format ou l’agrandissement.".into())
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Reference {
    pub enabled: bool,
    pub model: Option<String>,
    pub clip_vision: Option<String>,
    pub weight: f64,
}
impl Default for Reference {
    fn default() -> Self {
        Self {
            enabled: false,
            model: None,
            clip_vision: None,
            weight: 0.75,
        }
    }
}
impl Reference {
    pub fn validate(&self) -> Result<()> {
        if !self.weight.is_finite()
            || !(0.0..=1.5).contains(&self.weight)
            || self
                .model
                .as_ref()
                .is_some_and(|s| !filename(s) || !supported_adapter(s))
            || self.clip_vision.as_ref().is_some_and(|s| !filename(s))
        {
            return Err(Error::Validation(
                "La configuration IP-Adapter SDXL est invalide.".into(),
            ));
        }
        Ok(())
    }
}
fn filename(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 512
        && !s.starts_with(['/', '\\'])
        && !s.contains(':')
        && !s.chars().any(char::is_control)
        && !s.split(['/', '\\']).any(|p| p == "..")
}
pub fn supported_adapter(s: &str) -> bool {
    let s = s.to_ascii_lowercase();
    filename(&s) && s.contains("sdxl") && !s.contains("faceid")
}
pub fn reference_path(s: &str) -> bool {
    s == "%reference_image%"
        || s.strip_prefix("sherka/reference-")
            .and_then(|s| s.strip_suffix(".png [temp]"))
            .is_some_and(|s| uuid::Uuid::parse_str(s).is_ok())
}
pub fn catalog_names(info: &Value, node: &str, field: &str) -> Vec<String> {
    let input = &info[node]["input"]["required"][field];
    // Both legacy custom nodes and ComfyUI's newer COMBO schema.
    input[0]
        .as_array()
        .or_else(|| input[1]["options"].as_array())
        .map(|v| {
            v.iter()
                .filter_map(Value::as_str)
                .filter(|s| filename(s))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}
pub fn resolution(config: &Config) -> Resolution {
    config.resolution.clone().unwrap_or_else(|| Resolution {
        width: if config.advanced.enabled {
            config.advanced.size
        } else {
            768
        },
        height: if config.advanced.enabled {
            config.advanced.size
        } else {
            768
        },
        ..Default::default()
    })
}

/// Strip only the adjustments recorded by our exported template. The user's
/// original graph remains editable and is validated again by base_workflow.
pub fn base_template(mut graph: Value) -> Value {
    if let Some(nodes) = graph.as_object_mut() {
        nodes.retain(|id, _| !id.starts_with("sherka_"));
        for node in nodes.values_mut() {
            if let Some(original) = node
                .get_mut("_meta")
                .and_then(Value::as_object_mut)
                .and_then(|m| m.remove("sherka_original_inputs"))
            {
                let applied = node["_meta"]
                    .as_object_mut()
                    .and_then(|m| m.remove("sherka_applied_inputs"));
                if let Some(applied) = applied {
                    let current = node["inputs"].as_object_mut();
                    if let Some(current) = current {
                        for (key, value) in applied.as_object().into_iter().flatten() {
                            // Preserve intentional edits to the exported JSON.
                            if current.get(key) == Some(value) {
                                if let Some(previous) = original.get(key) {
                                    current.insert(key.clone(), previous.clone());
                                } else {
                                    current.remove(key);
                                }
                            }
                        }
                    }
                } else {
                    node["inputs"] = original;
                }
            }
        }
    }
    graph
}
fn require(info: &Value, classes: &[&str]) -> Result<()> {
    if let Some(name) = classes.iter().find(|c| info.get(**c).is_none()) {
        return Err(Error::Validation(format!(
            "Un node ComfyUI requis est absent : {name}"
        )));
    }
    Ok(())
}

pub fn workflow(
    config: &Config,
    scene: Scene,
    info: &Value,
    seed: u64,
    include_companion: bool,
    reference_image: Option<&str>,
    transparent: bool,
) -> Result<Value> {
    // Transparent generations are the studio portraits. Conversation scenes
    // keep their own composition, including full-body and side views.
    let prompts = illustration::final_prompts(
        config,
        scene,
        include_companion,
        transparent.then_some(crate::character_studio::PORTRAIT_NEGATIVE),
    )?;
    workflow_with_prompts(
        config,
        prompts,
        info,
        seed,
        include_companion,
        reference_image,
        transparent,
    )
}

/// Same graph as [`workflow`], with prompts already final (see
/// [`illustration::final_prompts`] and [`illustration::exact_prompts`]).
pub fn workflow_with_prompts(
    config: &Config,
    prompts: Scene,
    info: &Value,
    seed: u64,
    include_companion: bool,
    reference_image: Option<&str>,
    transparent: bool,
) -> Result<Value> {
    let mut base = config.clone();
    base.workflow = base.workflow.map(base_template);
    let source = base
        .workflow
        .clone()
        .unwrap_or_else(illustration::default_workflow);
    let mut graph = illustration::base_workflow(&base, prompts, info, seed)?;
    let size = resolution(&base);
    size.validate()?;
    let (sample_width, sample_height) = size.sampled_size();
    if transparent
        && [size.width, size.height, sample_width, sample_height]
            .iter()
            .any(|n| !n.is_multiple_of(64))
    {
        return Err(Error::Validation("LayerDiffuse nécessite des dimensions intermédiaires divisibles par 64. Choisis un format recommandé ou désactive le latent ×1,5.".into()));
    }
    let use_reference = include_companion && config.reference.enabled;
    let needs_order = base.resolution.is_some() || use_reference || transparent;
    let order = if needs_order {
        Some(illustration::ordered_samplers(&graph)?)
    } else {
        None
    };
    let nodes = graph.as_object_mut().unwrap();
    if let Some(order) = &order {
        // Explicit sizing controls the inter-pass latent, including templates
        // whose unused upscale node was previously bypassed.
        if base.resolution.is_some() {
            if size.latent_scale > 1.0 {
                require(info, &["LatentUpscaleBy"])?;
                nodes.insert("sherka_latent".into(),json!({"class_type":"LatentUpscaleBy","inputs":{
                    "samples":[order[0],0],"upscale_method":"bislerp","scale_by":size.latent_scale}}));
                nodes.get_mut(&order[1]).unwrap()["inputs"]["latent_image"] =
                    json!(["sherka_latent", 0]);
            } else {
                nodes.get_mut(&order[1]).unwrap()["inputs"]["latent_image"] = json!([order[0], 0]);
            }
        }
        let mut model = nodes[&order[0]]["inputs"]["model"].clone();
        if use_reference {
            require(
                info,
                &[
                    "IPAdapterModelLoader",
                    "CLIPVisionLoader",
                    "IPAdapterAdvanced",
                    "LoadImage",
                ],
            )?;
            let missing = || {
                Error::Validation("Choisis un IP-Adapter SDXL, son CLIP Vision et un portrait neutre avant de générer.".into())
            };
            let adapter = config.reference.model.as_deref().ok_or_else(missing)?;
            let clip = config
                .reference
                .clip_vision
                .as_deref()
                .ok_or_else(missing)?;
            let reference = reference_image
                .filter(|s| reference_path(s))
                .ok_or_else(missing)?;
            if !catalog_names(info, "IPAdapterModelLoader", "ipadapter_file")
                .iter()
                .any(|s| s == adapter)
                || !catalog_names(info, "CLIPVisionLoader", "clip_name")
                    .iter()
                    .any(|s| s == clip)
            {
                return Err(missing());
            }
            nodes.insert(
                "sherka_reference".into(),
                json!({"class_type":"LoadImage","inputs":{"image":reference}}),
            );
            nodes.insert(
                "sherka_ip_model".into(),
                json!({"class_type":"IPAdapterModelLoader","inputs":{"ipadapter_file":adapter}}),
            );
            nodes.insert(
                "sherka_clip_vision".into(),
                json!({"class_type":"CLIPVisionLoader","inputs":{"clip_name":clip}}),
            );
            nodes.insert("sherka_ip".into(),json!({"class_type":"IPAdapterAdvanced","inputs":{
                "model":model,"ipadapter":["sherka_ip_model",0],"clip_vision":["sherka_clip_vision",0],
                "image":["sherka_reference",0],"weight":config.reference.weight,"weight_type":"linear",
                "combine_embeds":"concat","start_at":0.0,"end_at":1.0,"embeds_scaling":"V only"}}));
            model = json!(["sherka_ip", 0]);
        }
        if transparent {
            require(
                info,
                &["LayeredDiffusionApply", "LayeredDiffusionDecodeRGBA"],
            )?;
            nodes.insert(
                "sherka_transparent".into(),
                json!({"class_type":"LayeredDiffusionApply","inputs":{
                "model":model,"config":"SDXL, Conv Injection","weight":1.0}}),
            );
            model = json!(["sherka_transparent", 0]);
        }
        if use_reference || transparent {
            for id in order {
                let node = nodes.get_mut(id).unwrap();
                node["inputs"]["model"] = model.clone();
            }
        }
    }
    let outputs: Vec<String> = nodes
        .iter()
        .filter(|(_, n)| n["class_type"] == "PreviewImage")
        .map(|(k, _)| k.clone())
        .collect();
    if transparent || size.upscale_model.is_some() {
        // A single final output avoids returning an intermediate preview.
        let [output] = outputs.as_slice() else {
            return Err(Error::Validation(
                "Ce workflow nécessite une seule sortie d’image finale.".into(),
            ));
        };
        let mut image = nodes[output]["inputs"]["images"].clone();
        if transparent {
            let order = order.as_ref().unwrap();
            let decoder = image[0].as_str().ok_or_else(invalid_size)?;
            if nodes.get(decoder).is_none_or(|n| {
                n["class_type"] != "VAEDecode" || n["inputs"]["samples"] != json!([order[1], 0])
            }) {
                return Err(Error::Validation(
                    "Le portrait transparent nécessite un décodage VAE du second sampler.".into(),
                ));
            }
            nodes.insert(
                "sherka_rgba".into(),
                json!({"class_type":"LayeredDiffusionDecodeRGBA","inputs":{
                "samples":[order[1],0],"images":image,"sd_version":"SDXL","sub_batch_size":1}}),
            );
            image = json!(["sherka_rgba", 0]);
        }
        if let Some(upscaler) = &size.upscale_model {
            require(
                info,
                &["UpscaleModelLoader", "ImageUpscaleWithModel", "ImageScale"],
            )?;
            if !catalog_names(info, "UpscaleModelLoader", "model_name").contains(upscaler) {
                return Err(Error::Validation(
                    "Le modèle d’upscale choisi est absent du serveur ComfyUI.".into(),
                ));
            }
            let (width, height) = size.output_size();
            if transparent {
                require(
                    info,
                    &[
                        "SplitImageWithAlpha",
                        "JoinImageWithAlpha",
                        "MaskToImage",
                        "ImageToMask",
                    ],
                )?;
                nodes.insert(
                    "sherka_split".into(),
                    json!({"class_type":"SplitImageWithAlpha","inputs":{"image":image}}),
                );
                nodes.insert(
                    "sherka_alpha_image".into(),
                    json!({"class_type":"MaskToImage","inputs":{"mask":["sherka_split",1]}}),
                );
                nodes.insert("sherka_alpha_scale".into(),json!({"class_type":"ImageScale","inputs":{"image":["sherka_alpha_image",0],"width":width,"height":height,"upscale_method":"bilinear","crop":"disabled"}}));
                nodes.insert("sherka_alpha_mask".into(),json!({"class_type":"ImageToMask","inputs":{"image":["sherka_alpha_scale",0],"channel":"red"}}));
                image = json!(["sherka_split", 0]);
            }
            nodes.insert(
                "sherka_up_model".into(),
                json!({"class_type":"UpscaleModelLoader","inputs":{"model_name":upscaler}}),
            );
            nodes.insert("sherka_upscale".into(),json!({"class_type":"ImageUpscaleWithModel","inputs":{"upscale_model":["sherka_up_model",0],"image":image}}));
            // A 4x model is also usable: resample its output to the promised 2x.
            nodes.insert("sherka_output_size".into(),json!({"class_type":"ImageScale","inputs":{
                "image":["sherka_upscale",0],"width":width,"height":height,"upscale_method":"lanczos","crop":"disabled"}}));
            image = json!(["sherka_output_size", 0]);
            if transparent {
                nodes.insert("sherka_join".into(),json!({"class_type":"JoinImageWithAlpha","inputs":{"image":image,"alpha":["sherka_alpha_mask",0]}}));
                image = json!(["sherka_join", 0]);
            }
        }
        nodes.get_mut(output).unwrap()["inputs"]["images"] = image;
    }
    for (id, node) in graph.as_object_mut().unwrap() {
        if let Some(original) = source.get(id) {
            // Only reversible sampling and graph links need metadata.
            // Never retain replaced prompts, LoRA lists or SaveImage paths.
            let keys: &[&str] = match node["class_type"].as_str() {
                Some("KSampler") => &[
                    "model",
                    "latent_image",
                    "steps",
                    "cfg",
                    "sampler_name",
                    "scheduler",
                ],
                Some("PreviewImage") => &["images"],
                _ => &[],
            };
            let mut previous = serde_json::Map::new();
            let mut applied = serde_json::Map::new();
            for key in keys {
                if original["inputs"][key] != node["inputs"][key] {
                    if let Some(value) = original["inputs"].get(key) {
                        previous.insert((*key).into(), value.clone());
                    }
                    if let Some(value) = node["inputs"].get(key) {
                        applied.insert((*key).into(), value.clone());
                    }
                }
            }
            if applied.is_empty() {
                continue;
            }
            if !node["_meta"].is_object() {
                node["_meta"] = json!({});
            }
            node["_meta"]["sherka_original_inputs"] = Value::Object(previous);
            node["_meta"]["sherka_applied_inputs"] = Value::Object(applied);
        }
    }
    illustration::validate_workflow(&graph)?;
    Ok(graph)
}

/// Offline, inspectable API template. No avatar bytes, conversation or keys.
pub fn template(config: &Config, transparent: bool) -> Result<Value> {
    let mut config = config.clone();
    config.enabled = false;
    config = config.validated()?;
    if !transparent && !config.reference.enabled && config.resolution.is_none() {
        return Ok(config
            .workflow
            .unwrap_or_else(illustration::default_workflow));
    }
    let checkpoint = config
        .checkpoint
        .get_or_insert_with(|| "%checkpoint%".into())
        .clone();
    let adapter = config
        .reference
        .model
        .get_or_insert_with(|| "%ipadapter_sdxl%".into())
        .clone();
    let clip = config
        .reference
        .clip_vision
        .get_or_insert_with(|| "%clip_vision%".into())
        .clone();
    let mut info = json!({});
    let graph = config
        .workflow
        .clone()
        .unwrap_or_else(illustration::default_workflow);
    for node in graph.as_object().ok_or_else(invalid_size)?.values() {
        if let Some(class) = node["class_type"].as_str() {
            info[class] = json!({});
        }
    }
    for class in NODE_CLASSES {
        info[*class] = json!({});
    }
    info["LatentUpscaleBy"] = json!({});
    info["CheckpointLoaderSimple"] = json!({"input":{"required":{"ckpt_name":[[checkpoint]]}}});
    let loras = config
        .loras
        .iter()
        .map(|l| l.name.clone())
        .chain(std::iter::once("PAseer-SDXL-LCM+Turbo.safetensors".into()))
        .collect::<Vec<_>>();
    info["LoraLoader"] = json!({"input":{"required":{"lora_name":[loras]}}});
    info["KSampler"] =
        json!({"input":{"required":{"sampler_name":[[config.advanced.sampler_name]]}}});
    info["IPAdapterModelLoader"] = json!({"input":{"required":{"ipadapter_file":[[adapter]]}}});
    info["CLIPVisionLoader"] = json!({"input":{"required":{"clip_name":[[clip]]}}});
    info["UpscaleModelLoader"] = json!({"input":{"required":{"model_name":[config.resolution.as_ref().and_then(|r|r.upscale_model.clone()).into_iter().collect::<Vec<_>>() ]}}});
    config.negative_prompt = "%negative_prompt%".into();
    config.appearance_negative_prompt.clear();
    workflow(
        &config,
        Scene {
            positive: "%prompt%".into(),
            negative: String::new(),
        },
        &info,
        0,
        true,
        Some("%reference_image%"),
        transparent,
    )
}
