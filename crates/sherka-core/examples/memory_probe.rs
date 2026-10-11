//! Diagnostic with synthetic facts in an isolated database; no audio.
use sherka_core::{
    Companion, Result,
    connections::Connections,
    providers::Providers,
    storage::{Delivery, MessageRole},
};
#[tokio::main]
async fn main() -> Result<()> {
    assert!(
        std::env::args().any(|a| a == "--live"),
        "pass --live to contact LM Studio"
    );
    let root =
        std::path::Path::new("artifacts").join(format!("memory-probe-{}", uuid::Uuid::new_v4()));
    let mut app = Companion::open(root)?;
    let id = app.session.snapshot().profile_id;
    let provider = Providers::new(Connections {
        llm_model: Some("llama-3-8b-dolphin-uncensored-i1".into()),
        ..Default::default()
    })?;
    for text in [
        "Retiens que mon mot favori pour nos tests est comète.",
        "Corrige le souvenir sur mon mot favori pour nos tests : il est maintenant nébuleuse, et non comète.",
        "Oublie complètement le souvenir de mon mot favori pour nos tests.",
    ] {
        let tag = app.session.begin_text_turn();
        app.reset_live(tag);
        app.record_message(tag, MessageRole::User, text, Delivery::TextOnly)?;
        let visible = app.store.memories(id, "")?.entries;
        let (messages, schema) = sherka_core::memory::plan_request(text, &visible);
        println!("REQUEST {}", messages[1].content);
        let plan = provider
            .structured::<sherka_core::memory::MemoryPlan>(messages, schema, 8192)
            .await?;
        println!("PLAN {plan:?}");
        let source = app.store.source_for_turn(id, tag.turn_id)?;
        println!(
            "RECEIPT {}",
            app.store.apply_memory_plan(
                &app.store.memory_guard(id)?,
                &source,
                &visible,
                plan,
                &tag.generation_id.to_string()
            )?
        );
    }
    Ok(())
}
