# Project status

Release version: **1.4.0**, October 28, 2026. Windows x64 and an initial Debian x64 port, Rust/Tauri 2, seven interface languages.

## Current state

- **Data formats:** profile/settings database schema **10** (speaker-attributed, mirrored three-way messages), legacy JSON profile export **version 5** (accepting 1–4), default companion export Sherka Companion Archive **v1** (`.sherka.zip`). Upgraded data must not be opened with older executables; keep installed and no-install copies on the same version.
- **Platforms:** Windows x64 is the tested platform (NSIS installer, portable ZIP, signed in-app updates). The Debian x64 package builds, starts under Xvfb and opens its widget window in a KDE Plasma (X11) session; physical Linux audio, compositor, tray and Secret Service unlocking remain unvalidated, and Linux updates are manual.
- **Test suite:** for 1.4.0, `cargo test --workspace --locked` passed 246 Rust tests on Windows, with one opt-in live DuckDuckGo test ignored.
- **Validation model:** core and native tests use isolated data (`SHERKA_DATA_DIR`) and simulated services; real-service checks are opt-in. Physical microphone, speaker-echo and listening behavior are validated manually by the author, not automatically.
- **Known limitations:** speaker echo cancellation depends on hardware and placement; one VoiceStudio cold start exceeded the startup timeout; token counts are conservative estimates; speech is synthesized per passage, not per token; image and emotion fidelity depend on the selected models.

See [roadmap](ROADMAP.md) for planned work. The sections below are the release log, newest first.

## 1.4.0 — three-way conversations

A guest companion can join the active one (the host) from the widget toolbar. The guest runs in its own transparent `guest` window with its own expressions and voice and borrows the host's provider, model and context settings, so one LM Studio model serves both. `sherka-core::trio` addresses each user message to one companion (name in the first three words, accent- and one-typo-tolerant, else random); after a configurable silence (off, 5, 8, 10 or 15 s; 8 by default) the other reacts, using a reply prepared while the first one was speaking, then the two chain replies with a short pause until the user acts, 20 replies have been said or both pass. Speech never overlaps. The thread is mirrored into both histories with the speaker recorded (schema 10); user lines are labelled in model context and the instructions name the three participants, so a companion no longer takes the user for the other one. The host greets a new guest with one spoken remark, and the relay also starts from that remark. Memory extraction still reads only the user's messages and is no longer cancelled by the companions' own replies. Hermes, voiceless companions and conversation illustrations are excluded. Also: a loading screen until VoiceStudio is ready, slow VoiceStudio starts reported as loading rather than errors (ten-minute budget, immediate report of an exited process), and conversation images off at startup and on companion switch.

Validation: 246 Rust tests (one opt-in live search test skipped), formatting, Clippy, localization and publication checks, and the Windows/Linux CI including native WebView2 scenarios with five new `trio:` scenarios on simulated services. The user tested invitation, turn-taking, the silence relay, the arrival remark, interruption and speaker awareness by voice in the Windows app over five builds.

## 1.3.4 — lossless WebP companion images

Bundled companion images were converted from PNG to lossless WebP with libwebp (49.8 MB to 31.9 MB, visible pixels verified identical for all 35 files); two PNG originals remain as test fixtures. `storage::normalize_image` now encodes lossless WebP, so studio results, imported pictures and edited avatars are stored as WebP, while PNG avatars from earlier versions are served with their own media type. ComfyUI uploads, vision requests and archive v1 entries are re-encoded as PNG with the same pixels; the database schema and JSON export version are unchanged. Validation: 172 Rust tests (one opt-in live search test skipped), formatting, Clippy, localization and publication checks, and the full Windows/Linux CI including native WebView2 scenarios. The user confirmed companion display, character creation, picture import and archives in the Windows app. On a second Debian 13 machine, the released `.deb` extracted without installation resolves all shared libraries, starts under Xvfb with isolated data and renders the WebP Sherka portrait with transparency in WebKitGTK 2.54; no physical Linux audio test was performed.

## 1.3.3 — microphone mute keeps the reply

Turning the microphone off now mutes capture only: the reply in progress finishes and is spoken, an utterance still being spoken is dropped, and the device closes once playback is idle. Global pause and Stop are unchanged. The large Hermes stop overlay over the desktop companion is removed; the toolbar stop and `/stop` remain. Validation: 170 Rust tests on Windows and 168 on Debian 13 (two Windows-only tests excluded; one opt-in live search test skipped on each), formatting, Clippy and localization checks; the user confirmed both behaviors in the Windows app. The released Debian package starts under Xvfb and opens its widget window in a KDE Plasma (X11) session; no physical Linux audio test was performed.

## 1.3.2 — image control and companion retouching

Character creation accepts a user picture as the neutral portrait (flattened onto the portrait gray, at most 1024 pixels); the sheet request then accepts an empty idea and attaches the picture only when the source model reports vision support. Each creation image exposes its exact ComfyUI prompts for verbatim editing (`Draft.prompt_overrides`), which take part in invalidation. Existing companions, bundled ones included, can be loaded into the studio to regenerate any emotion or the portrait and written back in place; identity, voice, history and memories are untouched. The conversation scene prompt is shown and editable below the desktop image cell, reused for variants and live mode until the next reply. Illustration settings are grouped into remembered collapsible sections with LoRA folder and search filters above the LoRA rows. Character creation moved below Companions in the sidebar.

Validation: 169 Rust tests (one opt-in live search test skipped), formatting, Clippy and seven-language checks passed. Native scenarios with simulated ComfyUI and model services covered prompt editing in the studio and widget, picture import, the full retouch of a bundled companion, the reorganized settings and catalog cache. Real image generation was checked by the user in the app, not by automated tests.

## 1.3.1 — faster voice turns

The first spoken passage starts at 25 characters and may end at a clause comma, then passages grow; a breath or click no longer restarts the end-of-turn silence countdown; interrupted speech fades out over 30 ms; streamed replies fail only after 180 seconds without data instead of after 180 seconds in total; provider requests share one connection pool; live text reaches the webviews at most every 40 ms. Each reply writes one `turn_timing` line with stage latencies to `audio-events.log`.

A first real session on the author's machine (Gemma 4 E4B in LM Studio, faster-whisper, automatic web search, OmniVoice) measured about 8–12 s from the end of speech to the first sound: roughly 3 s of end-of-utterance Whisper transcription, 0.5–2.5 s of preparation including the automatic search decision, 3–5 s before the first visible token although LM Studio finished prompt processing within a second (consistent with hidden model thinking, not proven) and about 1.5 s to synthesize the first passage. The first passage left 0.1–0.3 s after the first token. A per-turn model cache was tried and dropped: it could keep a stale context limit after a model reload.

Validation: 163 Rust tests (one opt-in live search test skipped), formatting, Clippy, localization checks and 27 native Windows scenarios passed in text-only mode with isolated data, plus 8 interruption-related scenarios after the fade. The user confirmed by ear that interruption works on a pre-release build and reported the click that the fade addresses; the fade itself is verified by unit test, not yet by a listening test.

## 1.3.0 — three-stage character creation

Character creation is in the sidebar between Memory and Settings, accessible through **Companions → Create with AI**. A connected classic companion's LLM proposes an editable sheet. Three supplied API workflows replace the previous creation path: an HD neutral portrait on uniform dark gray, Klein 4B editing for six emotions, then fixed RMBG-2.0 transparency. No IP-Adapter or Turbo is used for creation. The third stage has no settings.

The studio exposes the neutral checkpoint/family and Klein diffusion model, text encoder and VAE. The original base/refiner/detail sampling, 1024 output, Klein reference-latent editing and RMBG mask settings remain fixed. Model choices and catalog lists are cached. All three graphs are viewable read-only. Large image previews support review before approval. Frontal head/bust framing and a uniform gray background are requested in both the LLM sheet and rendering prompts.

Raw and transparent images are tracked separately. Neutral regeneration resets all dependents; an individual emotion retry invalidates its own alpha only. A model change invalidates affected stages, while personality edits retain images. Finished alpha images survive partial failures and only missing transparency jobs are retried. Publishing requires seven valid transparent images and creates a profile with empty history/memory and no inherited voice. Drafts remain in memory during the app session.

Conversation illustrations retain a separate per-companion IP-Adapter switch, recommended aspect ratios, explicit inter-pass latent scaling and selectable final ×2 upscaling. The bundled graph's formerly disconnected ×1.5 node is wired when selected. Desktop previews preserve aspect ratio. API workflow import/export remains in conversation illustration settings; creation uses its three fixed graphs. Export metadata keeps sampling changes reversible on reimport.

Search engines and API keys are configured globally in Settings; search access mode remains per companion. An existing provider configuration migrates once. Help & about sits below Settings and summarizes audio guidance, ComfyUI nodes/models and workflow requirements. All authored interface messages have translations in the seven supported languages.

Development checks for this release included 26 focused core tests for the changed modules, TypeScript, formatting, Clippy and localization/catalog checks. Live local ComfyUI testing rendered a 1024² neutral robot, a Klein joy edit and both transparent versions using the exact RMBG settings. Both final PNGs have roughly 60.7% transparent pixels, transparent corners and an opaque face. The user subsequently created a companion from beginning to end and confirmed that the complete flow works. Artistic quality and emotional fidelity still depend on the selected models.

Creation keeps conversation priority, cancellation tokens and targeted ComfyUI cleanup. It never globally interrupts the server. Image processing during final creation runs outside the shared conversation lock. The optimized Windows executable passed the native scenario in 17.8 seconds with isolated data and local service fixtures: all three stages, individual retries, preview dialogs, cached models, missing-node recovery, creation and restart. These fixture checks are separate from live rendering. Release validation includes Windows/Linux CI, signed Windows assets and an extracted Debian package startup check. Physical Linux audio and compositor behavior remain separate validation work.

## 1.2.0 — full-screen scenes and desktop responsiveness

Conversation images have larger desktop cells and a separate full-screen surface. The companion keeps its native drag/resize behavior in front, and leaving full screen restores the image cell. Character-specific negative appearance prompts are appended only when the scene includes the companion; the general negative prompt remains authoritative. Image pastes use the same capability checks as dropped attachments. First-image loading explains local ComfyUI startup and loading of the selected checkpoint/LoRAs.

A configured-model native regression reproduces a desktop freeze with the 1.1.1 executable. Activity notifications now run asynchronously; native visibility/geometry reads finish before acquiring engine locks, and geometry saves reject stale profile/surface state. The same stress scenario passes with the fix. Native Windows checks also cover full-screen stacking and controls, larger saved cells, clipboard attachments and text-cell resizing. Static/localization checks and focused core tests pass; Debian compilation and the core illustration/device tests pass. These checks use isolated data and simulated service responses, without physical audio or Linux desktop interaction claims.

## 1.1.1 — reachable widget controls

Widget controls stay visible while the pointer crosses transparent gaps between the avatar, toolbar and resize handle, including maximum-size Minerva. Automatic hiding still applies when leaving the avatar region, and overlapping image cells remain interactive. A native regression covers maximum-size controls and real pointer resizing.

## 1.1.0 — shared services and spontaneous conversation

VoiceStudio connection/transcription and VoiceStudio/ComfyUI local startup paths now live in global Settings. Each service has one **Start with Sherka** option. VoiceStudio starts its prepared Python backend without Electron; only servers launched by Sherka and their children close when Sherka exits. Existing and remote services stay independent. Voice engine, voice and speech delivery remain companion-specific.

Avatar and image-cell placement are saved per companion; illustrations overlap behind the avatar. Classic companions can optionally initiate a brief spoken invitation after a random idle interval, respecting conversation, typing, hidden-widget and pause states. One invitation waits for an actual reply before rearming. Hermes is excluded. Machine settings and desktop geometry are not exported with companion packs.

Validation: 144 Rust tests passed on Windows and 142 on Debian (one optional public-search test skipped on each), with localization/catalog checks and TypeScript compilation. Native scenarios exercised shared speech settings, per-companion geometry across restart, image stacking, text-cell resizing, spontaneous speech and service ownership. The installed VoiceStudio backend also started without Electron and stopped on normal Sherka exit; no physical microphone test was performed.

## 1.0.0 — persistent moods and live scenes

Optional mood persistence is stored separately from messages and facts in schema 9. A completed text turn updates bounded scores once, with gradual decay, grounded user cues and diminishing repeated model signals; there are no timed/model loops. Independent reset fences in-flight replies. Copies and exports carry the setting but no accumulated state; Hermes is excluded. Widget chrome follows the board theme, while the avatar halo keeps its character accent.

ComfyUI adds advanced per-pass steps/CFG, a shared sampler, square output size, API-workflow export and live variations with conversation priority. Only Sherka-owned jobs are interrupted; live mode requires prompt-specific interruption in ComfyUI 0.3.57+. Model/LoRA/sampler catalogs persist per server on this device and full refresh replaces them. Optional device-specific Python/portable startup applies only to HTTP loopback addresses, deduplicates launches, reuses ready servers and leaves ComfyUI running after Sherka exits. First-generation startup/model-loading delays are explained in the interface.

Search planning handles reasoning-model responses, search settings have a connection check, speech omits emojis and local deletion commands distinguish conversation history from saved facts. Seven-language coverage, mood/migration tests, native reset/restart/theme checks, isolated ComfyUI launch/reuse tests and a read-only existing-server catalog probe passed during development. These automated checks do not validate physical microphone/audio behavior or every model's emotional accuracy.

## 0.12.1 — user-led illustration scenes

The user's visual request takes priority over reply context and default clothing, pose or expression. Stable identity remains a reference, while requested outfits and actions can change. Subject selection distinguishes addressing the companion from asking to depict it. Scene planning resolves character count and a short visual description before composing Illustrious tags or SDXL prose, within the existing request. The dialogue speakers do not imply two copies of the companion. Illustrious `solo` is normalized from the planned character count; group and environment scenes omit it. SDXL sentences no longer inherit the short-tag character cutoff. No profile schema or settings changes are required.

The opt-in `check_scene_intent` example exercises seven languages, conflicting reply/appearance context, unrelated scenery and a distinct human/robot pair using synthetic dialogue. Prompt adherence and rendered identity still depend on the selected models. A real ComfyUI illustration showed one armored dancer in the requested tutu; the base checkpoint still rendered a human-looking face, so this is not proof of perfect Minerva likeness.

Validation: 123 Rust tests passed (one opt-in public-search test skipped), along with TypeScript, formatting, Clippy and three localization tests. The final local Gemma 4 E4B run passed all 22 synthetic prompt checks; earlier runs exposed conflicting outfits, duplicated characters and a short-tag cutoff incorrectly applied to SDXL prose. These checks verify representative instructions, not universal compliance by arbitrary models.

The optimized Windows executable passed the native desktop-scenes scenario, including the structured planning response and SDXL workflow. An initial test worker exited before the scenario started; a later run exposed a diagnostic screenshot racing the settings repaint. The fixture now waits for saving to finish and captures the page instead of a replaceable settings element. No physical microphone or speech validation was required for these illustration changes.

## 0.12.0 — companion packs and image style

The navigation order is Companions, Conversation, Memory & history, Settings. Duplication is more prominent and uses a localized copy suffix; it preserves images and configuration with fresh history and memory. ComfyUI receives an optional per-companion positive prompt addition, appended verbatim after the generated prompt is validated, once per graph. Existing profiles default to an empty suffix.

The default export is now Sherka Companion Archive v1 (`.sherka.zip`): a JSON manifest and seven normalized PNGs, with bundled art resolved from application assets and missing expressions represented by neutral. Import validates size, fixed paths, format/version, images and settings before creating a new identity. It reads in memory without extraction and never activates the new profile. No conversations, memories, credentials or Hermes sessions are exported. Service addresses and authored prompts are included and documented. Legacy JSON import accepts versions 1–5; the internal JSON export is version 5, database schema remains 8.

Validation before packaging: 121 Rust tests passed, with one opt-in public-search test skipped. Ten focused archive/illustration tests passed again after the archive path-rejection fixture was strengthened. TypeScript, formatting, Clippy and three localization tests passed; all 872 authored messages have seven languages. Native tests passed legacy JSON import/permissions, ZIP import and duplication with restart, SDXL positive-prompt concatenation and desktop cells, and a fresh English documentation capture. An initial full-size-image fixture exceeded the short UI wait; the suite uses a small PNG and allows longer imports. Two native workers failed to initialize their WebView debugging endpoint; both scenarios passed on rerun. Service contracts were simulated; no new physical microphone or billed model test was performed.

## 0.11.0 — scene planning and memory controls

Companions now select an explicit Illustrious or SDXL image-model family, with tag-based or visual-prose prompts respectively. The full ComfyUI checkpoint catalog is available, with relative model/LoRA folder filters and an editable, authoritative negative prompt. External directories remain configured on the ComfyUI server. Disabling Turbo uses 16 steps on each sampler and doubles its original CFG (3.8 and 7.0 in the supplied graph). A separate short decision includes the companion's appearance only when the scene calls for it. Minerva's default description now explicitly requires a robotic face; customized descriptions are preserved.

Auxiliary JSON requests use a bounded output budget, optional reasoning disabled when supported, final-answer parsing and a single validated fallback for incompatible local JSON grammars. System instructions are consolidated at the beginning: real NeoHorse testing identified its template rejecting a system message after user data. Expressions now have short examples, recognized aliases and a conservative visible-answer fallback that ignores private thinking. These changes help small models without guaranteeing that every tone is classified correctly.

Memory optimization is available in Settings, Memory and explicit conversation commands. It proposes shorter facts/notes for review, then applies only after confirmation. Existing ids, source links, approval state, session and memory epochs are validated; omitted facts and conversation history survive. Semantic fidelity still needs the user's review. Conversation clearing now requires `/clear` followed by `/confirm-clear` (or recognized localized phrases), expires after three minutes and preserves saved facts. Selecting a companion card also activates that companion in the widget. The README includes the supplied ecosystem diagram.

Validation: 118 Rust tests and three localization tests passed; TypeScript, formatting and Clippy passed, with all 868 authored UI messages covered in seven languages. Across native runs, all 32 default scenarios passed; 12 opt-in scenarios were skipped. Eight focused scenarios also passed on the optimized Windows executable. Three older UI fixtures were updated for visible-widget startup, card activation and read-only catalog discovery. Two native workers exited before their scenarios began; both scenarios passed on rerun.

Synthetic real-service checks passed with local Gemma 4 A4B, Dolphin 8B and NeoHorse: subject selection, both image-prompt families and joy expressions, including optional thinking on/off where supported. NeoHorse also produced a compact memory proposal preserving the synthetic tea preference and the unrelated cat/birth-date fact. A real SDXL Base generation through the production ComfyUI client produced a square mountain/lake landscape without a foreground character, using a custom negative prompt and the non-Turbo schedule. These tests used no personal conversation data or paid API calls; each temporary language-model instance was unloaded. Physical microphone/listening remains outside these automated checks. The release process also checks the signed Windows update paths and extracted Debian package startup before publication.

GitHub CI passed 118 Rust tests on Windows and 117 on Linux, plus 17 Windows native interface scenarios. Static/localization checks passed on both systems; the opt-in live public-search test was skipped.

The Ubuntu-built 0.11.0 Debian x64 release package passed metadata/content inspection, shared-library resolution and an extracted-package startup on Debian 13.6 under Xvfb. Its widget became visible with isolated data. No VoiceStudio, LM Studio or external models were installed; physical desktop/audio behavior remains unvalidated.

## 0.10.0 — inline cells, scenes and Linux

Version **0.10.0** adds automatic text-cell surface resizing and preserves the chosen height through repainting. Widget controls follow visible image bounds, including Kit's wide silhouette, and the active halo follows the character's accent. Six global themes color the interface. Text and image cells share the avatar's transparent window: text stays below the buttons, while the image can move and resize relative to the companion. Both follow the widget and toggle from the same buttons. Text supports typing/pasting, drops, optional spoken replies and IME composition. Widget errors are brief and cannot intercept the controls.

Classic companions can use a per-profile ComfyUI connection with an Illustrious checkpoint, physical description, optional LoRAs/strength sliders and automatic hidden Turbo LoRA selection. The supplied graph repairs prompt/CLIP wiring and generates square temporary previews. Planning starts as soon as text generation completes, while voice continues. One freely composed Illustrious scene prompt drives 1–8 sequential variants, shown in a looping slideshow with configurable timing and 500 ms crossfades. Only the latest user/reply pair and appearance are used; dialogue, memory and avatar emotions remain separate. Hermes is excluded. New input, profile/configuration changes or clearing invalidate older jobs. See [ComfyUI setup](COMFYUI.md).

ASR now preserves segment finals and resumed audio during recognition, with run/turn isolation and conservative filtering of specific subtitle-credit noise. Dropped image attachments are accepted only for vision-capable/explicitly confirmed models, normalized and sent for that turn without storing pixels in history, memory or exports. Per-character web search adds Brave, Tavily and SearXNG to DuckDuckGo, with OS-vault keys and bounded requests. See [web search](WEB_SEARCH.md).

Windows core validation: **110 Rust tests passed**, with the opt-in public DuckDuckGo request skipped. TypeScript, formatting, Clippy and the three localization tests passed. Twelve focused native scenarios passed across runs, covering the seven locales, provider and profile isolation, privacy, Hermes, long speech, image batches, cell geometry and readiness while speech is held. The optimized Windows executable passed the illustration/cell scenario with both speech enabled and the CI text-only setting. API search contracts are simulated; no live Brave/Tavily credits were used.

Earlier alpha.1 service validation passed a real synthetic exchange through GLM 5.3 Flash on Z.ai Coding Plan and the local ComfyUI server, producing a square scene with the installed Illustrious checkpoint and Turbo LoRA. A separate direct ComfyUI generation passed. Existing conversation history was not read; neither test used a physical microphone or listening assessment. Appearance consistency is approximate without reference-image conditioning.

Linux has Secret Service credentials, native source-link opening, Debian packaging and CI configuration. External voice services start separately and updates are manual. Debian 13 x86-64 passed 109 Rust tests, all static/localization checks, native Debian packaging and an extracted-package startup under Xvfb. No LM Studio or VoiceStudio was installed. Physical desktop/audio checks remain unvalidated; see [Linux development](LINUX.md).


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

## 0.7.1 — application logo

The application now has its own logo across Windows icons, the tray, sidebar and favicon. Companion portraits stay independent. Incremental builds rebuild icon resources when branding changes, and the English README features the new identity and a refreshed control-panel screenshot.

## 0.7.0 — Hermes remote agent

Hermes remote-agent mode connects to server profiles with protected keys, persistent conversations, streaming, interruption, explicit tool approvals and a widget **New conversation** action. Hermes owns personality, memory, tools and model budgets; emotional avatars are explicitly excluded. See [Hermes setup](HERMES.md).

This version also includes the fixes prepared for 0.6.1: Automatic search planning failures leave ordinary conversation available. Replies default to 8,192 tokens, with a configurable 64,000-token context fallback for remote models without metadata. Existing 1,024-token defaults are upgraded once (profile schema 5).

Earlier 0.6.1 checks: 78 Rust tests, three localization tests and six native scenarios passed. Native fixtures cover failed local/API search planning followed by normal streamed conversation, advertised output caps, missing context metadata, cancellation, restart, seven languages and startup recovery. Migration tests preserve profile choices, messages and memories. These regression checks use simulated services; there was no new billed inference or physical listening test.

## 0.6.0 — remote providers

Per-companion conversation providers include LM Studio, OpenRouter, Z.ai API, Z.ai Coding Plan, OpenAI, Anthropic and compatible APIs. Keys use Windows Credential Manager and are excluded from profiles. See [provider setup](PROVIDERS.md).

## 0.5.1 — startup recovery

Startup recovery keeps signed updates available when settings or databases are incompatible or unreadable. Installed and no-install executables share data; keep their versions aligned. See [startup recovery](STARTUP_RECOVERY.md).

## 0.5.0 — web search

Optional DuckDuckGo search offers disabled, on-request and automatic modes, bounded snippets and persistent source links. Widget controls now dismiss on pointer exit, focus loss and opening the panel. See [web search](WEB_SEARCH.md).

## 0.4.0 — languages

The public release adds localized widget/tray controls, polished settings fields and a built-in Sherka personality in each language. The original Sherka follows the interface language; custom companions keep the language chosen at creation. Models and voices remain user-configured. See [languages](LANGUAGES.md).

## Baseline features (0.3.3)

Transparent widget, hover/click controls, optional panel, independent profiles and voices, profile import/export, provider catalogs, streamed text with speech prepared ahead, cancellation and voice interruption, editable essential/topical memories, context budgets and capability-aware reasoning.

Installed VoiceStudio starts when its local service is stopped. Sherka waits for its API with timeout and retry, without waiting for every model to preload. Starting LM Studio is independent.

GitHub updates are checked on request. Downloads are signed; installed and no-install copies use separate paths. Voice and the managed model stop before replacement.

## Earlier validation (0.6–0.7)

0.7.0 checks: 81 Rust tests, three localization tests and 25 native scenarios passed. One Playwright worker exited before a scenario began; that scenario passed when rerun separately. Native Hermes coverage includes separate profile keys, user-only requests, streamed speech, approvals opened from Settings, interruption and a new conversation from the widget. Two real server profiles passed synthetic conversation checks, including cross-message recall on the first profile. Real Hermes inference was tested; speech used a VoiceStudio fixture, with no physical microphone/listening validation. The Windows installer and no-install package were built locally; both updater signatures passed verification and rejected altered data. The packaged release executable started with the widget visible and panel hidden, read the protected profile keys, then exited cleanly with isolated data. The installer was not executed on the developer account.

Core tests cover network contracts, sessions, memory, context, isolation and models. Native tests exercise Rust, SQLite and WebView2 with isolated data. Real-service tests are explicitly enabled.

For 0.6.0, native GLM 5.3 Flash streaming passed on OpenRouter and Z.ai Coding Plan using synthetic messages (345 input tokens, 3 output tokens per turn). Standard Z.ai API returned HTTP 429/code 1113 for the test account; subscription credit and API credit are separate. OpenAI and Anthropic adapters have contract coverage, without a live account test. Remote speech was validated against an isolated VoiceStudio fixture, not by a physical listening test.

0.6.0 validation: 76 Rust tests, three localization tests and 24 native UI scenarios passed; opt-in suites are skipped by default. Windows installer and no-install assets were built, both updater signatures verified with corruption rejected, and the release executable started and quit with isolated data. The installer itself was not executed on the developer account.

Headphone conversation and interruption were manually tested during development. Speaker echo cancellation exists, but reliability depends on hardware and placement. One real VoiceStudio cold start exceeded the timeout; a later attempt succeeded. That provider case needs further diagnosis.

Local diagnostics, personal databases and developer machine details are excluded from the repository and releases.

See [roadmap](ROADMAP.md), [contracts](CONNECTIONS.md) and [development](DEVELOPMENT.md).
