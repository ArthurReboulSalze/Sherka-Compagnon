"""Generate distributable notices from locked Windows runtime dependencies."""
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
metadata = json.loads(subprocess.check_output([
    "cargo", "metadata", "--locked", "--format-version", "1",
    "--filter-platform", "x86_64-pc-windows-msvc",
], cwd=ROOT))
packages = {p["id"]: p for p in metadata["packages"]}
nodes = {n["id"]: n for n in metadata["resolve"]["nodes"]}
todo = list(metadata["workspace_members"])
seen = set()
while todo:
    key = todo.pop()
    if key in seen:
        continue
    seen.add(key)
    todo.extend(d["pkg"] for d in nodes[key]["deps"]
                if any(k["kind"] is None for k in d["dep_kinds"]))

parts = ["# Third-party notices\n\nSherka is GPL-3.0-only. The dependencies below retain their own licenses.\n"
         "Generated from Cargo.lock for the Windows x64 runtime and the bundled frontend.\n"
         "External VoiceStudio, LM Studio and conversation/speech models are not distributed here.\n"]
missing = []
for key in sorted(seen, key=lambda k: (packages[k]["name"], packages[k]["version"])):
    p = packages[key]
    if key in metadata["workspace_members"]:
        continue
    folder = Path(p["manifest_path"]).parent
    files = sorted(f for f in folder.iterdir() if f.is_file()
                   and f.name.lower().startswith(("license", "licence", "copying", "notice")))
    if p.get("license_file"):
        license_file = folder / p["license_file"]
        if license_file not in files:
            files.append(license_file)
    if not files:
        cached = ROOT / "licenses/third-party" / f"{p['name']}-{p['version']}.txt"
        if cached.is_file():
            files.append(cached)
    parts.append(f"\n## {p['name']} {p['version']}\n\nLicense: {p.get('license') or 'See text below'}\n")
    if p.get("repository"):
        parts.append(f"Source: {p['repository']}\n")
    if not files:
        # SPDX crates may embed a license in README or source headers; require review.
        missing.append(p["name"])
    for f in files:
        parts.append(f"\n### {f.name}\n\n```text\n{f.read_text(encoding='utf-8', errors='replace').strip()}\n```\n")

for js in sorted((ROOT / "node_modules/@tauri-apps/api").glob("LICENSE*")):
    parts.append(f"\n## @tauri-apps/api {js.name}\n\n```text\n" + js.read_text(encoding="utf-8").strip() + "\n```\n")
for file in ["LICENSE.silero", "LICENSE.onnxruntime", "ONNXRUNTIME-ThirdPartyNotices.txt"]:
    parts.append(f"\n## {file}\n\n```text\n" + (ROOT / "crates/sherka-core/models" / file).read_text(encoding="utf-8").strip() + "\n```\n")
if missing:
    raise SystemExit("License text missing; inspect these crates before publication: " + ", ".join(missing))
(ROOT / "THIRD_PARTY_NOTICES.md").write_text("".join(parts), encoding="utf-8")
print(f"Generated notices for {len(seen) - len(metadata['workspace_members'])} Rust runtime dependencies.")
