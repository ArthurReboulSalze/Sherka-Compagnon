# Build and contribute

Install Rust MSVC **1.91+**, Visual Studio C++ tools, Node.js **22.12+**, WebView2 and Python **3.11+** for release scripts. Use a local path or mapped drive rather than an UNC npm directory.

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

`SHERKA_DATA_DIR` isolates diagnostics. `SHERKA_AUTOSTART_VOICE=0` disables VoiceStudio launch and readiness waiting. Update overrides are debug-only, loopback-only and require isolated data. `SHERKA_TEST_ALLOW_CURRENT_UPDATE=1` allows a same-version signed debug replacement for tests; release builds ignore it.

Windows CI checks Rust, TypeScript and native flows without external services or real audio hardware. A network simulation does not validate acoustic interruption: distinguish automated, real-service and headphone/speaker tests.

## Contribute

Read [architecture](ARCHITECTURE.md) and [contracts](CONNECTIONS.md), then open an issue or pull request with appropriate validation. Preserve isolation, cancellation and separate widget/panel permissions. Never attach databases, personal exports, recordings, secrets or `.env`. Contributions are GPL-3.0-only.

Distribution instructions: [releasing](RELEASING.md).
