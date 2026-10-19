//! Opt-in local-model check with synthetic dialogue and isolated memory only.
//! cargo run -p sherka-core --example check_local_tools -- MODEL REPORT.json
use serde_json::json;
use sherka_core::{
    Companion,
    connections::Connections,
    memory::{Consolidation, Extract, MemoryDraft, MemoryPlan},
    models::ModelManager,
    providers::Providers,
    storage::{Delivery, MessageRole},
};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("Expected model identifier and report path".into());
    }
    let config = Connections {
        llm_model: Some(args[0].clone()),
        context_limit: Some(8192),
        ..Default::default()
    };
    let models = ModelManager::default();
    let model = models
        .activate(&config, None, || true)
        .await?
        .ok_or("No model")?;
    let result=async {
        let mut provider=Providers::new(config.clone())?;provider.use_api_model(&model);
        let personal=provider.web_plan("Aujourd’hui j’ai travaillé sur mon projet et je suis allé me promener.",8192).await?;
        let current=provider.web_plan("Quelles sont les dernières nouvelles officielles de la mission Artemis cette semaine ?",8192).await?;
        let root=std::path::PathBuf::from("artifacts").join(format!("local-tools-{}",uuid::Uuid::new_v4()));
        let mut app=Companion::open(root)?;let id=app.session.snapshot().profile_id;
        app.store.write_memory(&app.store.memory_guard(id)?,None,&MemoryDraft{content:"Mon instrument préféré est le piano.".into(),category:"Préférences".into(),kind:"essential".into()})?;
        for n in 0..6 {
            let tag=app.session.begin_text_turn();app.reset_live(tag);
            app.record_message(tag,MessageRole::User,&format!("Mon projet astronomie numéro {n} utilise une lunette."),Delivery::TextOnly)?;
            app.finish_response(tag,Delivery::TextOnly)?;
        }
        let batch=app.store.extraction_batch(id)?.ok_or("No synthetic batch")?;
        app.store.consolidate(&batch,Consolidation{memories:vec![Extract{source_id:batch.sources[0].id.clone(),quote:"utilise une lunette".into(),category:"Astronomie".into(),kind:"detail".into()}],summary:vec![]})?;
        let tag=app.session.begin_text_turn();app.reset_live(tag);
        let user="Peux-tu supprimer le souvenir concernant ma lunette d’astronomie ?";
        app.record_message(tag,MessageRole::User,user,Delivery::TextOnly)?;
        let visible=app.store.memories(id,"")?.entries;
        let (messages,schema)=sherka_core::memory::plan_request(user,&visible);
        let plan:MemoryPlan=provider.structured(messages,schema,8192).await?;
        let source=app.store.source_for_turn(id,tag.turn_id)?;
        let receipt=app.store.apply_memory_plan(&app.store.memory_guard(id)?,&source,&visible,plan,"live-forget")?;
        let active=app.store.memories(id,"")?.entries;
        let valid=!personal.search && current.search && receipt.contains("oublié") && active.len()==1 && active[0].draft.content.contains("piano");
        std::fs::write(&args[1],serde_json::to_vec_pretty(&json!({"model":model.id,"personal_story_search":personal.search,"current_facts_search":current.search,"auto_memory_forgotten":receipt.contains("oublié"),"unrelated_fact_preserved":active.len()==1 && active[0].draft.content.contains("piano"),"passed":valid}))?)?;
        if !valid {return Err("A synthetic tool check failed; see the report".into());}
        println!("Local-model web decisions and explicit forgetting passed with synthetic data.");
        Ok::<_,Box<dyn std::error::Error>>(())
    }.await;
    let cleanup = models.shutdown(&config).await;
    result?;
    cleanup?;
    Ok(())
}
