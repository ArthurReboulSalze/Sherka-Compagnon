"""Package only explicit distributable files; never copy a working directory."""
import hashlib
import json
import shutil
import subprocess
import zipfile
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
version = json.loads((ROOT / "package.json").read_text(encoding="utf-8"))["version"]
repo = "ArthurReboulSalze/Sherka-Compagnon"
output = ROOT / "releases" / version
output.mkdir(parents=True, exist_ok=True)
exe = output / "sherka-compagnon.exe"
shutil.copy2(ROOT / "target/release/sherka-compagnon.exe", exe)
setup_name = f"Sherka-Compagnon_{version}_x64-setup.exe"
setup = output / setup_name
source = ROOT / "target/release/bundle/nsis" / f"Sherka Compagnon_{version}_x64-setup.exe"
shutil.copy2(source, setup)
shutil.copy2(Path(str(source) + ".sig"), Path(str(setup) + ".sig"))
subprocess.run(["node", "node_modules/@tauri-apps/cli/tauri.js", "signer", "sign", "--app-version", version, str(exe)], cwd=ROOT, check=True)
readme = """Sherka Companion — Windows x64, no installation

Extract the entire ZIP into a writable folder and run sherka-compagnon.exe.
WebView2, the Visual C++ x64 runtime and VoiceStudio are required for voice.
Use LM Studio locally, or connect your own remote API provider in Companions.
Sherka, Botsy, Alba, Kit and Minerva include seven expressions with smooth fades.
Set each character's images and VoiceStudio voice in Avatar & voice.
Hermes mode connects to your agent's own profiles, memory and tools.
The /stop shortcut stops its current task, not the Hermes server.
Setup guide: https://github.com/ArthurReboulSalze/Sherka-Compagnon/blob/main/docs/HERMES.md
Start your local model server. Configure VoiceStudio and transcription once in Settings;
Sherka can start its prepared Python server in the background without a desktop window.
Set custom runtime/Python paths if needed. Each service has one Start with Sherka option:
servers launched by Sherka close with it; pre-existing or remote services stay independent.
Choose one of seven interface languages in Settings. The original Sherka's
personality and instructions follow that language automatically.
Configure compatible models with your provider and voices in VoiceStudio.
Remote providers receive conversation context and may charge API fees.

Data stays in %LOCALAPPDATA%\\fr.sherka.compagnon, not beside this executable.
Check updates manually from Settings. No private data is included here.
Clear a companion's conversation from Conversation, or request /clear and confirm
with /confirm-clear. Saved facts remain; individual facts can be deleted from Memory.
Preview memory optimization from Settings or Memory before applying it.
Optional ComfyUI scenes support Illustrious and SDXL, with a per-companion
negative prompt, character-only negative appearance prompt, positive prompt addition,
model/LoRA folder filters and Turbo or normal sampling.
Your requested scene, outfit and action take priority over default poses or reply context.
Scene planning requests one companion by default and distinguishes intentional group scenes.
Duplicate companions or share a .sherka.zip pack with all seven expression images.
Packs contain no conversations, memories, accumulated moods or API keys. Recheck service connections after import.
Optional persistent moods remember reactions between turns and can be reset separately.
Widget controls follow the selected theme; the active halo keeps the character accent.
ComfyUI adds advanced sampling/output size, live scenes with conversation priority,
remembered catalogs and optional startup of a configured local Python/portable install.
The first images may take longer while ComfyUI and models load. See docs/COMFYUI.md.
Avatar and image-cell position/size are remembered per companion across restarts.
Overlapping images sit behind the avatar. Enlarge image cells or double-click an image
for full screen; the companion stays movable/resizable in front. Esc returns to the cell.
The widget text cell also accepts image pastes when the model supports them.
Optional Spontaneous conversation chooses
one brief invitation after a random idle delay, then waits for a real reply. Hermes is excluded.
Voice pause, a hidden widget, typing and ongoing conversation postpone invitations.
Hosted providers may charge for them. Machine paths/shared speech settings stay out of packs.
Settings may be upgraded; avoid opening them with older executables.
For Hermes, clearing in Sherka affects local history only.

Documentation: https://github.com/ArthurReboulSalze/Sherka-Compagnon
License: GPL-3.0-only. Dependency notices accompany this distribution.
"""
zip_path = output / f"Sherka-Compagnon_{version}_windows-x64-portable.zip"
with zipfile.ZipFile(zip_path, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
    for name, path in {
        "sherka-compagnon.exe": exe,
        "LICENSE": ROOT / "LICENSE",
        "THIRD_PARTY_NOTICES.md": ROOT / "THIRD_PARTY_NOTICES.md",
        "licenses/Silero.txt": ROOT / "crates/sherka-core/models/LICENSE.silero",
        "licenses/ONNXRuntime.txt": ROOT / "crates/sherka-core/models/LICENSE.onnxruntime",
        "licenses/ONNXRuntime-ThirdPartyNotices.txt": ROOT / "crates/sherka-core/models/ONNXRUNTIME-ThirdPartyNotices.txt",
    }.items():
        z.write(path, name)
    z.writestr("README.txt", readme)
base = f"https://github.com/{repo}/releases/download/v{version}/"
manifest = {
    "version": version, "notes": f"Sherka {version}. See the release page for changes.",
    "pub_date": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
    "platforms": {
        "windows-x86_64-nsis": {"url": base + setup_name, "signature": Path(str(setup) + ".sig").read_text().strip()},
        "windows-x86_64-portable": {"url": base + exe.name, "signature": Path(str(exe) + ".sig").read_text().strip()},
    },
}
(output / "latest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
names = [exe.name, exe.name + ".sig", setup.name, setup.name + ".sig", zip_path.name, "latest.json"]
lines = [hashlib.sha256((output / n).read_bytes()).hexdigest() + "  " + n for n in names]
(output / "SHA256SUMS.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")
subprocess.run(["python", "scripts/audit_publication.py", "--zip", str(zip_path)], cwd=ROOT, check=True)
print(f"Release {version} packaged: installer, no-install ZIP and signed updater assets.")
