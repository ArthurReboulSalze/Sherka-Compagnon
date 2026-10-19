# Linux builds

Sherka's desktop shell, profiles, conversation providers, audio pipeline and optional ComfyUI connector have Linux implementations. API credentials use the desktop Secret Service through `secret-tool` (KWallet/compatible service). Windows continues to use Windows Credential Manager. Missing or locked credential services produce an explicit error; keys are never written to SQLite or plaintext files.

VoiceStudio and LM Studio are separate services. The Linux build does not install either application or download their models. The prepared VoiceStudio Python backend can start without its desktop window: configure its runtime folder and Python in Settings, or start your speech service separately. No service is installed by Sherka. ComfyUI can optionally start from an explicitly configured local Python installation when illustrations are enabled; remote ComfyUI servers remain manually managed. Servers started by Sherka close with it; pre-existing services stay running. See [local ComfyUI startup](COMFYUI.md#optional-local-startup). Windows installers are never offered to the Linux updater: update Linux manually with the release's Debian package.

## Debian development

Install Rust 1.92, Node.js 20.19+ and these native development packages:

```sh
sudo apt-get update
sudo apt-get install build-essential pkg-config libssl-dev libasound2-dev \
  libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
  librsvg2-dev libxdo-dev libsecret-tools xdg-utils patchelf
npm ci
npm run check
cargo test --workspace --locked
npm run build:linux
```

`tauri.linux.conf.json` selects a Debian package and disables Windows updater artifacts. Output appears under `target/release/bundle/deb`. The data directory follows Tauri's per-user Linux data path; `SHERKA_DATA_DIR` can isolate development checks. Existing Windows profiles can be exported and imported, but API keys must be entered again in the Linux session.

For headless compilation/smoke checks use Xvfb and an isolated data directory, with `SHERKA_AUTOSTART_VOICE=0`. They establish build/runtime startup, not physical audio or compositor behavior. Transparent always-on-top windows, dragging, click-through, monitor positioning and tray availability also depend on the desktop/compositor. Validate those interactions on the target desktop before treating them as physically tested.

References: [Tauri Linux prerequisites](https://v2.tauri.app/start/prerequisites/#linux).

## Validation of 0.11.0

The Debian x64 release package built on Ubuntu 24.04 passed version/architecture and package-content inspection, shared-library resolution and an extracted-package startup on Debian 13.6. The widget became visible under Xvfb with isolated user data. Linux CI also passed static/localization checks, the Rust tests and a native debug build. No VoiceStudio, LM Studio or external models were installed on the Debian machine. Physical audio and compositor behavior remain unvalidated.

## Validation of 0.10.0-alpha.2

On Debian 13 x86-64 with Rust 1.92 and Node.js 20.19, formatting, Clippy, TypeScript and localization checks passed, followed by **109 Rust tests** (one opt-in live public-search test skipped). A native debug Debian package was built, extracted without installing it, checked for missing shared libraries and started with an isolated data directory under Xvfb. Its companion window remained visible and the process stayed alive. No LM Studio, VoiceStudio or conversation/diffusion models were installed on this machine.

This validates compilation and packaged startup. Physical microphone/playback, compositor interactions, tray behavior and Secret Service unlocking still require a normal Linux desktop session. The Xvfb session can emit desktop-portal warnings; it does not reproduce the user's full desktop environment.
