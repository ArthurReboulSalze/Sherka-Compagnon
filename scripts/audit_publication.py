"""Fail closed on staged private data and on unexpected release ZIP entries.

Print paths/reasons only, never matching secret values.
"""
import argparse
import re
import subprocess
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ROOT_FILES = {".gitattributes", ".gitignore", "Cargo.lock", "Cargo.toml", "LICENSE", "README.md",
              "THIRD_PARTY_NOTICES.md", "CHANGELOG.md", "SECURITY.md",
              "index.html", "package-lock.json", "package.json", "playwright.config.ts", "tsconfig.json", "vite.config.ts"}
FOLDERS = {".github", "config", "crates", "docs", "licenses", "scripts", "src", "src-tauri", "tests", "public"}
PRIVATE = re.compile(r"(?i)(?:^|/)(?:artifacts|target|dist|node_modules|profiles|backups|data|secrets|test-results|playwright-report|webview)(?:/|$)|(?:^|/)\.env(?:\.|$)|\.(?:sqlite3?|db|key|pem|pfx|p12|exe|dll|zip|7z|dmp|wav|pcm|mp3|mp4|webm)(?:-|$)")
# Reviewed public media that would otherwise match PRIVATE or exceed the size
# review threshold: exact path -> maximum bytes. Add entries deliberately.
PUBLIC_MEDIA = {"docs/site/video/sherka-overview.mp4": 16 * 1024 * 1024}
PATTERNS = [
    ("private key", re.compile(rb"-----BEGIN (?:RSA |EC |OPENSSH |ENCRYPTED )?PRIVATE KEY-----")),
    ("GitHub token", re.compile(rb"\b(?:gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{40,})\b")),
    ("provider key", re.compile(rb"\bsk-(?:proj-|ant-)?[A-Za-z0-9_-]{24,}\b")),
    ("credential in URL", re.compile(rb"https?://(?!user:password@localhost)[^\s/'\"]+:[^\s/'\"]+@")),
    ("private Windows user path", re.compile(rb"(?i)[A-Z]:[\\/]+Users[\\/]+[A-Za-z0-9_]+")),
]

def scan(name, data):
    if b"\0" in data[:4096] or name.endswith((".png", ".webp", ".ico", ".onnx")):
        return []
    errors = [f"{name}: {label}" for label, pattern in PATTERNS if pattern.search(data)]
    # Tauri private keys are base64 containers, not PEM.
    import base64
    for value in re.findall(rb"[A-Za-z0-9+/]{100,}={0,2}", data):
        try:
            decoded = base64.b64decode(value)
            if b"minisign secret key" in decoded or b"minisign encrypted secret key" in decoded:
                errors.append(f"{name}: private signing key")
        except ValueError:
            pass
    return errors

def staged():
    names = subprocess.check_output(["git", "ls-files", "-z"], cwd=ROOT).decode().split("\0")
    errors = []
    count = 0
    for name in filter(None, names):
        count += 1
        public_media = name in PUBLIC_MEDIA
        if (PRIVATE.search(name) and not public_media) or ("/" not in name and name not in ROOT_FILES) or ("/" in name and name.split('/')[0] not in FOLDERS):
            errors.append(f"{name}: forbidden publication path")
        if name.startswith("src-tauri/gen/"):
            errors.append(f"{name}: generated private build metadata")
        data = subprocess.check_output(["git", "show", ":" + name], cwd=ROOT)
        if len(data) > PUBLIC_MEDIA.get(name, 8 * 1024 * 1024):
            errors.append(f"{name}: oversized file requires review")
        errors.extend(scan(name, data))
    if not count:
        errors.append("No staged files to audit")
    return count, errors

def archive(path):
    allowed = {"sherka-compagnon.exe", "README.txt", "LICENSE", "THIRD_PARTY_NOTICES.md", "licenses/Silero.txt", "licenses/ONNXRuntime.txt", "licenses/ONNXRuntime-ThirdPartyNotices.txt"}
    with zipfile.ZipFile(path) as z:
        names = [n for n in z.namelist() if not n.endswith('/')]
        errors = [f"{n}: unexpected release ZIP entry" for n in names if n not in allowed]
        if set(names) != allowed or len(names) != len(allowed):
            errors.append("Release ZIP must contain exactly the explicit distributable files")
        if z.testzip():
            errors.append("Release ZIP CRC failure")
        for name in names:
            if name != "sherka-compagnon.exe":
                errors.extend(scan(name, z.read(name)))
    return len(names), errors

parser = argparse.ArgumentParser()
parser.add_argument("--zip", type=Path)
args = parser.parse_args()
count, errors = archive(args.zip) if args.zip else staged()
if errors:
    raise SystemExit("\n".join(errors))
print(f"Publication audit passed: {count} files; no forbidden data or credential patterns.")
