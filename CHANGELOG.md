# Changelog

## 0.10.0-alpha.2 — in development

- Keep widget controls and its resize handle close to the visible portrait, including wide avatars; use the character's chosen color for its active halo.
- Add Gold, Sage, Ocean, Iris, Rose and Mono application themes.
- Toggle a transparent text cell below the widget controls to type, paste or drop text, with optional spoken replies. It follows the avatar without opening another window.
- Add optional square conversation illustrations through ComfyUI, configured per companion with an Illustrious checkpoint, physical description, additional LoRAs and strength sliders.
- Prepare each scene from the latest user/companion exchange using freely composed Illustrious tags and English descriptions, independently of speech and memory.
- Supply the provided workflow, automatic Turbo LoRA resolution and a bounded API-workflow import. Keep only the newest eligible preview; exclude Hermes.
- Start scene preparation as soon as the reply text is complete, while speech continues. Generate 1–8 images per exchange, with a configurable looping slideshow and half-second crossfades in a movable, resizable inline cell.
- Preserve confirmed transcription segments and the original audio when speech resumes during recognition. Filter specific subtitle-credit/non-speech hallucinations without discarding real short answers.
- Show brief, non-interactive widget errors above the controls.
- Accept dropped PNG, JPEG and WebP images when the selected model advertises vision or the user confirms an undocumented capability. Image pixels are transient and excluded from history, memory and exports.
- Add per-companion Brave Search, Tavily and SearXNG options alongside DuckDuckGo. Store API keys in the OS vault; keep Hermes search on its server.
- Add Linux credential storage, browser integration, Debian packaging and CI checks. Linux updates remain manual and external voice services are started separately.
- Advance profile/settings schema to 8 and profile export format to 4; retain legacy imports and scope isolated native instances to their data directory.

## 0.9.0 — 2026-10-18

- Clear a companion’s local conversation, archives, sources and summaries while keeping explicitly saved or approved facts by default; optionally clear saved facts too.
- Accept explicit conversation-reset requests in seven languages and `/clear`, with a spoken local acknowledgement and no extra language-model request.
- Delete individual facts from the Memory page and exported memory data.
- Cancel stale generation/extraction jobs and reset capture/session references so deleted history cannot be reintroduced. Hermes server data remains under Hermes control.
- Keep spoken replies and microphone consent active through temporary capture-buffer congestion; discard damaged utterances and resume after a pause.
- Keep transcription failures local to the affected turn instead of disabling the microphone.
- Avoid unused echo-reference work in headphone and playback-only modes.
- Preserve bounded local audio diagnostics across restarts, without recordings, conversation content or credentials.

## 0.8.1 — 2026-10-18

- Regenerate the Windows, installer, tray and in-app icons from the tightly cropped golden logo.
- Replace Candis with Alba's seven supplied images and display name, keeping the same personality traits in all seven languages.
- Update Minerva's neutral portrait and six expressions with the new armored design, preserving her irritable personality.
- Upgrade the original Candis identity to Alba while retaining profile IDs, models, voices, history and custom images. Keep edited names and personality text intact.
- Refresh the README portraits, descriptions and control-panel screenshot with new image URLs to avoid stale branding caches.

## 0.8.0 — 2026-10-18

- Add Botsy, Candis, Kit and Minerva alongside Sherka, with localized personalities and seven supplied expressions each; retire the Orion starter example.
- Group provider, model, reasoning, context, transcription and per-companion Internet search controls under Companions.
- Associate each character's VoiceStudio voice and images in the Avatar & voice editor.
- Select neutral, joy, sadness, anger, surprise, reflection or tenderness through the existing reply stream, without an extra model request. Remove expression metadata before display, speech and history.
- Crossfade images over half a second after loading, including the return to neutral. Coalesce rapid changes and respect reduced-motion preferences.
- Preserve per-character image overrides through restart, duplication and profile import/export (schema 6, export format 2; legacy imports remain supported).
- Replace application branding with the orbital-star logo, independently of companion portraits, and refresh the English README and screenshot.
- Add a visible Hermes /stop control and typed shortcut, confirm remote task termination and keep failed stops retryable without overlapping work. The server remains running.
- Keep local search, memory, personality and expressions bypassed for Hermes. Its avatar remains neutral.

## 0.7.1 — 2026-10-16

- Give the application its own blue-haired, starry-eyed logo across Windows icons, the tray, sidebar and favicon; companion portraits remain independent.
- Rebuild Windows icon resources whenever branding assets change, avoiding stale icons in incremental builds.
- Refresh the English README with the new logo, cyan accents, a feature overview and an updated control-panel screenshot.

## 0.7.0 — 2026-10-11

- Connect to Hermes as a voice client, with per-server profile selection and protected profile-specific API keys.
- Keep conversation, personality, memory, model selection and tools on Hermes; reuse Sherka’s avatar and VoiceStudio audio pipeline.
- Stream replies, relay one-time tool approvals, and stop server work when interrupted.
- Preserve separate sessions per companion, server and Hermes profile, including server-side context compression.
- Start a new Hermes conversation from the widget or `/new`, preserving server memory and previous conversations.
- Include connection/key setup help in seven languages. Remote-agent mode is explicitly excluded from future emotional-avatar behavior.

### Conversation fixes (prepared in 0.6.1)

- Continue ordinary conversation when automatic web search planning fails, instead of replacing the reply with a search error.
- Clarify that personal stories, daily activities and feelings do not require web search; allow more room for structured planning on thinking API models.
- Raise the default reply reserve to 8,192 tokens and upgrade the former 1,024-token profile default once, preserving other settings and history.
- Use a configurable 64,000-token context fallback when a remote provider omits its limit; keep advertised context/output limits and the actual LM Studio context authoritative.
- Explain the context fallback in all seven interface languages.

## 0.6.0 — 2026-10-11

- Per-companion OpenRouter, Z.ai API and Coding Plan, OpenAI, Anthropic and compatible API providers alongside LM Studio.
- OpenAI Chat Completions and Anthropic Messages adapters for streaming, usage and structured memory operations.
- Windows Credential Manager keys scoped to provider and endpoint, separate from profiles and exports.
- Remote context/output budgets, mandatory thinking controls and local model cleanup on cloud handover.
- Provider settings and feedback in all seven interface languages, with privacy and setup documentation.

## 0.5.1 — 2026-10-11

- Startup recovery screen replaces silent exits when settings or profile databases cannot be opened, including data last used by a newer executable.
- Signed GitHub update checks and installation remain available in recovery, without initializing voice, loading a model or resetting data.
- Read-only compatibility checks run before settings writes or profile migrations; recovery follows the saved interface language where readable.
- A local startup diagnostic records the version and error; documentation explains that installed and no-install copies share data and how to repair older releases.
- Native recovery coverage for newer settings, newer profile schemas and malformed settings; installer validation guidance now requires an isolated Windows account or VM.

## 0.5.0 — 2026-10-11

- Optional DuckDuckGo web search without an account or API key, disabled by default.
- Explicit text/voice searches and automatic local search decisions, with cancellable requests.
- Bounded search snippets within the loaded model's context budget; normal answer streaming continues.
- Source links attached to each response, retained in profile-local history and omitted from character exports.
- Search settings, progress and errors translated into all seven interface languages.
- Fixed localized notices for failed/empty searches prevent fabricated verifications; safe source links and speech without source markers or URLs.
- History schema 4 stores source receipts; upgraded data must not be opened with older binaries.
- Widget controls and companion picker now dismiss when the pointer leaves, focus is lost or the control panel opens; clicking no longer leaves controls permanently pinned.
- README showcases the independent transparent desktop widget alongside the optional control panel.

## 0.4.0 — 2026-10-11

- Seven interface languages: French, English, Spanish, Italian, German, Simplified Chinese and Japanese.
- Saved interface preference, synchronized widget controls and translated tray/window labels.
- Ready-to-use Sherka personality and instructions in all seven languages, following the interface language until personalized.
- Companion language chosen once at creation, independent of the interface; model and voice choices remain user-configured.
- Custom profile contents, histories and memories preserved when switching interface languages.
- Unicode sentence boundaries for Chinese and Japanese speech text.
- Shared catalog checks and isolated native coverage for all seven interfaces.
- Consistent dark form fields, aligned audio selections and styled sliders with live percentage values.

## 0.3.3 — 2026-10-11

First public release.

- Continuous local voice conversations with streamed text, prepared speech and interruption.
- Transparent widget and independent character models, voices and memories.
- Context budgets, reasoning controls and safe model switching/shutdown.
- Automatic startup and readiness checks for installed VoiceStudio.
- Manual signed GitHub updates for installed and directly launched copies.
- Windows installer and no-install distribution, English documentation, GPLv3 and dependency notices.

VoiceStudio is required. LM Studio is currently the only conversation provider. The application UI is in French. Avatar animation, expressions and moods remain planned.
