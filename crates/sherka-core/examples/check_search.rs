//! Explicit, non-personal search probe. The key stays in the OS vault.
//! cargo run -p sherka-core --example check_search -- brave
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use sherka_core::search_provider::{self, Config, Provider};
    let provider = match std::env::args().nth(1).as_deref() {
        Some("brave") => Provider::Brave,
        Some("tavily") => Provider::Tavily,
        _ => return Err("Expected brave or tavily".into()),
    };
    let results = search_provider::search(
        &Config {
            provider,
            max_results: 2,
            ..Default::default()
        },
        "Rust programming language official documentation",
        "fr",
    )
    .await?;
    println!("Search provider reachable: {} sources", results.len());
    Ok(())
}
