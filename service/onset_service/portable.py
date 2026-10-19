"""Moving workspaces between machines and relocating external caches.

Audio identity is always rechecked by checksum after a path is remapped; a
matching filename or folder is never treated as proof of the same audio."""
from __future__ import annotations
import json
import sqlite3
from pathlib import Path, PureWindowsPath


def _norm(path):
    return str(path).replace("\\", "/").rstrip("/")


def parse_rules(rules):
    parsed = []
    for rule in rules or []:
        if not isinstance(rule, dict) or not str(rule.get("from") or "").strip() or not str(rule.get("to") or "").strip():
            raise ValueError("Each path rule needs non-empty 'from' and 'to' folders")
        parsed.append((_norm(rule["from"]), str(rule["to"])))
    # Longest prefix first so nested folders win over their parents.
    return sorted(parsed, key=lambda r: -len(r[0]))


def remap(path, rules):
    if not isinstance(path, str) or not path:
        return path
    norm = _norm(path)
    for source, target in rules:
        if norm.casefold() == source.casefold():
            return str(Path(target))
        if norm.casefold().startswith(source.casefold() + "/"):
            rest = norm[len(source) + 1:]
            return str(Path(target).joinpath(*PureWindowsPath(rest).parts))
    return path


def remap_tree(value, rules):
    """Rewrite every string under a remapped prefix; returns (new value, rewritten count)."""
    count = 0
    def visit(item):
        nonlocal count
        if isinstance(item, str):
            new = remap(item, rules)
            count += new != item
            return new
        if isinstance(item, list):
            return [visit(v) for v in item]
        if isinstance(item, dict):
            return {k: visit(v) for k, v in item.items()}
        return item
    return visit(value), count


def remap_profile(record, rules):
    record = dict(record)
    record["path"] = remap(record.get("path"), rules)
    record["locations"] = list(dict.fromkeys(remap(p, rules) for p in record.get("locations", [])))
    return record


def read_sqlite_export(path):
    """Read a portable SQLite workspace export as workspace JSON plus raw history rows."""
    uri = Path(path).resolve().as_uri() + "?mode=ro"
    db = sqlite3.connect(uri, uri=True)
    try:
        if db.execute("PRAGMA user_version").fetchone()[0] != 1:
            raise ValueError("Unsupported workspace export version")
        if db.execute("PRAGMA integrity_check").fetchone()[0] != "ok":
            raise ValueError("Workspace export failed its integrity check")
        tracks = [json.loads(r[0]) for r in db.execute("SELECT profile FROM tracks ORDER BY id")]
        artifacts = {}
        for kind, record in db.execute("SELECT kind, record FROM artifacts"):
            artifacts.setdefault(kind, []).append(json.loads(record))
        revisions = [tuple(r) for r in db.execute("SELECT id, track_id, profile FROM revisions")]
        edits = [tuple(r) for r in db.execute("SELECT track_id, before_value, after_value, undone, created FROM edits ORDER BY id")]
    finally:
        db.close()
    workspace = {"schema_version": 1, "kind": "workspace", "tracks": tracks,
                 "transitions": artifacts.get("transition", []), "plans": artifacts.get("set", []),
                 "benchmark_labels": artifacts.get("benchmark-label", [])}
    return workspace, {"revisions": revisions, "edits": edits}


def relocate_cache(store, source, target, rules=None):
    """Point stored analysis at feature/stem/model files moved from `source` to `target`."""
    rules = parse_rules(rules or [{"from": source, "to": target}])
    if not Path(target).is_dir():
        raise ValueError("Move the cache folder first; the new location does not exist")
    updated, missing = [], []
    for track in store.tracks():
        analysis = track.get("analysis")
        if not analysis:
            continue
        new, count = remap_tree(analysis, rules)
        if not count:
            continue
        paths = [new.get("features", {}).get("path"), *new.get("deep", {}).get("stems", {}).values()]
        absent = [p for p in paths if p and not Path(p).is_file()]
        if absent:
            missing.append({"track_id": track["id"], "missing": absent[:5]})
            continue
        track["analysis"] = new
        store.save(track)
        updated.append(track["id"])
    # Cached resolved results inside the moved folder carry the same absolute paths.
    rewritten = 0
    for result in Path(target).rglob("*.json"):
        try:
            value = json.loads(result.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            continue
        new, count = remap_tree(value, rules)
        if count:
            from .util import atomic_json
            atomic_json(result, new)
            rewritten += 1
    return {"updated": len(updated), "track_ids": updated, "missing": missing, "cache_files_rewritten": rewritten}
