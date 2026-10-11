# Releasing and updates

## Trust and data boundaries

Only source, generated artwork, clean demo screenshots, public examples and license notices are committed. User databases, audio, local diagnostics, exports, `.env`, secrets and build outputs are ignored. The publication audit inspects the Git index and the portable ZIP separately; it reports paths and reasons without revealing matched values. Inspect screenshots manually as well.

The application checks the fixed official GitHub release manifest only on request. Update requests carry no conversation data. Downloads require the embedded public key and a signed version matching the manifest. A checksum alone is not the update trust mechanism.

The private signing key must remain outside the checkout, backed up securely. Losing it breaks updates for existing installations. Never publish it. The repository secret `TAURI_SIGNING_PRIVATE_KEY` supplies it to release CI; the public key is in `src-tauri/tauri.conf.json`. The current key has an empty passphrase and is protected by filesystem permissions locally and GitHub secret storage remotely.

## Build locally

Keep versions synchronized in the workspace/package manifests, lockfiles, Tauri config and UI. Update the changelog and release notes, then validate:

```powershell
npm.cmd ci
npm.cmd run check
cargo test --workspace --locked
python scripts/third_party.py
$env:TAURI_SIGNING_PRIVATE_KEY = (Get-Content -LiteralPath '<path-to-your-private-key>' -Raw).Trim()
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ''
npm.cmd run build:windows
python scripts/package_release.py
python scripts/audit_publication.py
```

The package script explicitly includes the executable, a short guide and licenses. It signs the direct executable with its version, copies the signed NSIS installer, generates `latest.json` with separate installed/portable targets, and hashes every asset. Output is ignored under `releases/<version>`.

The no-install package uses the Windows user data folder. Updates replace only its executable, after model cleanup. Temporary replacement files and a backup are used; a failed replacement keeps/restarts the previous binary. The installer update path uses Tauri's NSIS support. Both restart with the microphone off.

## Publish

Run NSIS installation tests in a VM or isolated Windows account. A custom `/D=...` directory does not isolate the current user's installation registry entries or shortcuts and can redirect later updates. Use `SHERKA_DATA_DIR` for executable tests without installing on the developer's account.

1. Review staged contents and screenshots; run the audit. Do not use a wholesale working-folder ZIP.
2. Push source and verify CI against the exact commit.
3. Tag that commit, create a draft release, attach the installer, portable ZIP, executable/signatures, `latest.json` and `SHA256SUMS.txt`.
4. Check uploaded asset hashes and manifests before publishing the draft as latest.
5. Verify the public repository, topics, tag, downloads and actual application update check.

The `Release` workflow can build and create a draft from a version already committed to the selected branch. It never silently publishes a draft. It requires the signing secret. Source for a release is the corresponding tag and GitHub's source archives; locked dependency sources remain available from their recorded upstream repositories/crates.

References: [Tauri updater](https://v2.tauri.app/plugin/updater/), [GitHub releases](https://docs.github.com/en/repositories/releasing-projects-on-github/managing-releases-in-a-repository).
