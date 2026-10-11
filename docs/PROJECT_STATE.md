# Project status

Public version: **0.5.0**, October 11, 2026. Windows x64, Rust/Tauri 2, seven interface languages.

Optional DuckDuckGo search offers disabled, on-request and automatic modes, bounded snippets and persistent source links. Widget controls now dismiss on pointer exit, focus loss and opening the panel. See [web search](WEB_SEARCH.md).

The public release adds localized widget/tray controls, polished settings fields and a built-in Sherka personality in each language. The original Sherka follows the interface language; custom companions keep the language chosen at creation. Models and voices remain user-configured. See [languages](LANGUAGES.md).

## Available

Transparent widget, hover/click controls, optional panel, independent profiles and voices, profile import/export, provider catalogs, streamed text with speech prepared ahead, cancellation and voice interruption, editable essential/topical memories, context budgets and capability-aware reasoning.

Installed VoiceStudio starts when its local service is stopped. Sherka waits for its API with timeout and retry, without waiting for every model to preload. Starting LM Studio is independent.

GitHub updates are checked on request. Downloads are signed; installed and no-install copies use separate paths. Voice and the managed model stop before replacement.

## Validation and limitations

Core tests cover network contracts, sessions, memory, context, isolation and models. Native tests exercise Rust, SQLite and WebView2 with isolated data. Real-service tests are explicitly enabled.

Headphone conversation and interruption were manually tested during development. Speaker echo cancellation exists, but reliability depends on hardware and placement. One real VoiceStudio cold start exceeded the timeout; a later attempt succeeded. That provider case needs further diagnosis.

Local diagnostics, personal databases and developer machine details are excluded from the repository and releases.

See [roadmap](ROADMAP.md), [contracts](CONNECTIONS.md) and [development](DEVELOPMENT.md).
