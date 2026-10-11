//! Explicit one-off user-requested preset creation. Does not activate a mic,
//! load a model, switch the current companion or change Sherka's settings.
use sherka_core::{Result, model::ProfileDraft, storage::Store};
fn main() -> Result<()> {
    let root = std::env::args()
        .nth(1)
        .expect("usage: create_orion <data-directory>");
    let store = Store::open(root)?;
    if store.list()?.iter().any(|p| p.details.name == "Orion") {
        println!("Orion existe déjà ; aucune modification.");
        return Ok(());
    }
    let sherka = store.profile(store.settings()?.active_profile.expect("profil actif"))?;
    let mut connections = sherka.details.connections;
    connections.llm_model = Some("gemma4-12b-qat-uncensored-hauhaucs-balanced".into());
    connections.llm_model_key = connections.llm_model.clone();
    connections.reasoning = Some("off".into());
    connections.tts_voice = None; // Choose Orion’s own VoiceStudio voice in the application.
    let draft=ProfileDraft{
        name:"Orion".into(),
        personality:"Orion, gardien des cartes stellaires. Curieux, observateur et malicieux, avec un humour discret. Tu tutoies ton interlocuteur et vas droit au but. Tu poses parfois une question éclairante, sans en ajouter systématiquement. Les images d'astronomie restent occasionnelles. Tu incarnes ces traits sans te présenter ni les énumérer spontanément.".into(),
        instructions:"Réponds en français sauf demande contraire. Reste concis et concret. Distingue les faits, les hypothèses et les souvenirs confirmés. Si tu ne sais pas, dis-le simplement. Ne répète pas ta nature ni ton rôle au fil des réponses.".into(),
        accent:"teal".into(),connections,
    };
    let image = include_bytes!("../../../public/assets/orion.png");
    let profile = store.create(draft, Some(image))?;
    println!(
        "{}",
        serde_json::to_string(
            &serde_json::json!({"id":profile.id,"name":profile.details.name,"model":profile.details.connections.llm_model,"messages":profile.message_count,"memories":profile.memory_count})
        )?
    );
    Ok(())
}
