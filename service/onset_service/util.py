from __future__ import annotations

import hashlib
import json
import os
import tempfile
import time
from datetime import datetime, timezone
from pathlib import Path, PureWindowsPath


def utcnow():
    return datetime.now(timezone.utc).isoformat()


DB_NAME = "workspace.db"


def app_home():
    """The workspace folder: %ONSET_WORKSPACE%, else OnSet's local data folder.

    Matches the desktop app's `directories::ProjectDirs::from("", "Onset", "Onset")` data
    folder on Windows. A workspace left in the folder of OnSet's predecessor (Autocue) is
    moved here once.
    """
    if os.environ.get("ONSET_WORKSPACE"):
        return Path(os.environ["ONSET_WORKSPACE"])
    local = os.environ.get("LOCALAPPDATA")
    if local:
        home, legacy = Path(local) / "Onset" / "Onset" / "data" / "workspace", Path(local) / "Autocue"
    else:
        home, legacy = Path.home() / ".local/share/onset/workspace", Path.home() / ".local/share/Autocue"
    migrate_legacy_home(legacy, home)
    return home


def migrate_legacy_home(legacy, home):
    """Moves a pre-OnSet workspace folder to `home` and renames its database to DB_NAME.

    Does nothing when `home` already exists or there is nothing to move. If the old files
    are in use (another copy of the service is running), leaves everything in place and
    tries again next start.
    """
    legacy, home = Path(legacy), Path(home)
    if home.exists() or not (legacy / "autocue.db").exists():
        return False
    home.parent.mkdir(parents=True, exist_ok=True)
    try:
        legacy.rename(home)
    except OSError:
        return False
    for suffix in ("", "-wal", "-shm"):
        old = home / f"autocue.db{suffix}"
        if old.exists():
            old.rename(home / f"{DB_NAME}{suffix}")
    return True


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


def replace(source, target, attempts=6):
    """os.replace, retried briefly: Windows scanners and indexers can hold a just-written file."""
    for attempt in range(attempts):
        try:
            return os.replace(source, target)
        except PermissionError:
            if attempt == attempts - 1:
                raise
            time.sleep(.05 * (attempt + 1))


def atomic_json(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix=path.name + ".", suffix=".tmp", dir=path.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8", newline="\n") as f:
            f.write(json.dumps(value, indent=2, ensure_ascii=False, allow_nan=False))
            f.flush()
            os.fsync(f.fileno())
        replace(temporary, path)
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
