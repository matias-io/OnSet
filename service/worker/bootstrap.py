"""Fetch a pinned Linux uv binary, verify its published wheel hash, and extract it."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import urllib.request
import zipfile


def prepare(destination):
    destination = Path(destination)
    destination.mkdir(parents=True, exist_ok=True)
    binary = destination / "uv"
    if binary.exists():
        return binary
    with urllib.request.urlopen("https://pypi.org/pypi/uv/0.8.22/json", timeout=30) as response:
        metadata = json.load(response)
    wheel = next(p for p in metadata["urls"] if "manylinux_2_17_x86_64" in p["filename"] and p["filename"].endswith(".whl"))
    with urllib.request.urlopen(wheel["url"], timeout=120) as response:
        content = response.read()
    if hashlib.sha256(content).hexdigest() != wheel["digests"]["sha256"]:
        raise RuntimeError("Worker bootstrap checksum mismatch")
    with zipfile.ZipFile(io.BytesIO(content)) as archive:
        member = next(n for n in archive.namelist() if n.endswith("/bin/uv") or n.endswith("/scripts/uv"))
        binary.write_bytes(archive.read(member))
    (destination / "manifest.json").write_text(json.dumps({"version": "0.8.22", "sha256": wheel["digests"]["sha256"], "source": wheel["url"]}), encoding="utf-8")
    return binary


if __name__ == "__main__":
    parser=argparse.ArgumentParser()
    parser.add_argument("destination")
    print(prepare(parser.parse_args().destination))
