# Changelog

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
