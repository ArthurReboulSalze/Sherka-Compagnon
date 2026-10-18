# Project status

Release version: **0.10.0**, October 18, 2026. Windows x64 and an initial Debian x64 port, Rust/Tauri 2, seven interface languages.

## In development — scene planning and memory controls

Companions now select an explicit Illustrious or SDXL image-model family, with tag-based or visual-prose prompts respectively. The full ComfyUI checkpoint catalog is available, with relative model/LoRA folder filters and an editable, authoritative negative prompt. External directories remain configured on the ComfyUI server. Disabling Turbo uses 16 steps on each sampler and doubles its original CFG (3.8 and 7.0 in the supplied graph). A separate short decision includes the companion's appearance only when the scene calls for it. Minerva's default description now explicitly requires a robotic face; customized descriptions are preserved.

Auxiliary JSON requests use a bounded output budget, optional reasoning disabled when supported, final-answer parsing and a single validated fallback for incompatible local JSON grammars. System instructions are consolidated at the beginning: real NeoHorse testing identified its template rejecting a system message after user data. Expressions now have short examples, recognized aliases and a conservative visible-answer fallback that ignores private thinking. These changes help small models without guaranteeing that every tone is classified correctly.

Memory optimization is available in Settings, Memory and explicit conversation commands. It proposes shorter facts/notes for review, then applies only after confirmation. Existing ids, source links, approval state, session and memory epochs are validated; omitted facts and conversation history survive. Semantic fidelity still needs the user's review. Conversation clearing now requires `/clear` followed by `/confirm-clear` (or recognized localized phrases), expires after three minutes and preserves saved facts. Selecting a companion card also activates that companion in the widget. The README begins with the supplied ecosystem diagram.

Validation: 118 Rust tests and three localization tests passed; TypeScript, formatting and Clippy passed, with all 868 authored UI messages covered in seven languages. Across native runs, all 32 default scenarios passed; 12 opt-in scenarios were skipped. Eight focused scenarios also passed on the optimized Windows executable. Three older UI fixtures were updated for visible-widget startup, card activation and read-only catalog discovery. Two native workers exited before their scenarios began; both scenarios passed on rerun.

Synthetic real-service checks passed with local Gemma 4 A4B, Dolphin 8B and NeoHorse: subject selection, both image-prompt families and joy expressions, including optional thinking on/off where supported. NeoHorse also produced a compact memory proposal preserving the synthetic tea preference and the unrelated cat/birth-date fact. A real SDXL Base generation through the production ComfyUI client produced a square mountain/lake landscape without a foreground character, using a custom negative prompt and the non-Turbo schedule. These tests used no personal conversation data or paid API calls; each temporary language-model instance was unloaded. Physical microphone/listening and a new Debian build were not tested for these changes. No new release has been published.

## 0.10.0 — inline cells, scenes and Linux

Version **0.10.0** adds automatic text-cell surface resizing and preserves the chosen height through repainting. Widget controls follow visible image bounds, including Kit's wide silhouette, and the active halo follows the character's accent. Six global themes color the interface. Text and image cells share the avatar's transparent window: text stays below the buttons, while the image can move and resize relative to the companion. Both follow the widget and toggle from the same buttons. Text supports typing/pasting, drops, optional spoken replies and IME composition. Widget errors are brief and cannot intercept the controls.

Classic companions can use a per-profile ComfyUI connection with an Illustrious checkpoint, physical description, optional LoRAs/strength sliders and automatic hidden Turbo LoRA selection. The supplied graph repairs prompt/CLIP wiring and generates square temporary previews. Planning starts as soon as text generation completes, while voice continues. One freely composed Illustrious scene prompt drives 1–8 sequential variants, shown in a looping slideshow with configurable timing and 500 ms crossfades. Only the latest user/reply pair and appearance are used; dialogue, memory and avatar emotions remain separate. Hermes is excluded. New input, profile/configuration changes or clearing invalidate older jobs. See [ComfyUI setup](COMFYUI.md).

ASR now preserves segment finals and resumed audio during recognition, with run/turn isolation and conservative filtering of specific subtitle-credit noise. Dropped image attachments are accepted only for vision-capable/explicitly confirmed models, normalized and sent for that turn without storing pixels in history, memory or exports. Per-character web search adds Brave, Tavily and SearXNG to DuckDuckGo, with OS-vault keys and bounded requests. See [web search](WEB_SEARCH.md).

Windows core validation: **110 Rust tests passed**, with the opt-in public DuckDuckGo request skipped. TypeScript, formatting, Clippy and the three localization tests passed. Twelve focused native scenarios passed across runs, covering the seven locales, provider and profile isolation, privacy, Hermes, long speech, image batches, cell geometry and readiness while speech is held. The optimized Windows executable passed the illustration/cell scenario with both speech enabled and the CI text-only setting. API search contracts are simulated; no live Brave/Tavily credits were used.

Earlier alpha.1 service validation passed a real synthetic exchange through GLM 5.3 Flash on Z.ai Coding Plan and the local ComfyUI server, producing a square scene with the installed Illustrious checkpoint and Turbo LoRA. A separate direct ComfyUI generation passed. Existing conversation history was not read; neither test used a physical microphone or listening assessment. Appearance consistency is approximate without reference-image conditioning.

Linux has Secret Service credentials, native source-link opening, Debian packaging and CI configuration. External voice services start separately and updates are manual. Debian 13 x86-64 passed 109 Rust tests, all static/localization checks, native Debian packaging and an extracted-package startup under Xvfb. No LM Studio or VoiceStudio was installed. Physical desktop/audio checks remain unvalidated; see [Linux development](LINUX.md).

Profile/settings schema is **8** and profile exports are **version 4**, accepting legacy versions 1–3. Development checks use isolated data and singleton identifiers. Windows updates use the signed installer/executable manifest; Linux packages update manually.

## Published release history

The application now has its own logo across Windows icons, the tray, sidebar and favicon. Companion portraits stay independent. Incremental builds rebuild icon resources when branding changes, and the English README features the new identity and a refreshed control-panel screenshot.

Hermes remote-agent mode connects to server profiles with protected keys, persistent conversations, streaming, interruption, explicit tool approvals and a widget **New conversation** action. Hermes owns personality, memory, tools and model budgets; emotional avatars are explicitly excluded. See [Hermes setup](HERMES.md).

This version also includes the fixes prepared for 0.6.1: Automatic search planning failures leave ordinary conversation available. Replies default to 8,192 tokens, with a configurable 64,000-token context fallback for remote models without metadata. Existing 1,024-token defaults are upgraded once (profile schema 5).

Earlier 0.6.1 checks: 78 Rust tests, three localization tests and six native scenarios passed. Native fixtures cover failed local/API search planning followed by normal streamed conversation, advertised output caps, missing context metadata, cancellation, restart, seven languages and startup recovery. Migration tests preserve profile choices, messages and memories. These regression checks use simulated services; there was no new billed inference or physical listening test.

Per-companion conversation providers include LM Studio, OpenRouter, Z.ai API, Z.ai Coding Plan, OpenAI, Anthropic and compatible APIs. Keys use Windows Credential Manager and are excluded from profiles. See [provider setup](PROVIDERS.md).

Startup recovery keeps signed updates available when settings or databases are incompatible or unreadable. Installed and no-install executables share data; keep their versions aligned. See [startup recovery](STARTUP_RECOVERY.md).

Optional DuckDuckGo search offers disabled, on-request and automatic modes, bounded snippets and persistent source links. Widget controls now dismiss on pointer exit, focus loss and opening the panel. See [web search](WEB_SEARCH.md).

The public release adds localized widget/tray controls, polished settings fields and a built-in Sherka personality in each language. The original Sherka follows the interface language; custom companions keep the language chosen at creation. Models and voices remain user-configured. See [languages](LANGUAGES.md).

## 0.9.0 — conversation privacy and audio recovery

Conversation clearing is available in the panel and through explicit requests in seven languages or `/clear`. Saved or approved facts survive by default; a separate checkbox includes them in deletion, and individual facts can be removed from Memory. Clearing removes local history, archive search, source quotations, summaries and pending suggestions, invalidates generation/extraction work and retires queued input. Typed privacy commands work without a configured provider. Spoken acknowledgements are not archived. Hermes retains its server-side memory and conversations; its panel action clears only local records and rotates the client session reference.

Temporary audio callback/event congestion now discards incomplete input and waits for a fresh pause without shutting down playback or microphone consent. Failed/closed transcription queues no longer disable the microphone. Headphone and output-only sessions avoid unused echo-reference work. Real device failures still report an error. Bounded technical logs support future diagnosis without recording conversation content or audio.

The reported interruption had a complete written reply and only partially confirmed playback. No historical runtime error established its exact trigger. The changes address independently identified failure paths; they do not establish the cause of that past incident or guarantee immunity to device/provider failures.

Validation: 99 Rust tests passed, with one opt-in public DuckDuckGo request skipped. TypeScript, formatting, Clippy and three localization tests passed; all 729 authored messages have seven languages.

The final optimized executable passed nine native scenarios: seven-language interfaces and presets, memory editing and restart, long spoken replies and audio-error recovery, device consent, a clean English screenshot, Hermes sessions/local clearing, confirmed `/stop`, and conversation/fact deletion including ephemeral speech receipts and offline commands. These use isolated data and simulated services; no physical microphone or real-model quality test was performed. Installer and portable update signatures verified and rejected altered data; package and binary audits passed. The installer was extracted for inspection without installing on the developer account.

## 0.8.1 — refreshed artwork

The tightly cropped golden logo supplies the application, Windows, tray, installer and uninstaller icons. Alba replaces Candis's artwork and display name, keeping the same localized personality traits. Minerva uses the new armored image pack with the same irritable personality. Both packs contain all seven expressions. The original Candis name/personality migrate at startup without replacing personal settings, histories or image overrides; edited names/personality text remain user-owned. The stored preset identifier stays compatible with 0.8.0. README images use fresh filenames, including a new control-panel capture.

Validation: three focused Rust tests passed, including upgrades in all seven languages with retained history, memory, models, voices and custom portraits. TypeScript, formatting, Clippy and three localization tests passed. The final no-install executable passed three native scenarios covering all bundled expression images, avatar voice selection, 500 ms crossfades, settings persistence and a clean English documentation capture. Both updater signatures verified and rejected altered data; installer and portable binary audits passed. The extracted application and installer icons are identical and use the V2 crop. The installer was inspected without installing on the developer account.

## 0.8.0 — companions and Hermes controls

The Companions page now groups identity, avatar/voice, provider, model, context, reasoning, speech and per-character web-search settings. General Settings retains interface, widget, audio-device and update preferences. The starter collection contains Sherka, Botsy, Candis, Kit and Minerva; Orion is no longer shipped as a starter example. Kit uses dry humor; Minerva is intentionally irritable and sharp-tongued. New starters are localized at creation and added once, so deleting one is persistent. Model and voice selection remain user-configured.

The avatar editor groups the VoiceStudio voice and seven static image states. All five bundled characters include neutral/joy/sadness/anger/surprise/reflection/tenderness images supplied for the project. Expressions crossfade over 500 ms after image decoding; rapid changes coalesce, and reduced-motion preferences disable the fade. Model-selected expressions use the existing reply stream, are stripped before speech/history and reset on completion or interruption. Hermes stays neutral. Schema 6 and profile export version 2 preserve custom images and the bundled preset reference. ComfyUI generation is still a future phase. After upgrading, use 0.8.0 or later for both installed and no-install copies. Version 0.7.1 does not understand the upgraded profiles.

The application uses the new orbital-star logo. Hermes has a visible widget stop button and typed `/stop` shortcut. Both target only the current remote task through the run API and check its terminal status. Unconfirmed cancellation stays retryable and blocks overlapping work. No server shutdown or restart is involved. Local web search, personality, memory and expressions remain bypassed.

A real synthetic voice check passed through VoiceStudio synthesis/transcription, Hermes web search and skill-list inspection, and reply synthesis. An independent harmless run was stopped with server confirmation. These checks used fresh server conversations and no physical microphone or playback. The previously reported voice confusion is consistent with an unexpected transcript found in the relevant history; without an original audio recording, its precise cause is not established.

Pre-release validation: 90 Rust tests passed (one opt-in public DuckDuckGo test skipped), three localization tests passed, and TypeScript/format/Clippy checks passed. Thirteen native scenarios plus a clean documentation capture passed across the focused runs. These cover seven languages, profile lifecycle/restart, catalogs, imports, providers, reasoning, expressions and 500 ms crossfades, widget hover controls, web search, Hermes sessions and stop retry, and the five starter companions. Older UI fixtures were updated for the moved controls, completed asynchronous saves and visible-window interaction. One WebView2 startup failed before its scenario began; the scenario passed on rerun.

The packaged 0.8.0 executable, extracted from the no-install ZIP, passed the avatar/crossfade, companion-settings and Hermes-stop scenarios with isolated data, including restart persistence. A clean English control-panel capture was refreshed for the README. One test worker exited before its scenario began; the avatar scenario passed on rerun. Both updater signatures passed verification and rejected altered data. The installer was extracted for inspection: its executable differs from the portable copy only by Tauri's expected installer marker. Publication and package audits found no private data or signing material. The installer was not executed on the developer account.

Validation for alpha.1: 88 Rust tests in total (87 in the workspace run, plus the added context-budget test in the focused seven-test avatar suite), three localization tests, and six native scenarios passed. Native checks cover all seven interface languages, the image editor, streamed emotion metadata, TTS input, cancellation, profile voices and Hermes exclusion. The optimized Windows executable also passed the avatar scenario with isolated data, including image changes, voice selection, metadata filtering, interruption and persistence after restart. Two native fixtures were adjusted to await completed saves and the short-answer TTS flush; an unrelated worker startup crash passed on rerun. Services were simulated; no physical microphone or real-model emotion-quality test was performed.

## Available

Transparent widget, hover/click controls, optional panel, independent profiles and voices, profile import/export, provider catalogs, streamed text with speech prepared ahead, cancellation and voice interruption, editable essential/topical memories, context budgets and capability-aware reasoning.

Installed VoiceStudio starts when its local service is stopped. Sherka waits for its API with timeout and retry, without waiting for every model to preload. Starting LM Studio is independent.

GitHub updates are checked on request. Downloads are signed; installed and no-install copies use separate paths. Voice and the managed model stop before replacement.

## Validation and limitations

0.7.0 checks: 81 Rust tests, three localization tests and 25 native scenarios passed. One Playwright worker exited before a scenario began; that scenario passed when rerun separately. Native Hermes coverage includes separate profile keys, user-only requests, streamed speech, approvals opened from Settings, interruption and a new conversation from the widget. Two real server profiles passed synthetic conversation checks, including cross-message recall on the first profile. Real Hermes inference was tested; speech used a VoiceStudio fixture, with no physical microphone/listening validation. The Windows installer and no-install package were built locally; both updater signatures passed verification and rejected altered data. The packaged release executable started with the widget visible and panel hidden, read the protected profile keys, then exited cleanly with isolated data. The installer was not executed on the developer account.

Core tests cover network contracts, sessions, memory, context, isolation and models. Native tests exercise Rust, SQLite and WebView2 with isolated data. Real-service tests are explicitly enabled.

For 0.6.0, native GLM 5.3 Flash streaming passed on OpenRouter and Z.ai Coding Plan using synthetic messages (345 input tokens, 3 output tokens per turn). Standard Z.ai API returned HTTP 429/code 1113 for the test account; subscription credit and API credit are separate. OpenAI and Anthropic adapters have contract coverage, without a live account test. Remote speech was validated against an isolated VoiceStudio fixture, not by a physical listening test.

0.6.0 validation: 76 Rust tests, three localization tests and 24 native UI scenarios passed; opt-in suites are skipped by default. Windows installer and no-install assets were built, both updater signatures verified with corruption rejected, and the release executable started and quit with isolated data. The installer itself was not executed on the developer account.

Headphone conversation and interruption were manually tested during development. Speaker echo cancellation exists, but reliability depends on hardware and placement. One real VoiceStudio cold start exceeded the timeout; a later attempt succeeded. That provider case needs further diagnosis.

Local diagnostics, personal databases and developer machine details are excluded from the repository and releases.

See [roadmap](ROADMAP.md), [contracts](CONNECTIONS.md) and [development](DEVELOPMENT.md).
