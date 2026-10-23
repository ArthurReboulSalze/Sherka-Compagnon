//! Opt-in local LLM check: URL MODEL_KEY OUTPUT.json. Uses a synthetic concept.
use sherka_core::{
    character_studio::{self, Draft},
    connections::Connections,
    i18n::Language,
    illustration::ModelFamily,
    models::ModelManager,
    providers::Providers,
};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().skip(1).collect();
    if a.len() != 3 {
        return Err("Expected local LM Studio URL, model key and output JSON".into());
    }
    let connections = Connections {
        lm_studio_url: a[0].clone(),
        llm_model: Some(a[1].clone()),
        llm_model_key: Some(a[1].clone()),
        ..Default::default()
    };
    let manager = ModelManager::default();
    let result:Result<(),Box<dyn std::error::Error>>=async{
        let model=manager.activate(&connections,None,||true).await?.ok_or("No model")?;
        let mut provider=Providers::new(connections.clone())?;provider.connections.llm_model=Some(model.id.clone());
        let (messages,schema)=character_studio::request("Une exploratrice robotique adulte, calme et curieuse. Visage entièrement en métal, yeux optiques verts, armure argentée avec quelques accents rouges. Un buste isolé de face avec la tête et les épaules entièrement visibles.",Language::Fr,ModelFamily::Sdxl,false)?;
        let draft:Draft=provider.structured_budgeted(messages,schema,model.loaded_context_length.unwrap_or(8192),3072).await?;
        draft.validate()?;std::fs::write(&a[2],serde_json::to_vec_pretty(&draft)?)?;
        println!("Validated synthetic French character sheet with English SDXL image descriptions and six emotion prompts.");
        Ok(())
    }.await;
    let cleanup = manager.shutdown(&connections).await;
    result?;
    cleanup?;
    Ok(())
}
