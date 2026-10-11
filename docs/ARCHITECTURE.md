# Architecture

Tauri 2 hosts a Windows desktop application. Rust owns SQLite, providers and audio; TypeScript renders the panel and transparent widget. Production embeds the frontend with no internal application HTTP server.

## Profiles and persistence

`sherka-core` uses a settings database and one database per UUID profile. Identity, revision checks and schema versions reject stale edits and incompatible databases. Imports validate bounded JSON and images, re-encode PNG and reject executable or external resources. Exports exclude history, memories and secrets.

Essential and topical memories use profile-local FTS5. Structured proposals are validated against revisions, epochs and quoted sources before a transaction. Corrections/forgetting exclude obsolete exchanges from future context while retaining archives. Automatic extraction requires review.

The interface locale is a global settings preference. TypeScript and Rust tray labels share the same seven-language catalog; only authored UI text is translated before inserting dynamic values. Custom personality, dialogue, memories and provider identifiers remain opaque. Custom profile language is fixed at creation, and the conversation prompt follows it.

The original Sherka's UUID is registered in settings. Her built-in personality and instructions follow the interface locale from seven authored presets, preserving provider choices. Personalization detaches the preset; copies and imports are independent. Locale changes bypass voice cancellation unless they change the active built-in character's language. That case cancels the old generation and stops capture before continuing with the new prompt language.

## Sessions and audio

Capture and response belong to a profile and generation. Switching, interruption and pause cancel obsolete work; late events cannot reactivate it. Voice commands release their gate while awaiting VoiceStudio, keeping settings and exit responsive. A separate pending token handles pre-session waits.

CPAL capture/output feeds Silero speech confirmation, VoiceStudio transcription, the selected provider's SSE stream and VoiceStudio TTS. LM Studio, OpenAI Chat Completions and Anthropic Messages normalize final text and usage into the same pipeline; reasoning events are excluded. Sonora provides echo processing in speaker mode. Text and speech buffers are independent. TTS prepares upcoming passages during playback; short verses are grouped. Bounded PCM queues can transition in one output callback. Heard receipts follow completed playback. Cancellation clears queued work.

Companions serialize provider, full API base, model and budget preferences without keys. Windows Credential Manager owns provider/base-scoped credentials; only the panel can query key presence, replace a key or remove it. Remote requests use HTTPS except loopback, do not follow redirects and never forward provider credentials to VoiceStudio. Context discovery precedes local model retirement, and remote adapters never call local model load/unload endpoints.

VoiceStudio synthesizes a passage before returning audio, so synthesis can exceed the available buffer. No token-level speech generation or zero-latency guarantee is claimed.

Context budgets use loaded capacity, reply reserve and margin. UTF-8 estimation is conservative, not an exact tokenizer count. History and secondary memories are bounded; reasoning is excluded from speech and dialogue storage.

## Lifecycles

Optional web search runs before final generation, within the same cancellable profile/generation operation. Explicit requests skip the local structured search planner; automatic mode passes only the current question to it. The Rust client uses a fixed DuckDuckGo Lite endpoint, bounded HTML parsing and no arbitrary-page fetching. Evidence is included as untrusted user-role text, with whole sources bounded against the model context. Search receipts and the sources actually supplied are stored in the assistant's history row (schema 4), separate from memories and profile exports. The panel can open a source selected by profile/message/index; it cannot supply an arbitrary URL. The widget cannot invoke that command.

`ModelManager` serializes loading, resolves aliases before deletion, rejects ambiguity and preserves foreign instances. Shutdown waits for loads, stops audio, saves geometry, unloads its instance and verifies the catalog. Failure offers retry/stay/explicit exit.

`voice_startup` shares a cancellable readiness attempt; root cancellation prevents late launch during exit. `service_process` locates installed VoiceStudio and minimizes only a new window. No additional Python runtime or model download manager is added. LM Studio startup is independent.

## Updates and boundaries

`updates` exposes status/check/install only to the panel. No arbitrary URL, path or process command comes from the webview. GitHub checks are manual and use the official fixed repository. The widget has no updater permissions. Signed downloads are verified before shutdown.

Installed copies use NSIS. Directly launched copies prepare a verified new executable as a temporary helper. It binds the target to the old process, verifies the signature and version, waits for clean exit, then replaces the file with a backup and restarts. User databases are not replaced.

A restrictive CSP limits the webview. Provider responses and release notes are untrusted text. Update test overrides are debug-only, loopback-only, with isolated data.

References: [capabilities](https://v2.tauri.app/security/capabilities/), [updater](https://v2.tauri.app/plugin/updater/), [WebView2 testing](https://playwright.dev/docs/webview2), [contracts](CONNECTIONS.md).
