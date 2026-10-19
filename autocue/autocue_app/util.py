from __future__ import annotations

import hashlib
import json
import os
import tempfile
from datetime import datetime, timezone
from pathlib import Path, PureWindowsPath


def utcnow():
    return datetime.now(timezone.utc).isoformat()


def app_home():
    return Path(os.environ.get("AUTOCUE_HOME", str(Path(os.environ.get("LOCALAPPDATA", Path.home() / ".local/share")) / "Autocue")))


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False, ensure_ascii=False,
                      default=lambda v: v.item() if hasattr(v, "item") else v.tolist() if hasattr(v, "tolist") else (_ for _ in ()).throw(TypeError(f"Unsupported value {type(v)}")))


def digest(value):
    return hashlib.sha256(canonical(value).encode()).hexdigest()


def file_hash(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def atomic_json(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix=path.name + ".", suffix=".tmp", dir=path.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8", newline="\n") as f:
            f.write(json.dumps(value, indent=2, ensure_ascii=False, allow_nan=False))
            f.flush()
            os.fsync(f.fileno())
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def basename(path):
    return PureWindowsPath(str(path).replace("/", "\\")).name


def match_opinion(opinions, path, all_paths=()):
    """Exact path first; cross-machine basename only when unique on both sides."""
    key = str(path).replace("\\", "/").casefold()
    exact = [v for k, v in opinions.items() if str(k).replace("\\", "/").casefold() == key]
    if len(exact) == 1:
        return exact[0]
    name = basename(path).casefold()
    matches = [v for k, v in opinions.items() if basename(k).casefold() == name]
    targets = [p for p in all_paths if basename(p).casefold() == name]
    return matches[0] if len(matches) == 1 and len(targets) <= 1 else None


def check_version(value):
    if value.get("schema_version") != 1:
        raise ValueError("Unsupported schema version; expected 1")


def audio_path(value):
    path = Path(value).expanduser().resolve()
    if not path.is_file():
        raise ValueError(f"Audio file missing: {path}")
    if path.suffix.lower() not in {".wav", ".flac", ".mp3", ".m4a", ".aac", ".aiff", ".aif", ".ogg", ".opus"}:
        raise ValueError("Unsupported audio extension")
    return path
