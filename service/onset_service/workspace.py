"""Consistent, app-owned recovery snapshots. Music and credentials stay separate."""
from __future__ import annotations

import json
import os
from pathlib import Path
import sqlite3
import uuid

from .util import atomic_json, digest, file_hash, utcnow

TABLES = ("tracks", "aliases", "revisions", "edits", "jobs", "artifacts", "settings")


def counts(db):
    return {table: db.execute(f"SELECT count(*) FROM {table}").fetchone()[0] for table in TABLES}


def identity(path):
    info = Path(path).stat()
    return [info.st_dev, info.st_ino]


def health(store):
    with store.lock:
        warnings = list(store.workspace_warnings)
        try:
            if identity(store.db_path) != store.file_identity:
                warnings.append("The database file was replaced while OnSet was open. Restart before making changes.")
            current = counts(store.db)
            for table in ("tracks", "revisions", "jobs"):
                if current[table] < store.known_counts.get(table, 0):
                    warnings.append(f"{table}: expected at least {store.known_counts[table]}, found {current[table]}. Review a workspace backup.")
        except (OSError, sqlite3.DatabaseError) as error:
            current = {}
            warnings.append(str(error))
        return {"schema_version": 1, "home": str(store.home.resolve()), "database": str(store.db_path.resolve()),
                "workspace_id": store.workspace_id, "counts": current, "last_known_counts": store.known_counts,
                "file_identity": store.file_identity, "process_id": os.getpid(),
                "healthy": not warnings, "warnings": warnings}


def guard(store):
    status = health(store)
    if not status["healthy"]:
        raise ValueError("Workspace needs recovery review: " + " ".join(status["warnings"]))


def remember(store, reset=False):
    current = counts(store.db)
    store.known_counts = current if reset else {k: max(current[k], store.known_counts.get(k, 0)) for k in current}
    atomic_json(store.home / "workspace-state.json", {"schema_version": 1, "workspace_id": store.workspace_id,
                "counts": store.known_counts, "updated": utcnow()})


def backup(store, reason="manual", allow_unhealthy=False):
    with store.lock:
        if not allow_unhealthy:
            guard(store)
        folder = store.home / "workspace-backups"
        folder.mkdir(parents=True, exist_ok=True)
        backup_id = uuid.uuid4().hex
        destination = folder / (backup_id + ".db")
        temporary = folder / (backup_id + ".tmp")
        target = sqlite3.connect(temporary)
        try:
            store.db.backup(target)
            if target.execute("PRAGMA integrity_check").fetchone()[0] != "ok":
                raise ValueError("Workspace snapshot failed integrity validation")
            snapshot_counts = counts(target)
        finally:
            target.close()
        os.replace(temporary, destination)
        record = {"schema_version": 1, "id": backup_id, "workspace_id": store.workspace_id,
                  "created": utcnow(), "reason": reason, "counts": snapshot_counts,
                  "sha256": file_hash(destination), "bytes": destination.stat().st_size}
        atomic_json(folder / (backup_id + ".json"), record)
        return record


def backups(store):
    result = []
    for path in (store.home / "workspace-backups").glob("*.json"):
        try:
            value = json.loads(path.read_text(encoding="utf-8"))
            if value.get("schema_version") == 1 and value.get("id") == path.stem:
                result.append(value)
        except (OSError, ValueError):
            continue
    return sorted(result, key=lambda record: record["created"], reverse=True)


def verified_backup(store, backup_id):
    if not isinstance(backup_id, str) or len(backup_id) != 32 or any(c not in "0123456789abcdef" for c in backup_id):
        raise ValueError("Invalid workspace backup ID")
    record = next((r for r in backups(store) if r["id"] == backup_id), None)
    path = store.home / "workspace-backups" / (backup_id + ".db")
    if not record or not path.is_file() or file_hash(path) != record["sha256"]:
        raise ValueError("Workspace backup is missing or its checksum changed")
    return record, path


def preview(store, backup_id):
    with store.lock:
        if identity(store.db_path) != store.file_identity:
            raise ValueError("Restart OnSet before restoring a replaced database file")
        record, path = verified_backup(store, backup_id)
        db = sqlite3.connect(f"file:{path.resolve().as_posix()}?mode=ro", uri=True)
        try:
            if db.execute("PRAGMA user_version").fetchone()[0] != 1 or db.execute("PRAGMA integrity_check").fetchone()[0] != "ok":
                raise ValueError("Unsupported or damaged workspace backup")
            after = counts(db)
        finally:
            db.close()
        before = counts(store.db)
        # Content, rather than counts alone, binds the approval to current edits.
        state = {table: [list(row) for row in store.db.execute(f"SELECT * FROM {table} ORDER BY rowid")] for table in TABLES}
        token = digest([record["id"], record["sha256"], state])
        return {"schema_version": 1, "id": backup_id, "token": token, "created": record["created"],
                "before": before, "after": after, "preserved": "Music files, credentials and rekordbox remain separate",
                "replaces": "All local profiles, revisions, corrections, jobs, settings and saved plans",
                "rollback_backup": "A consistent snapshot of the current database is saved before restoration"}


def restore(store, backup_id, token):
    with store.lock:
        proposed = preview(store, backup_id)
        if token != proposed["token"]:
            raise ValueError("Workspace changed after the preview; review a fresh preview")
        _, path = verified_backup(store, backup_id)
        rollback = backup(store, "before-restore", allow_unhealthy=True)
        source = sqlite3.connect(f"file:{path.resolve().as_posix()}?mode=ro", uri=True)
        try:
            source.backup(store.db)
        finally:
            source.close()
        store.workspace_warnings = []
        store.file_identity = identity(store.db_path)
        # Restored active jobs need an explicit resume, never automatic execution.
        for row in store.db.execute("SELECT id,record FROM jobs").fetchall():
            job = json.loads(row[1])
            if job["state"] in {"running", "queued", "cancelling"}:
                job.update(state="paused", error="Restored from a workspace backup; resume to continue")
                store.db.execute("UPDATE jobs SET record=? WHERE id=?", (json.dumps(job), row[0]))
        store.db.commit()
        remember(store, reset=True)
        return {"restored": backup_id, "rollback_backup": rollback["id"], "counts": counts(store.db)}
