//! Explicit, read-only check of locally running services. No model is loaded.
use sherka_core::catalogs::CatalogClient;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let language = CatalogClient::new("http://127.0.0.1:1234")?
        .language_models()
        .await?;
    let voice = CatalogClient::new("http://127.0.0.1:3900")?
        .voices()
        .await?;
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({ "lm_studio": language, "voice_studio": voice })
        )?
    );
    Ok(())
}
