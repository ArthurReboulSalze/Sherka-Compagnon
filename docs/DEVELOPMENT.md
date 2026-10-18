# Build and contribute

Install Rust MSVC **1.91+**, Visual Studio C++ tools, Node.js **22.12+**, WebView2 and Python **3.11+** for release scripts. Use a local path or mapped drive rather than an UNC npm directory.

CI and releases use **Rust 1.92.0**, matching the tested release compiler.

```powershell
npm.cmd ci
npm.cmd run desktop
```

Vite serves development only. Production embeds the UI and uses Rust/Tauri commands. The first build downloads ONNX Runtime; embedded Silero VAD runs on CPU.

## Validate

```powershell
npm.cmd run check
cargo test --workspace --locked
npm.cmd run tauri -- build --debug --no-bundle
npx.cmd playwright test
```

Native tests use isolated databases under `artifacts/native`, without normal user data or real microphone capture. Real-service suites are opt-in: `SHERKA_LIVE_SERVICES`, `SHERKA_LIVE_CONVERSATION`, `SHERKA_LIVE_MEMORY`, `SHERKA_LIVE_SHUTDOWN`, `SHERKA_LIVE_VOICE_STARTUP`. Read each before enabling. Supply your own `SHERKA_TEST_VOICE_ID` for TTS tests.

Remote contract tests use loopback servers and temporary Windows vault entries. `SHERKA_CLOUD_CHECK=1` explicitly enables the native **providers live:** test with `OPENROUTER_API_KEY`, `ZAI_API_KEY` and `ZAI_CODING_API_KEY`. Set `SHERKA_CLOUD_PROVIDERS` to a comma-separated selection (`open_router,zai,zai_coding`) to test only available accounts. It sends a synthetic short prompt to GLM 5.3 Flash on each selected provider, with a 512-token output cap and low thinking, and can incur a small charge. It populates an empty provider key entry without replacing an existing one. Never enable Playwright tracing or log key values during real-service tests. No personal profiles are used.

`SHERKA_DATA_DIR` isolates diagnostics. `SHERKA_AUTOSTART_VOICE=0` disables VoiceStudio launch and readiness waiting. Update overrides are debug-only, loopback-only and require isolated data. `SHERKA_TEST_ALLOW_CURRENT_UPDATE=1` allows a same-version signed debug replacement for tests; release builds ignore it.

Windows CI checks Rust, TypeScript and native flows without external services or real audio hardware. A network simulation does not validate acoustic interruption: distinguish automated, real-service and headphone/speaker tests.

The audio unit tests inject callback/event-queue congestion during synthetic capture, check that playback remains valid, reject partial commands after a gap, and verify that fresh barge-in still works. Desktop coordinator tests verify microphone consent and stale-result rejection after transcription failure. The native `texte indépendant du TTS` scenario exercises long-answer completion, queued-speech cancellation, a VoiceStudio failure and the next successful turn using silent PCM. Technical events are written only to its isolated data directory; these tests do not open a physical microphone.

`npm run check` includes the shared translation catalog audit and text-isolation tests. The native `languages:` scenario exercises all seven interfaces and persistence with mock services and isolated profiles.

CI prepares WebView2 and native inference library search paths before launching the UI tests on Windows Server. Elevated hosted runners also need an EXE-scoped HKLM test policy for the debugging port and isolated browser data: recent WebView2 versions [ignore environment overrides in elevated apps](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/webview-features-flags). The fixture removes its values after each launch; local tests use environment variables only.

## Contribute

Read [architecture](ARCHITECTURE.md) and [contracts](CONNECTIONS.md), then open an issue or pull request with appropriate validation. Preserve isolation, cancellation and separate widget/panel permissions. Never attach databases, personal exports, recordings, secrets or `.env`. Contributions are GPL-3.0-only.

Distribution instructions: [releasing](RELEASING.md).

### Conversation privacy checks

The core `privacy` suite checks saved-fact preservation, optional complete clearing, individual fact deletion, empty archive/FTS results, removed source quotations and summaries, SQLite integrity, profile isolation, persistence after reopen and rejection of stale generation/extraction results. Command-parser tests cover all seven languages and negative/quoted requests. The native `privacy:` scenario exercises the confirmation dialog, cancellation, the unchecked saved-facts option, individual deletion, local speech acknowledgement, `/clear` without configured providers and restart persistence. Services and data are synthetic; it never deletes the developer's conversations or contacts a real model.

## Application branding

The application logo lives in `public/branding/sherka-logo-v2.png`. It supplies the sidebar and favicon; Windows executable and tray icons are generated from it:

```powershell
npm.cmd run tauri -- icon public/branding/sherka-logo-v2.png
```

Commit the generated PNG sizes and `icon.ico` referenced by `src-tauri/tauri.conf.json`. Companion portraits are separate assets in `public/assets`; changing application branding does not change a companion's appearance. The README uses an identical copy at `docs/images/sherka-logo-gold-v2.png` and a versioned control-panel screenshot so GitHub does not keep displaying older branding from its image cache. Refresh those references when the visual identity changes.

### Hermes integration checks

The native `hermes:` scenario uses an isolated HTTP server and temporary vault keys. It checks profile isolation, user-only requests, streamed text/speech, tool approvals, interruption, and the widget `/new` action. Unit tests cover private-address validation, persistent session mappings, export boundaries and the explicit exclusion from emotional avatars.

Set `SHERKA_TEST_TEXT_ONLY=1` to exercise the Hermes and provider scenarios without audio output, matching hosted Windows runners. The widget is explicitly shown before testing its controls.

The optional `hermes live:` scenario uses `SHERKA_TEST_HERMES_URL` and a comma-separated `SHERKA_TEST_HERMES_PROFILES` (default `default`). Configure each profile’s key in Windows Credential Manager through Sherka first. The scenario creates isolated test conversations, sends short synthetic prompts through Hermes’s existing model, and verifies continuation. It may incur provider usage; no key values or existing conversations are logged. Do not enable Playwright tracing for this test.

The `hermes stop:` native scenario verifies the widget and typed `/stop` shortcut, delayed server confirmation, a failed cancellation followed by retry, and prevention of overlapping work. It uses a local fixture and never controls a real service.

For an explicitly authorized real speech-path check, `cargo run -p sherka-core --example check_hermes_voice -- <profile.sqlite3> <private-report.json>` reads an existing Hermes profile without migrating it. It synthesizes two short inputs through VoiceStudio, transcribes them, asks Hermes to search the web and list one skill in fresh conversations, and synthesizes the replies. A final harmless response is interrupted through the run API. The example uses the profile's protected credentials and may incur provider usage. Its report contains transcripts and replies: keep it outside published files. This checks synthetic audio, not a physical microphone or speaker.

### Avatar checks

`avatar_emotions` core tests cover Unicode stream boundaries, malformed markers, image validation, stale edits, migrations, profile copy/import/export and neutral resets. The native `avatars:` test exercises the seven bundled images, upload preview, per-avatar voice selection, streamed expression changes, TTS text filtering, cancellation and restart. It also freezes the real 500 ms opacity animations at their midpoint to verify both decoded layers, then resumes them and checks cleanup and return to neutral. The `languages:` tests cover the editor in all seven locales; `hermes:` checks its neutral-only mode. These tests simulate the model and VoiceStudio, so they do not measure a real model’s emotion choices or physical speech playback.

`companion_pack` checks the five starter profiles, translated personalities, bundled image validity, one-time initialization and independent search modes. The native `companion settings:` scenario verifies placement in Companions, image rendering, separate preferences and preservation of unsaved connection edits during a web-mode save.
