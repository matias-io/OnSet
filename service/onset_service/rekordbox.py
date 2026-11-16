"""Reviewable, approval-bound database changes with consistent encrypted backups."""
from __future__ import annotations
import copy
import json
import os
from pathlib import Path
import shutil
import sqlite3
import tempfile
import uuid
from .util import digest, utcnow, atomic_json, file_hash

CUE_FIELDS = ("UUID", "Kind", "InMsec", "OutMsec", "Color", "Comment", "BeatLoopSize")


def cue_record(cue):
    return {k: getattr(cue, k, None) for k in CUE_FIELDS}


def consistent_backup(db, destination):
    """Use the driver's online backup API, including committed WAL pages."""
    destination = Path(destination)
    destination.parent.mkdir(parents=True, exist_ok=True)
    if destination.exists():
        raise ValueError("Backup destination already exists")
    encrypted = "pysqlcipher" in db.engine.url.drivername
    if encrypted:
        from sqlcipher3 import dbapi2 as driver
    else:
        driver = sqlite3
    handle = db.engine.raw_connection()
    target = driver.connect(str(destination))
    try:
        if encrypted:
            key = str(db.engine.url.password).replace("'", "''")
            target.execute("PRAGMA key='" + key + "'")
        source = handle.driver_connection
        if not hasattr(source, "backup"):
            raise RuntimeError("Database driver does not support consistent snapshots")
        source.backup(target, pages=256, sleep=.05)
        if target.execute("PRAGMA integrity_check").fetchone()[0] != "ok":
            raise RuntimeError("Snapshot integrity check failed")
        if encrypted:
            errors = target.execute("PRAGMA cipher_integrity_check").fetchall()
            if errors:
                raise RuntimeError("Encrypted snapshot integrity check failed")
    finally:
        target.close()
        handle.close()
    return str(destination)


def backup_auxiliary(db, backup):
    source = Path(db.db_directory) / "masterPlaylists6.xml"
    if not source.is_file():
        return None
    target = Path(str(backup) + ".playlists.xml")
    shutil.copy2(source, target)
    return {"source": str(source), "backup": str(target), "hash": file_hash(target)}


def open_db(path=None, plain=False):
    from pyrekordbox import Rekordbox6Database
    return Rekordbox6Database(path=path, unlock=not plain)


def not_running():
    from pyrekordbox.utils import get_rekordbox_pid
    if get_rekordbox_pid():
        raise ValueError("Close rekordbox before applying changes or restoring")


def supported(db):
    from sqlalchemy import inspect
    schema = inspect(db.engine)
    if not {"djmdContent", "djmdCue", "agentRegistry"}.issubset(schema.get_table_names()):
        raise ValueError("Unsupported rekordbox database schema")
    if not set(CUE_FIELDS).issubset({c["name"] for c in schema.get_columns("djmdCue")}):
        raise ValueError("Unsupported cue schema")


def content_for(db, profile):
    wanted = {str(p).replace("\\", "/").casefold() for p in profile["locations"]}
    matches = [c for c in db.get_content() if str(c.FolderPath or "").replace("\\", "/").casefold() in wanted]
    if len(matches) != 1:
        raise ValueError("Track needs one exact rekordbox location match")
    return matches[0]


def prepare(store, track_ids, path=None, plain=False, hot=False, comments=False):
    from onset_service.cues import UUID_PREFIX, CUE_SCHEME, HOT_SLOT
    from onset_service.cuewriter import is_hot_cue
    db = open_db(path, plain)
    try:
        supported(db)
        changes, conflicts = [], []
        for track_id in track_ids:
            p = store.get(track_id)
            approved = [c for c in p["cues"] if c["review"] in {"approved", "delete-approved"} and c["revision"] == p["revision"]]
            if not approved:
                continue
            if not Path(p["path"]).is_file() or file_hash(p["path"]) != p["fingerprint"]:
                conflicts.append({"track_id":track_id,"reason":"Audio changed or is missing; reimport and reanalyse before writing"})
                continue
            if p.get("overrides", {}).get("grid") and not p.get("overrides", {}).get("accept_rekordbox_grid_conflict"):
                conflicts.append({"track_id": track_id, "reason": "Analysis grid was edited; resolve the rekordbox grid conflict"})
                continue
            try:
                content = content_for(db, p)
            except ValueError as exc:
                conflicts.append({"track_id": track_id, "reason": str(exc)})
                continue
            existing = list(db.get_cue(ContentID=content.ID))
            prior = store.artifact("written", track_id + (":hot" if hot else ":memory")) or {"cues": []}
            previous = {c["UUID"]: c for c in prior["cues"]}
            before = [cue_record(c) for c in existing]
            operations, slots = [], set()
            for proposal in approved:
                role = proposal["role"]
                if hot and role not in HOT_SLOT:
                    continue
                kind = HOT_SLOT[role] if hot else (4 if proposal.get("end") is not None else 0)
                # A hot rewrite replaces the cue on the target pad and OnSet's own cue of that name on
                # any other pad, so cues from the old off-by-one slot numbering move instead of doubling.
                old = [c for c in existing if is_hot_cue(c) == hot and
                       ((hot and (c.Kind == kind or ((c.UUID or "").startswith(UUID_PREFIX) and c.Comment == CUE_SCHEME[role][0])))
                        or (not hot and c.Comment == CUE_SCHEME[role][0]))]
                if any(not (c.UUID or "").startswith(UUID_PREFIX) or previous.get(c.UUID) != cue_record(c) for c in old):
                    conflicts.append({"track_id": track_id, "role": role, "reason": "Manual, changed, or untracked existing cue preserved"})
                    continue
                if proposal["review"] == "delete-approved":
                    if old: operations.append({"role": role, "delete": [c.UUID for c in old], "target": None})
                    continue
                loop_beats = 0
                if proposal.get("end") is not None:
                    analysis = p.get("analysis") or {}
                    beats = analysis.get("beats", [])
                    if not beats:
                        conflicts.append({"track_id": track_id, "role": role, "reason": "Loop requires the actual beat grid"})
                        continue
                    import numpy as np
                    indices = [int(np.argmin(abs(np.asarray(beats) - t))) for t in (proposal["time"], proposal["end"])]
                    if any(abs(beats[i] - t) > .05 for i, t in zip(indices, (proposal["time"], proposal["end"]))) or indices[1] <= indices[0]:
                        bars = np.asarray(analysis.get("downbeats") or [])
                        stale = len(bars) > 1 and all(min(abs(bars - t)) <= .05 for t in (proposal["time"], proposal["end"]))
                        conflicts.append({"track_id": track_id, "role": role,
                                          "reason": "Stored beats disagree with the bar grid the cue sits on; reanalyse before writing loops"
                                          if stale else "Loop endpoints must align with the beat grid"})
                        continue
                    loop_beats = indices[1] - indices[0]
                target = {"Kind": kind, "InMsec": round(proposal["time"] * 1000),
                          "OutMsec": round(proposal["end"] * 1000) if proposal.get("end") is not None else -1,
                          "Color": CUE_SCHEME[role][1], "Comment": CUE_SCHEME[role][0],
                          "BeatLoopSize": 0x10000 * loop_beats + 1 if loop_beats else 0}
                if len(old) == 1 and all(getattr(old[0], k) == v for k, v in target.items()):
                    continue
                operations.append({"role": role, "delete": [c.UUID for c in old], "target": target})
            if not hot:
                removed = sum(len(o["delete"]) for o in operations)
                memory_count = sum(not is_hot_cue(c) for c in existing)
                for optional in ("I", "L", "G"):
                    if memory_count - removed + sum(o["target"] is not None for o in operations) > 10:
                        dropped = [o for o in operations if o["role"] == optional and o["target"] is not None]
                        operations = [o for o in operations if o not in dropped]
                        removed -= sum(len(o["delete"]) for o in dropped)
                        if dropped:
                            conflicts.append({"track_id": track_id, "role": optional, "reason": "Optional cue omitted: memory limit"})
                if memory_count - removed + sum(o["target"] is not None for o in operations) > 10:
                    conflicts.append({"track_id": track_id, "reason": "Memory cue limit exceeded"})
                    operations = []
            new_comment = content.Commnt or ""
            if comments:
                import re
                # Earlier versions, under the name Autocue, wrote [Autocue]...[/Autocue]; replace either block.
                original = re.sub(r"\s*\[(Autocue|OnSet)\].*?\[/\1\]", "", new_comment, flags=re.S).rstrip()
                addition = f"[OnSet]{len(approved)} approved cues[/OnSet]"
                if len(original + " " + addition) <= 255:
                    new_comment = (original + " " + addition).strip()
                else:
                    conflicts.append({"track_id": track_id, "reason": "Comments preserved: insufficient space"})
            if operations or new_comment != (content.Commnt or ""):
                changes.append({"track_id": track_id, "revision": p["revision"], "content_id": content.ID,
                                "approval_digest": digest(approved), "before": before, "operations": operations,
                                "comment_before": content.Commnt or "", "comment_after": new_comment})
        record = {"schema_version": 1, "track_ids": track_ids, "db_path": str(Path(db.engine.url.database).resolve()),
                  "plain": plain, "hot": hot, "comments": comments, "changes": changes, "conflicts": conflicts, "stale": False}
        record["id"] = digest(record)[:24]
        store.artifact("write-preview", record["id"], record)
        return record
    finally:
        db.close()
        db.engine.dispose()


def apply(store, preview_id):
    from onset_service.cuewriter import make_uuid
    from pyrekordbox.db6 import tables
    preview = store.artifact("write-preview", preview_id)
    if not preview or preview.get("stale"):
        raise ValueError("Write preview is missing or stale")
    # Adoption only records ownership in the workspace and reads rekordbox, so it needs no
    # closed rekordbox; every write below does.
    if preview.get("kind") == "cue-adoption":return apply_adoption(store,preview)
    if preview.get("kind") == "cue-adoption-library":return apply_library_adoption(store,preview)
    not_running()
    if preview.get("kind") == "playlist":
        return apply_playlist(store, preview)
    if preview.get("kind") == "hot-slot-repair":
        return apply_hot_slot_repair(store, preview)
    fresh = prepare(store, preview["track_ids"], preview["db_path"], preview["plain"], preview["hot"], preview["comments"])
    if fresh["id"] != preview_id:
        raise ValueError("Database or approvals changed; review a new preview")
    db = open_db(preview["db_path"], preview["plain"])
    backup = Path(store.home) / "backups" / (uuid.uuid4().hex + ".db.bak")
    try:
        consistent_backup(db, backup)
        auxiliary = backup_auxiliary(db, backup)
        not_running()
        # sqlite3's legacy transaction mode does not BEGIN for SELECT/SAVEPOINT.
        # Without a real outer transaction, releasing the first per-track
        # savepoint commits it and a later rollback cannot undo the cue write.
        db.session.connection().exec_driver_sql("BEGIN IMMEDIATE")
        written, failed = [], []
        for change in preview["changes"]:
            registry_before = list(db.registry.__update_sequence__)
            try:
                with db.session.begin_nested():
                    c = db.get_content(ID=change["content_id"])
                    if sorted([cue_record(x) for x in db.get_cue(ContentID=c.ID)], key=lambda x: x["UUID"] or "") != sorted(change["before"], key=lambda x: x["UUID"] or "") or (c.Commnt or "") != change["comment_before"]:
                        raise ValueError("Database changed after preview")
                    for operation in change["operations"]:
                        for uid in operation["delete"]:
                            for cue in list(db.get_cue(ContentID=c.ID)):
                                if cue.UUID == uid:
                                    db.delete(cue)
                        t = operation["target"]
                        if t is None: continue
                        import datetime as dt
                        now = dt.datetime.now()
                        cue = tables.DjmdCue(ID=str(db.generate_unused_id(tables.DjmdCue)), ContentID=c.ID,
                            UUID=make_uuid(), ContentUUID=c.UUID, InFrame=int(t["InMsec"] * .15),
                            OutFrame=int(t["OutMsec"] * .15) if t["OutMsec"] >= 0 else 0,
                            InMpegFrame=0, InMpegAbs=0, OutMpegFrame=0, OutMpegAbs=0, ActiveLoop=0,
                            CueMicrosec=0, ColorTableIndex=None, created_at=now, updated_at=now, **t)
                        db.add(cue)
                    c.Commnt = change["comment_after"]
                    db.flush()
                    written.append({"track_id": change["track_id"], "content_id": c.ID,
                                    "cues": [cue_record(x) for x in db.get_cue(ContentID=c.ID)]})
            except Exception as exc:
                db.registry.__update_sequence__[:] = registry_before
                failed.append({"track_id": change["track_id"], "error": str(exc)})
        not_running()
        db.commit()
        owned_before = {}
        for row in written:
            actual = [cue_record(x) for x in db.get_cue(ContentID=row["content_id"])]
            if sorted(actual, key=lambda c: c["UUID"]) != sorted(row["cues"], key=lambda c: c["UUID"]):
                raise RuntimeError("Post-commit verification failed; backup retained")
            key = row["track_id"] + (":hot" if preview["hot"] else ":memory")
            owned_before[key] = store.artifact("written", key) or {"cues": []}
            store.artifact("written", key, row)
            p = store.get(row["track_id"])
            p["write_state"] = "written"
            store.save(p)
        manifest = {"schema_version": 1, "id": uuid.uuid4().hex, "preview_id": preview_id,
                    "backup": str(backup), "backup_hash": file_hash(backup), "db_path": preview["db_path"],
                    "plain": preview["plain"], "written": written, "failed": failed, "created": utcnow(),
                    "owned_before": owned_before}
        manifest["auxiliary"] = auxiliary
        atomic_json(backup.with_suffix(".json"), manifest)
        store.artifact("write-manifest", manifest["id"], manifest)
        preview["stale"] = True
        store.artifact("write-preview", preview_id, preview)
        return manifest
    except Exception:
        db.rollback()
        raise
    finally:
        db.close()
        db.engine.dispose()


def restore(store, manifest_id):
    not_running()
    manifest = store.artifact("write-manifest", manifest_id)
    if manifest:
        if manifest.get("schema_version", 1) != 1:
            raise ValueError("Unsupported write manifest schema version; expected 1")
    if not manifest or not Path(manifest["backup"]).is_file() or file_hash(manifest["backup"]) != manifest["backup_hash"]:
        raise ValueError("Backup missing or checksum mismatch")
    auxiliary = manifest.get("auxiliary")
    if auxiliary and (not Path(auxiliary["backup"]).is_file() or file_hash(auxiliary["backup"]) != auxiliary["hash"]):
        raise ValueError("Playlist backup checksum mismatch")
    target = Path(manifest["db_path"])
    db = open_db(target, manifest["plain"])
    try:
        current = target.with_name(target.name + ".before-restore-" + uuid.uuid4().hex + ".bak")
        consistent_backup(db, current)
        if auxiliary and Path(auxiliary["source"]).exists():
            shutil.copy2(auxiliary["source"], current.with_suffix(".xml.bak"))
    finally:
        db.close()
        db.engine.dispose()
    not_running()
    # Preserve sidecars rather than replaying WAL pages from the replaced database.
    for suffix in ("-wal", "-shm"):
        side = target.with_name(target.name + suffix)
        if side.exists():
            side.replace(current.with_name(current.name + suffix))
    temporary = target.with_name(target.name + ".restore.tmp")
    shutil.copy2(manifest["backup"], temporary)
    os.replace(temporary, target)
    if auxiliary:
        shutil.copy2(auxiliary["backup"], auxiliary["source"])
    # The restored cues are the ones OnSet owned before that write; without this they would read as
    # manual and every later write to those tracks would be refused. Older manifests lack the record.
    for key, record in (manifest.get("owned_before") or {}).items():
        store.artifact("written", key, record)
    for row in manifest["written"]:
        p = store.get(row["track_id"])
        p["write_state"] = "stale"
        store.save(p)
    return {"restored": str(target), "previous_backup": str(current)}


def prepare_playlist(store, plan_id, name, path=None, plain=False):
    from .planning import validate_plan
    plan = store.artifact("set", plan_id)
    if not plan or plan.get("stale"):
        raise ValueError("Set is missing or stale")
    validate_plan(plan, store.tracks(), {o["id"]: o for o in store.artifacts("transition")}, plan.get("brief"))
    if not isinstance(name, str) or not name.strip() or len(name) > 100:
        raise ValueError("Playlist name must contain 1–100 characters")
    db = open_db(path, plain)
    try:
        supported(db)
        if any(p.Name == name for p in db.get_playlist()):
            raise ValueError("Playlist already exists; choose a new name")
        entries = [{"track_id": tid, "content_id": content_for(db, store.get(tid)).ID,
                    "title": store.get(tid)["title"]} for tid in plan["track_ids"]]
        preview = {"schema_version": 1, "kind": "playlist", "track_ids": plan["track_ids"], "plan_id": plan_id,
                   "plan_digest": digest(plan), "name": name.strip(), "entries": entries, "plain": plain,
                   "db_path": str(Path(db.engine.url.database).resolve()), "stale": False}
        preview["id"] = digest(preview)[:24]
        return store.artifact("write-preview", preview["id"], preview)
    finally:
        db.close(); db.engine.dispose()


def prepare_adoption(store, track_id, path=None, plain=False, hot=False):
    from onset_service.cues import UUID_PREFIX
    from onset_service.cuewriter import is_hot_cue
    track=store.get(track_id);db=open_db(path,plain)
    try:
        supported(db);content=content_for(db,track)
        cues=[cue_record(c) for c in db.get_cue(ContentID=content.ID) if (c.UUID or '').startswith(UUID_PREFIX) and is_hot_cue(c)==hot]
        preview={"schema_version":1,"kind":"cue-adoption","track_ids":[track_id],"db_path":str(Path(db.engine.url.database).resolve()),
                 "plain":plain,"hot":hot,"content_id":content.ID,"cues":cues,"stale":False}
        preview["id"]=digest(preview)[:24];store.artifact('write-preview',preview['id'],preview)
        return preview
    finally:db.close();db.engine.dispose()


def apply_adoption(store,preview):
    fresh=prepare_adoption(store,preview['track_ids'][0],preview['db_path'],preview['plain'],preview['hot'])
    if fresh['id']!=preview['id']:raise ValueError('Existing generated cues changed; review adoption again')
    if not preview['cues']:raise ValueError('No generated cues to adopt')
    tid=preview['track_ids'][0]
    store.artifact('written',tid+(':hot' if preview['hot'] else ':memory'),{'track_id':tid,'content_id':preview['content_id'],'cues':preview['cues'],'adopted':utcnow()})
    preview['stale']=True;store.artifact('write-preview',preview['id'],preview)
    return {'adopted':len(preview['cues']),'track_id':tid,'database_modified':False}


def prepare_hot_slot_repair(store, path=None, plain=False):
    """OnSet's own hot cues still on the numbering used before 2026-10-22 (D..L on Kind 4..8), each
    moved to its rekordbox pad with time, colour and name unchanged. Only a cue with the OnSet
    prefix, a scheme name and exactly that name's old Kind moves. One whose new pad keeps another
    cue (the DJ's own, say) is removed when the same cue is also a memory cue, which frees its old
    pad for the cue behind it; without a memory cue it stays where it is and is listed, which can
    in turn keep the cue that would take its pad."""
    from onset_service.cues import UUID_PREFIX, CUE_SCHEME, HOT_SLOT, LEGACY_HOT_SLOT
    from onset_service.cuewriter import is_hot_cue
    names = {CUE_SCHEME[role][0]: role for role in LEGACY_HOT_SLOT}
    db = open_db(path, plain)
    try:
        supported(db)
        contents = {str(c.ID): c for c in db.get_content()}
        by_content = {}
        for c in db.get_cue():
            by_content.setdefault(str(c.ContentID), []).append(c)
        moves, drops, conflicts = [], [], []
        for content_id, cues in sorted(by_content.items()):
            pending = [c for c in cues if (c.UUID or "").startswith(UUID_PREFIX) and c.Comment in names
                       and c.Kind == LEGACY_HOT_SLOT[names[c.Comment]]]
            dropped = []
            while True:
                leaving = {c.UUID for c in pending + dropped}
                targets = [HOT_SLOT[names[c.Comment]] for c in pending]
                blocked = [c for c in pending if targets.count(HOT_SLOT[names[c.Comment]]) > 1 or
                           any(o.Kind == HOT_SLOT[names[c.Comment]] and o.UUID not in leaving for o in cues)]
                if not blocked:
                    break
                for c in blocked:
                    pending.remove(c)
                    shared = targets.count(HOT_SLOT[names[c.Comment]]) > 1
                    if not shared and any(not is_hot_cue(o) and o.Comment == c.Comment for o in cues):
                        dropped.append(c)
                        continue
                    conflicts.append({"content_id": content_id, "title": contents[content_id].Title if content_id in contents else None,
                                      "cue": c.Comment, "reason": "Another cue holds its rekordbox pad; left where it is"})
            moves += [{"content_id": content_id, "title": contents[content_id].Title if content_id in contents else None,
                       "uuid": c.UUID, "before": cue_record(c), "kind": HOT_SLOT[names[c.Comment]]} for c in pending]
            drops += [{"content_id": content_id, "title": contents[content_id].Title if content_id in contents else None,
                       "uuid": c.UUID, "before": cue_record(c)} for c in dropped]
        by_path = {}
        for c in contents.values():
            by_path.setdefault(str(c.FolderPath or "").replace("\\", "/").casefold(), []).append(str(c.ID))
        moved = {m["content_id"] for m in moves + drops}
        owners = {}
        for t in store.tracks():
            ids = {i for loc in t["locations"] for i in by_path.get(str(loc).replace("\\", "/").casefold(), [])}
            if len(ids) == 1 and next(iter(ids)) in moved:
                owners.setdefault(next(iter(ids)), []).append(t["id"])
        preview = {"schema_version": 1, "kind": "hot-slot-repair", "db_path": str(Path(db.engine.url.database).resolve()),
                   "plain": plain, "moves": moves, "drops": drops, "conflicts": conflicts, "owners": owners, "stale": False}
        preview["id"] = digest(preview)[:24]
        store.artifact("write-preview", preview["id"], preview)
        return preview
    finally:
        db.close()
        db.engine.dispose()


def apply_hot_slot_repair(store, preview):
    import datetime as dt
    fresh = prepare_hot_slot_repair(store, preview["db_path"], preview["plain"])
    if fresh["id"] != preview["id"]:
        raise ValueError("Database changed; review the pad repair again")
    if not preview["moves"] and not preview.get("drops"):
        raise ValueError("No hot cues to move")
    db = open_db(preview["db_path"], preview["plain"])
    backup = Path(store.home) / "backups" / (uuid.uuid4().hex + ".db.bak")
    try:
        consistent_backup(db, backup)
        auxiliary = backup_auxiliary(db, backup)
        not_running()
        db.session.connection().exec_driver_sql("BEGIN IMMEDIATE")
        now = dt.datetime.now()
        for drop in preview.get("drops", []):
            cue = next((c for c in db.get_cue(ContentID=drop["content_id"]) if c.UUID == drop["uuid"]), None)
            if cue is None or cue_record(cue) != drop["before"]:
                raise ValueError("Database changed after preview")
            db.delete(cue)
        for move in preview["moves"]:
            cue = next((c for c in db.get_cue(ContentID=move["content_id"]) if c.UUID == move["uuid"]), None)
            if cue is None or cue_record(cue) != move["before"]:
                raise ValueError("Database changed after preview")
            cue.Kind = move["kind"]
            cue.updated_at = now
        db.flush()
        not_running()
        db.commit()
        for move in preview["moves"]:
            cue = next(c for c in db.get_cue(ContentID=move["content_id"]) if c.UUID == move["uuid"])
            if cue.Kind != move["kind"]:
                raise RuntimeError("Post-commit verification failed; backup retained")
        gone = {d["uuid"] for d in preview.get("drops", [])}
        if any(c.UUID in gone for d in preview.get("drops", []) for c in db.get_cue(ContentID=d["content_id"])):
            raise RuntimeError("Post-commit verification failed; backup retained")
        kinds = {m["uuid"]: m["kind"] for m in preview["moves"]}
        owned_before = {}
        for track_ids in preview["owners"].values():
            for track_id in track_ids:
                key = track_id + ":hot"
                record = store.artifact("written", key)
                if not record or not any(c["UUID"] in kinds or c["UUID"] in gone for c in record["cues"]):
                    continue
                owned_before[key] = record
                store.artifact("written", key, {**record, "cues": [{**c, "Kind": kinds.get(c["UUID"], c["Kind"])}
                                                                    for c in record["cues"] if c["UUID"] not in gone]})
        manifest = {"schema_version": 1, "id": uuid.uuid4().hex, "preview_id": preview["id"], "kind": "hot-slot-repair",
                    "backup": str(backup), "backup_hash": file_hash(backup), "db_path": preview["db_path"],
                    "plain": preview["plain"], "written": [], "failed": [], "moved": len(preview["moves"]), "dropped": len(gone),
                    "created": utcnow(), "auxiliary": auxiliary, "owned_before": owned_before}
        atomic_json(backup.with_suffix(".json"), manifest)
        store.artifact("write-manifest", manifest["id"], manifest)
        preview["stale"] = True
        store.artifact("write-preview", preview["id"], preview)
        return manifest
    except Exception:
        db.rollback()
        raise
    finally:
        db.close()
        db.engine.dispose()


def prepare_library_adoption(store, path=None, plain=False):
    """Every track's cues from earlier OnSet versions, hot and memory, in one preview. A track
    matched to no rekordbox entry, or to several, is listed as skipped rather than guessed;
    manual cues (no OnSet prefix) are never included."""
    from onset_service.cues import UUID_PREFIX
    from onset_service.cuewriter import is_hot_cue
    db = open_db(path, plain)
    try:
        supported(db)
        by_path = {}
        for c in db.get_content():
            by_path.setdefault(str(c.FolderPath or "").replace("\\", "/").casefold(), []).append(c)
        generated = {}
        for c in db.get_cue():
            if (c.UUID or "").startswith(UUID_PREFIX):
                generated.setdefault(str(c.ContentID), []).append(c)
        tracks, skipped = [], []
        for t in store.tracks():
            matches = {c.ID: c for loc in t["locations"]
                       for c in by_path.get(str(loc).replace("\\", "/").casefold(), [])}
            if len(matches) != 1:
                skipped.append({"track_id": t["id"], "title": t.get("title"),
                                "reason": "several rekordbox entries" if matches else "not in rekordbox"})
                continue
            content = next(iter(matches.values()))
            mine = generated.get(str(content.ID), [])
            for hot in (False, True):
                cues = [cue_record(c) for c in mine if is_hot_cue(c) == hot]
                if cues:
                    tracks.append({"track_id": t["id"], "title": t.get("title"), "content_id": content.ID,
                                   "hot": hot, "cues": cues})
        preview = {"schema_version": 1, "kind": "cue-adoption-library",
                   "db_path": str(Path(db.engine.url.database).resolve()), "plain": plain,
                   "tracks": tracks, "skipped": skipped, "stale": False}
        preview["id"] = digest(preview)[:24]
        store.artifact("write-preview", preview["id"], preview)
        return preview
    finally:
        db.close(); db.engine.dispose()


def apply_library_adoption(store, preview):
    fresh = prepare_library_adoption(store, preview["db_path"], preview["plain"])
    if fresh["id"] != preview["id"]:
        raise ValueError("Cues in rekordbox changed; find earlier cues again")
    if not preview["tracks"]:
        raise ValueError("No generated cues to adopt")
    adopted = utcnow()
    for entry in preview["tracks"]:
        store.artifact("written", entry["track_id"] + (":hot" if entry["hot"] else ":memory"),
                       {"track_id": entry["track_id"], "content_id": entry["content_id"],
                        "cues": entry["cues"], "adopted": adopted})
    preview["stale"] = True
    store.artifact("write-preview", preview["id"], preview)
    return {"adopted": sum(len(e["cues"]) for e in preview["tracks"]),
            "tracks": len({e["track_id"] for e in preview["tracks"]}),
            "skipped": len(preview["skipped"]), "database_modified": False}


def apply_playlist(store, preview):
    not_running()
    fresh = prepare_playlist(store, preview["plan_id"], preview["name"], preview["db_path"], preview["plain"])
    if fresh["id"] != preview["id"]:
        raise ValueError("Playlist preview changed; review again")
    db = open_db(preview["db_path"], preview["plain"])
    backup = Path(store.home) / "backups" / (uuid.uuid4().hex + ".db.bak")
    try:
        consistent_backup(db, backup)
        auxiliary = backup_auxiliary(db, backup)
        not_running()
        db.session.connection().exec_driver_sql("BEGIN IMMEDIATE")
        playlist = db.create_playlist(preview["name"])
        for index, entry in enumerate(preview["entries"], 1):
            db.add_to_playlist(playlist, entry["content_id"], track_no=index)
        not_running()
        db.commit()
        songs = list(db.get_playlist_songs(PlaylistID=playlist.ID))
        if [str(s.ContentID) for s in sorted(songs, key=lambda s: s.TrackNo)] != [str(e["content_id"]) for e in preview["entries"]]:
            raise RuntimeError("Playlist verification failed; backup retained")
        manifest = {"schema_version": 1, "id": uuid.uuid4().hex, "created": utcnow(), "preview_id": preview["id"],
                    "backup": str(backup), "backup_hash": file_hash(backup), "db_path": preview["db_path"],
                    "plain": preview["plain"], "written": [], "failed": [], "playlist": preview["name"], "auxiliary": auxiliary}
        atomic_json(backup.with_suffix(".json"), manifest)
        store.artifact("write-manifest", manifest["id"], manifest)
        preview["stale"] = True
        store.artifact("write-preview", preview["id"], preview)
        return manifest
    except Exception:
        db.rollback()
        raise
    finally:
        db.close(); db.engine.dispose()
