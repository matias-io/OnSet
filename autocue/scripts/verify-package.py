"""Compare a built package's payload with the current source and record hashes.

Usage: python scripts/verify-package.py <release-dir> <output-json> [installed-dir]

Checks every packaged Python source file, the renderer/desktop files inside
app.asar and the bundled worker against the working tree. With an installed
directory it also compares the installed resources with the same source.
"""
from __future__ import annotations
import hashlib
import json
import struct
import sys
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def file_sha(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def read_asar(path: Path) -> dict[str, bytes]:
    """Minimal asar reader: header pickle, then concatenated file bodies."""
    raw = path.read_bytes()
    header_size = struct.unpack_from("<I", raw, 12)[0]
    header = json.loads(raw[16:16 + header_size])
    base = 8 + struct.unpack_from("<I", raw, 4)[0]
    files = {}
    def walk(node, prefix):
        for name, item in node.get("files", {}).items():
            key = f"{prefix}/{name}" if prefix else name
            if "files" in item:
                walk(item, key)
            elif not item.get("unpacked"):
                offset = base + int(item["offset"])
                files[key] = raw[offset:offset + item["size"]]
    walk(header, "")
    return files


def compare_resources(resources: Path):
    mismatched, checked = [], 0
    for source in sorted((ROOT / "autocue_app").glob("*.py")) + [ROOT / n for n in ("autocue_core.py", "autocue_rb.py", "mixplan.py")]:
        relative = source.relative_to(ROOT).as_posix()
        packaged = resources / "python-src" / relative
        checked += 1
        if not packaged.is_file() or file_sha(packaged) != file_sha(source):
            mismatched.append(relative)
    for source in sorted((ROOT / "worker").rglob("*")):
        if source.is_file() and "__pycache__" not in source.parts:
            relative = source.relative_to(ROOT).as_posix()
            packaged = resources / relative
            checked += 1
            if not packaged.is_file() or file_sha(packaged) != file_sha(source):
                mismatched.append(relative)
    asar = resources / "app.asar"
    archive = read_asar(asar)
    for folder in ("dist", "desktop-dist"):
        for source in sorted((ROOT / folder).rglob("*")):
            if source.is_file() and ".test." not in source.name:
                relative = source.relative_to(ROOT).as_posix()
                checked += 1
                if sha(archive.get(relative, b"\0missing")) != file_sha(source):
                    mismatched.append("app.asar:" + relative)
    stale = [name for name in archive if name.startswith(("dist/", "desktop-dist/")) and not (ROOT / name).is_file()]
    return {"files_checked": checked, "mismatched": mismatched, "stale_in_archive": stale, "asar_sha256": file_sha(asar)}


def main():
    release, output = Path(sys.argv[1]), Path(sys.argv[2])
    installer = next(release.glob("Autocue Setup *.exe"))
    result = {"schema_version": 1, "created": datetime.now(timezone.utc).isoformat(),
              "installer": str(installer.resolve()), "bytes": installer.stat().st_size, "sha256": file_sha(installer),
              "unpacked": compare_resources(release / "win-unpacked" / "resources")}
    if len(sys.argv) > 3:
        result["installed"] = compare_resources(Path(sys.argv[3]) / "resources")
    result["source_sha256"] = {p.relative_to(ROOT).as_posix(): file_sha(p) for p in sorted((ROOT / "autocue_app").glob("*.py"))}
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(result, indent=2), encoding="utf-8")
    ok = not result["unpacked"]["mismatched"] and not result.get("installed", {}).get("mismatched")
    print(json.dumps({k: v for k, v in result.items() if k != "source_sha256"}, indent=2))
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
