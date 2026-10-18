"""Validate the explicit Windows/Linux release set and hash its payloads."""
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
version = json.loads((ROOT / "package.json").read_text(encoding="utf-8"))["version"]
folder = ROOT / "releases" / version
installer = f"Sherka-Compagnon_{version}_x64-setup.exe"
expected = {
    "sherka-compagnon.exe", "sherka-compagnon.exe.sig", installer, installer + ".sig",
    f"Sherka-Compagnon_{version}_windows-x64-portable.zip",
    f"Sherka-Compagnon_{version}_linux-amd64.deb", "latest.json", "SHA256SUMS.txt",
}
actual = {p.name for p in folder.iterdir()}
if actual != expected or any(not (folder / name).is_file() for name in expected):
    raise SystemExit("The release must contain exactly the eight distributable assets.")
manifest = json.loads((folder / "latest.json").read_text(encoding="utf-8"))
base = f"https://github.com/ArthurReboulSalze/Sherka-Compagnon/releases/download/v{version}/"
assert manifest["version"] == version
assert set(manifest["platforms"]) == {"windows-x86_64-nsis", "windows-x86_64-portable"}
for platform, name in [("windows-x86_64-nsis", installer), ("windows-x86_64-portable", "sherka-compagnon.exe")]:
    entry = manifest["platforms"][platform]
    assert entry["url"] == base + name
    assert entry["signature"] == (folder / (name + ".sig")).read_text().strip()
lines = []
for name in sorted(expected - {"SHA256SUMS.txt"}):
    with (folder / name).open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    lines.append(f"{digest}  {name}")
(folder / "SHA256SUMS.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")
print("Windows installer, portable edition, Debian package and updater assets assembled.")
