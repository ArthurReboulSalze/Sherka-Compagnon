# Changelog

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
