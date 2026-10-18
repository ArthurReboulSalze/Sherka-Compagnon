# Architecture

Tauri 2 hosts a Windows desktop application. Rust owns SQLite, providers and audio; TypeScript renders the panel and transparent widget. Production embeds the frontend with no internal application HTTP server.

## Profiles and persistence

`sherka-core` uses a settings database and one database per UUID profile. Identity, revision checks and schema versions reject stale edits and incompatible databases. Imports validate bounded JSON and images, re-encode PNG and reject executable or external resources. Exports exclude history, memories and secrets.

Essential and topical memories use profile-local FTS5. Structured proposals are validated against revisions, epochs and quoted sources before a transaction. Corrections/forgetting exclude obsolete exchanges from future context while retaining archives. Automatic extraction requires review.

Explicit conversation clearing cancels the current generation and extraction, retires the audio input session, then deletes all messages, source quotations, summaries and unapproved/retired memories in that companion's database. Active approved facts survive unless the user selects the separate erase-facts option. The transaction advances the memory epoch and rebuilds FTS indexes; secure deletion, vacuum and WAL truncation remove old database pages. Individual fact deletion also removes the row from exports and excludes past exchanges from recall to avoid immediately relearning it. Copies previously exported or backed up are outside this operation.

Whole-command privacy requests are matched locally in seven languages, including `/clear`; quoted, negated, conditional and mixed requests do not trigger deletion. Typed commands work without a configured model or speech engine. Spoken acknowledgements are ephemeral and never enter dialogue history. Hermes bypasses this command handler. Its panel action clearly limits deletion to Sherka's local history, stops an owned active run and retires client session references; it does not delete data on the Hermes server.

The interface locale is a global settings preference. TypeScript and Rust tray labels share the same seven-language catalog; only authored UI text is translated before inserting dynamic values. Custom personality, dialogue, memories and provider identifiers remain opaque. Custom profile language is fixed at creation, and the conversation prompt follows it.

The original Sherka's UUID is registered in settings. Her built-in personality and instructions follow the interface locale from seven authored presets, preserving provider choices. Personalization detaches the preset; copies and imports are independent. Locale changes bypass voice cancellation unless they change the active built-in character's language. That case cancels the old generation and stops capture before continuing with the new prompt language.

## Sessions and audio

Capture and response belong to a profile and generation. Switching, interruption and pause cancel obsolete work; late events cannot reactivate it. Voice commands release their gate while awaiting VoiceStudio, keeping settings and exit responsive. A separate pending token handles pre-session waits.

CPAL capture/output feeds Silero speech confirmation, VoiceStudio transcription, the selected provider's SSE stream and VoiceStudio TTS. LM Studio, OpenAI Chat Completions and Anthropic Messages normalize final text and usage into the same pipeline; reasoning events are excluded. Sonora provides echo processing in speaker mode. Text and speech buffers are independent. TTS prepares upcoming passages during playback; short verses are grouped. Bounded PCM queues can transition in one output callback. Heard receipts follow completed playback. Cancellation clears queued work.

Companions serialize provider, full API base, model and budget preferences without keys. Windows Credential Manager owns provider/base-scoped credentials; only the panel can query key presence, replace a key or remove it. Remote requests use HTTPS except loopback, do not follow redirects and never forward provider credentials to VoiceStudio. Context discovery precedes local model retirement, and remote adapters never call local model load/unload endpoints.

VoiceStudio synthesizes a passage before returning audio, so synthesis can exceed the available buffer. No token-level speech generation or zero-latency guarantee is claimed.

Transient input-buffer congestion is distinct from a disconnected device. A full callback queue rebuilds capture/echo state and discards stale frames without clearing playback. A full desktop event queue similarly resets input dispatch. Recovery drops any incomplete utterance and requires a full silence interval before accepting fresh speech, so a partial command is never sent to an agent. Transcription backpressure or a closed recognizer cancels only that utterance; microphone consent remains active. Real device errors and default-device changes still stop capture explicitly. Headphone and playback-only sessions do not enqueue unused echo-reference frames.

`audio-events.log` in the application data directory records fixed lifecycle events, version, timestamps, character counts and internal device failures. It contains no recording, transcript, prompt, profile/device name, server address or credential. It rotates at 128 KiB to one previous file. It is diagnostic evidence, not a replayable conversation, and is excluded from publication.

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

## Hermes agent mode

`hermes` is a distinct remote-agent provider. Native session SSE carries user text only: local prompt construction, web planning, memory operations, model/context management and emotional-avatar behavior are bypassed. `Snapshot.companion_capabilities` exposes `local_personality`, `local_memory` and `local_emotions`; all are false in Hermes mode. The expression renderer and stream adapter honor this contract; future generators must do so too.

The official `/api/sessions` and session-chat stream APIs own the transcript. Device-local SQLite mappings isolate each companion and profile endpoint and retain the server’s current session after compression while keeping the displayed transcript stable. `/new` rotates the mapping without deleting history or remote memory. Profile exports contain connection preferences only. Keys are scoped to the exact routed endpoint in Windows Credential Manager; technical events and tool results never enter TTS. Approval responses are tied to the active generation and only offer once/deny.

The `/stop` shortcut and widget button interrupt the owned run through `POST /v1/runs/{run_id}/stop`, then poll its status for confirmation. They never stop a server or service. A per-client async gate prevents another Hermes run from starting while cancellation is unresolved. Failed cancellation retains the run handle for retry; the UI reports uncertainty instead of claiming success.

## Avatar expressions

Profile schema 6 stores six optional expression overrides in `avatar_emotions`; the existing `profile.avatar` remains neutral. The seven-state enum is closed. An explicit null override suppresses a bundled expression. The Sherka preset uses bundled asset URLs so snapshots do not copy six large PNGs on every refresh. Upgrading an existing Sherka with no custom portrait adopts this pack once; custom portraits remain untouched. Neutral and custom expression bytes are validated PNG/WebP, normalized and stored locally. Imports validate all images before creating a new profile, use a 48 MiB aggregate JSON limit, and accept export versions 1 and 2. Version 2 preserves the bundled preset reference and custom image overrides. Voice remains in that same profile’s connection configuration and is edited from the avatar UI.

The bundled preset enum also includes Botsy, Alba, Kit and Minerva, each with seven images. A persisted one-time initialization flag adds missing starter profiles without restoring profiles subsequently deleted by the user. Their personality is localized at creation; model and voice choices remain unconfigured. The original built-in Sherka retains its interface-language synchronization.

Alba retains the serialized `candis` preset identifier for compatibility with existing profiles and exports; her images resolve to the new Alba pack. On startup, an unchanged shipped Candis name/personality upgrades to Alba, with only the identity word changed in the localized text. User-edited names/personality and all profile IDs, settings, images, history and memory are preserved. Minerva's bundled image URLs point to the V2 pack. No database schema change is needed.

The Companions page groups identity, avatar/voice, provider, model, speech and web-search preferences. Search mode is stored per profile; older profiles inherit the previous global preference until edited. Hermes always resolves this mode to off. Saving an unrelated profile field rebases a pending connection draft only when its persisted connection values are unchanged.

When images are available, `prepare_context_with_avatar` adds a short expression instruction inside the existing context budget. If mandatory input leaves too little room for this optional instruction, the turn stays neutral rather than failing or truncating the user’s message. A bounded `ExpressionStream` removes the leading `[[emotion:STATE]]` header across chunk boundaries before text reaches the live reply, history or `SpeechText`. Invalid or absent metadata stays neutral. No separate inference request is added. An empty marker-only reply reports an empty response. Hermes bypasses both prompting and parsing. Expression updates use the same operation tag as text and audio; completion, cancellation and profile switch reset neutral. The widget decodes a new image before a 500 ms opacity crossfade, with at most two layers and only the latest pending expression. Same-character refreshes preserve these image nodes so cancellation can fade back to neutral without resetting the animation. Switching characters discards the previous layers. Reduced-motion preferences skip the fade; hover controls and dragging stay independent.
