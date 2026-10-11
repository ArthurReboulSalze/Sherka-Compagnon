# Startup recovery

An installed executable and a no-install or development executable are separate copies of Sherka. They share the Windows user data folder by default. Starting a newer copy can upgrade settings and history without replacing an older executable or its shortcut.

For example, a 0.4.0 installation cannot read the `web_search` setting and history schema introduced by 0.5.0. Releases before 0.5.1 could close immediately during startup, leaving no visible explanation. The databases can be healthy even when this happens.

## Recover access

With 0.5.1 or later, Sherka opens a recovery screen if it cannot load its settings or profile databases. Use **Check for updates** to retrieve the latest signed release, then restart. The same update verification applies to installed and no-install copies.

The recovery screen does not initialize conversation, audio, VoiceStudio or a conversation model. It shows the saved interface language when that preference is readable, otherwise French. Compatibility checks run before normal startup writes and migrations. Data is not reset; downgrades remain blocked.

An update resolves a version mismatch only when the target release supports the data. Corrupt databases, file permissions or unavailable disk paths may need separate diagnosis. Expand **Startup details** and preserve the local `startup-error.log` in the data folder. Do not post personal databases or raw diagnostics publicly without reviewing them.

For a pre-0.5.1 executable that cannot open at all, download the latest [official installer](https://github.com/ArthurReboulSalze/Sherka-Compagnon/releases/latest) and install over the existing installation. Confirm its destination folder. For a no-install copy, extract the complete latest ZIP and use its executable. Neither action requires deleting user data.

## Avoid conflicting copies

Keep the installed and no-install versions aligned when switching between them. During development, set `SHERKA_DATA_DIR` to an isolated folder before running a new build. Installer tests also require an isolated Windows account or VM: `/D=...` changes the executable folder but does not isolate registry entries or shortcuts.
