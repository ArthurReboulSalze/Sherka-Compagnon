# Project status

Public version: **0.6.0**, October 11, 2026. Windows x64, Rust/Tauri 2, seven interface languages.

Per-companion conversation providers include LM Studio, OpenRouter, Z.ai API, Z.ai Coding Plan, OpenAI, Anthropic and compatible APIs. Keys use Windows Credential Manager and are excluded from profiles. See [provider setup](PROVIDERS.md).

Startup recovery keeps signed updates available when settings or databases are incompatible or unreadable. Installed and no-install executables share data; keep their versions aligned. See [startup recovery](STARTUP_RECOVERY.md).

Optional DuckDuckGo search offers disabled, on-request and automatic modes, bounded snippets and persistent source links. Widget controls now dismiss on pointer exit, focus loss and opening the panel. See [web search](WEB_SEARCH.md).

The public release adds localized widget/tray controls, polished settings fields and a built-in Sherka personality in each language. The original Sherka follows the interface language; custom companions keep the language chosen at creation. Models and voices remain user-configured. See [languages](LANGUAGES.md).

## Available

Transparent widget, hover/click controls, optional panel, independent profiles and voices, profile import/export, provider catalogs, streamed text with speech prepared ahead, cancellation and voice interruption, editable essential/topical memories, context budgets and capability-aware reasoning.

Installed VoiceStudio starts when its local service is stopped. Sherka waits for its API with timeout and retry, without waiting for every model to preload. Starting LM Studio is independent.

GitHub updates are checked on request. Downloads are signed; installed and no-install copies use separate paths. Voice and the managed model stop before replacement.

## Validation and limitations

Core tests cover network contracts, sessions, memory, context, isolation and models. Native tests exercise Rust, SQLite and WebView2 with isolated data. Real-service tests are explicitly enabled.

For 0.6.0, native GLM 5.3 Flash streaming passed on OpenRouter and Z.ai Coding Plan using synthetic messages (345 input tokens, 3 output tokens per turn). Standard Z.ai API returned HTTP 429/code 1113 for the test account; subscription credit and API credit are separate. OpenAI and Anthropic adapters have contract coverage, without a live account test. Remote speech was validated against an isolated VoiceStudio fixture, not by a physical listening test.

0.6.0 validation: 76 Rust tests, three localization tests and 24 native UI scenarios passed; opt-in suites are skipped by default. Windows installer and no-install assets were built, both updater signatures verified with corruption rejected, and the release executable started and quit with isolated data. The installer itself was not executed on the developer account.

Headphone conversation and interruption were manually tested during development. Speaker echo cancellation exists, but reliability depends on hardware and placement. One real VoiceStudio cold start exceeded the timeout; a later attempt succeeded. That provider case needs further diagnosis.

Local diagnostics, personal databases and developer machine details are excluded from the repository and releases.

See [roadmap](ROADMAP.md), [contracts](CONNECTIONS.md) and [development](DEVELOPMENT.md).
