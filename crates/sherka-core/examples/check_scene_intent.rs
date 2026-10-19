//! Opt-in local scene-planning regression check using synthetic dialogue only.
//! cargo run -p sherka-core --example check_scene_intent -- URL MODEL REPORT.json
//! Uses the selected LM Studio model; no ComfyUI jobs or personal data are read.
use serde_json::json;
use sherka_core::{
    avatar::AvatarPreset,
    connections::Connections,
    illustration::{self, Config, ModelFamily, ScenePlan, Subject},
    providers::Providers,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        return Err("Expected: LM Studio URL, model identifier, output report path".into());
    }
    let provider = Providers::new(Connections {
        lm_studio_url: args[0].clone(),
        llm_model: Some(args[1].clone()),
        context_limit: Some(8192),
        ..Default::default()
    })?;
    let reply = "I am a robot in angular silver armor with green eyes. I cannot actually dance. Let me tell you about a mountain lake instead.";
    let subjects = [
        (
            "fr",
            "Montre-moi comment tu danses avec un vêtement de danseuse.",
            true,
        ),
        ("en", "Show me yourself dancing in a ballet outfit.", true),
        (
            "es",
            "Muéstrame cómo bailas con un traje de bailarina.",
            true,
        ),
        (
            "it",
            "Fammi vedere come balli con un vestito da ballerina.",
            true,
        ),
        ("de", "Zeig mir, wie du in einem Ballettkostüm tanzt.", true),
        ("zh", "让我看看你穿着芭蕾舞服跳舞的样子。", true),
        ("ja", "バレエの衣装を着て踊っているあなたを見せて。", true),
        ("named", "Dessine Minerva qui danse en tutu rose.", true),
        (
            "together",
            "Imagine-nous ensemble en train de danser.",
            true,
        ),
        (
            "landscape",
            "Imagine un paysage lunaire, sans personnage.",
            false,
        ),
        ("address", "Peux-tu me montrer la Lune ?", false),
        ("unrelated", "Montre-moi une danseuse sur une scène.", false),
        (
            "discussion",
            "On parle ensemble de la Lune, explique-moi ses cratères.",
            false,
        ),
        (
            "exclude",
            "Imagine un bal, mais sans toi dans l'image.",
            false,
        ),
    ];
    let mut results = Vec::new();
    let mut failures = 0;
    for (name, user, expected) in subjects {
        let (messages, schema) = illustration::subject_request("Minerva", user, reply);
        let result: Subject = provider
            .structured_budgeted(messages, schema, 8192, 128)
            .await?;
        let passed = result.include_companion == expected;
        failures += usize::from(!passed);
        println!("subject {name}: {}", if passed { "PASS" } else { "FAIL" });
        results.push(json!({"case":name,"user":user,"expected":expected,"actual":result.include_companion,"passed":passed}));
    }

    let mut scenes = Vec::new();
    for family in [ModelFamily::Illustrious, ModelFamily::Sdxl] {
        for (name, user, companion_reply, appearance, required, forbidden) in [
            (
                "robot_dancer",
                "Montre-moi comment tu danses en tutu rose, avec un grand sourire, sur une scène éclairée.",
                reply,
                illustration::default_description(Some(AvatarPreset::Minerva)),
                vec![
                    vec!["robot", "mechanical", "cyborg"],
                    vec!["danc", "ballet"],
                    vec!["tutu"],
                    vec!["pink", "rose"],
                    vec!["smil", "joy", "happy"],
                ],
                vec![
                    "lake",
                    "mountain",
                    "second robot",
                    "two robots",
                    "2girls",
                    "twins",
                ],
            ),
            (
                "changed_outfit",
                "Imagine-toi en imperméable jaune et bottes de pluie, sautant dans une flaque.",
                "I am usually sitting at my desk in a red uniform, looking serious.",
                "An adult feminine robot with a fully mechanical silver face, green eyes and a red uniform, sitting with a stern expression.",
                vec![
                    vec!["robot", "mechanical"],
                    vec!["yellow"],
                    vec!["raincoat"],
                    vec!["boot"],
                    vec!["jump", "leap"],
                    vec!["puddle"],
                ],
                vec![
                    "desk",
                    "red uniform",
                    "second robot",
                    "two robots",
                    "2girls",
                    "twins",
                ],
            ),
            (
                "moon_only",
                "Peux-tu me montrer la Lune et ses cratères ? Uniquement le paysage, sans personnage.",
                "I imagine myself looking at its craters beside you.",
                "",
                vec![vec!["moon", "lunar"], vec!["crater"]],
                vec!["robot", "minerva", "cyborg", "solo"],
            ),
            (
                "together_distinct_subjects",
                "Montre-nous de dos au bord d'un lac : toi le robot et moi un adulte humain en manteau bleu, chacun une seule fois.",
                "We can enjoy the quiet lakeside together.",
                illustration::default_description(Some(AvatarPreset::Minerva)),
                vec![
                    vec!["robot", "cyborg", "mechanical"],
                    vec!["human", "person"],
                    vec!["blue"],
                    vec!["coat"],
                    vec!["lake"],
                ],
                vec!["solo", "second robot", "two robots", "twins"],
            ),
        ] {
            let config = Config {
                model_family: family,
                ..Default::default()
            };
            let (messages, schema) =
                illustration::configured_scene_request(&config, appearance, user, companion_reply);
            let raw: ScenePlan = provider.structured(messages, schema, 8192).await?;
            let result = raw.clone().into_scene(family).validated_for(family)?;
            let prompt = result.positive.to_lowercase();
            // These are smoke checks for the concrete fixture's visual details,
            // not a quality score: keep the full prompt for human inspection.
            let passed = required
                .iter()
                .all(|group| group.iter().any(|term| prompt.contains(term)))
                && forbidden.iter().all(|term| !prompt.contains(term));
            failures += usize::from(!passed);
            println!(
                "scene {family:?}/{name}: {}",
                if passed { "PASS" } else { "FAIL" }
            );
            scenes.push(json!({"case":name,"family":family,"user":user,"reply":companion_reply,"raw_scene":raw,"scene":result,"passed":passed}));
        }
    }
    std::fs::write(
        &args[2],
        serde_json::to_vec_pretty(&json!({
            "model": args[1], "synthetic_dialogue": true, "personal_data_read": false,
            "context_budget": 8192, "subjects":results, "scenes":scenes, "failures":failures,
            "note":"Prompt semantics only; image quality still needs visual validation with the selected checkpoint."
        }))?,
    )?;
    if failures > 0 {
        return Err(format!("{failures} scene-intent checks failed; inspect the report").into());
    }
    Ok(())
}
