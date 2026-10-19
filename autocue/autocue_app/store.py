"""SQLite owns mutable app state; observations and analysis revisions are immutable."""
from __future__ import annotations
import copy
import json
import sqlite3
import threading
import uuid
from pathlib import Path
from .util import app_home, canonical, digest, file_hash, utcnow
from . import workspace


class Store:
    def __init__(self, home=None):
        self.home = Path(home or app_home())
        self.home.mkdir(parents=True, exist_ok=True)
        self.lock = threading.RLock()
        self.db_path = self.home / "autocue.db"
        self.db = sqlite3.connect(self.db_path, check_same_thread=False)
        self.db.row_factory = sqlite3.Row
        self.db.execute("PRAGMA journal_mode=WAL")
        version = self.db.execute("PRAGMA user_version").fetchone()[0]
        if version > 1:
            raise ValueError("Application database is newer than this version")
        self.db.executescript("""
        CREATE TABLE IF NOT EXISTS tracks(id TEXT PRIMARY KEY, fingerprint TEXT UNIQUE, profile TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS aliases(path TEXT PRIMARY KEY, track_id TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS revisions(id TEXT PRIMARY KEY, track_id TEXT, profile TEXT);
        CREATE TABLE IF NOT EXISTS edits(id INTEGER PRIMARY KEY, track_id TEXT, before_value TEXT, after_value TEXT, undone INTEGER DEFAULT 0, created TEXT);
        CREATE TABLE IF NOT EXISTS jobs(id TEXT PRIMARY KEY, record TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS artifacts(kind TEXT, id TEXT, record TEXT NOT NULL, PRIMARY KEY(kind,id));
        CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY, value TEXT);
        PRAGMA user_version=1;
        """)
        self.db.commit()
        self.file_identity = workspace.identity(self.db_path)
        self.workspace_warnings = []
        self.known_counts = {}
        self.workspace_id = uuid.uuid4().hex
        marker = self.home / "workspace-state.json"
        if marker.exists():
            try:
                previous = json.loads(marker.read_text(encoding="utf-8"))
                if previous.get("schema_version") != 1: raise ValueError("Unsupported workspace marker")
                if not isinstance(previous.get("workspace_id"),str) or not isinstance(previous.get("counts"),dict):
                    raise ValueError("Malformed workspace marker")
                if any(not isinstance(v,int) or isinstance(v,bool) or v<0 for v in previous["counts"].values()):
                    raise ValueError("Malformed workspace counts")
                self.workspace_id = previous["workspace_id"]
                self.known_counts = previous["counts"]
            except (OSError, ValueError, KeyError):
                self.workspace_warnings.append("Workspace history marker is unreadable; review backups before making changes.")
        if workspace.health(self)["healthy"]:
            workspace.remember(self)
            if self.known_counts.get("tracks"):
                workspace.backup(self, "startup")

    def _commit(self):
        workspace.guard(self)
        self.db.commit()
        workspace.remember(self)

    def close(self):
        try:
            if workspace.health(self)["healthy"] and self.known_counts.get("tracks"):
                workspace.backup(self, "shutdown")
        finally:
            self.db.close()

    def setting(self, key, default=None, value=None):
        with self.lock:
            if value is not None:
                self.db.execute("INSERT OR REPLACE INTO settings VALUES(?,?)", (key, canonical(value)))
                self._commit()
                return value
            row = self.db.execute("SELECT value FROM settings WHERE key=?", (key,)).fetchone()
            return json.loads(row[0]) if row else default

    def get(self, track_id):
        with self.lock:
            row = self.db.execute("SELECT profile FROM tracks WHERE id=?", (track_id,)).fetchone()
            if not row:
                raise ValueError(f"Unknown track ID: {track_id}")
            return json.loads(row[0])

    def tracks(self):
        with self.lock:
            return [json.loads(r[0]) for r in self.db.execute("SELECT profile FROM tracks ORDER BY id")]

    def import_track(self, path, legacy=None):
        path = str(Path(path).resolve()) if Path(path).exists() else str(path)
        exists = Path(path).is_file()
        fingerprint = file_hash(path) if exists else "unverified:" + digest(path)
        with self.lock:
            row = self.db.execute("SELECT id FROM tracks WHERE fingerprint=?", (fingerprint,)).fetchone()
            if row:
                p = self.get(row[0])
                if path not in p["locations"]:
                    p["locations"].append(path)
                if exists and not Path(p["path"]).exists():
                    p["path"] = path
                if exists and p.get('analysis_state')=='missing':p['analysis_state']='imported' if p.get('analysis') else 'pending'
                if exists and not p.get('duration'):
                    try:
                        import mutagen
                        metadata=mutagen.File(path)
                        p['duration']=float(getattr(getattr(metadata,'info',None),'length',0) or 0)
                    except Exception:
                        pass
                    if not p.get('duration'):
                        try:
                            import soundfile as sf
                            p['duration']=sf.info(path).duration
                        except Exception:
                            pass
                self.save(p)
            else:
                old = self.db.execute("SELECT track_id FROM aliases WHERE path=?", (path,)).fetchone()
                track_id = old[0] if old else str(uuid.uuid4())
                prior = self.get(track_id) if old else None
                # A changed location must not replace the shared identity of
                # unchanged audio at its other aliases.
                if prior and len(prior.get("locations", [])) > 1:
                    prior["locations"] = [p for p in prior["locations"] if p != path]
                    if prior["path"] == path:
                        prior["path"] = next((p for p in prior["locations"] if Path(p).is_file()), prior["locations"][0])
                    self.save(prior)
                    track_id, prior = str(uuid.uuid4()), None
                p = {"schema_version": 1, "id": track_id, "fingerprint": fingerprint,
                     "path": path, "locations": [path], "title": Path(path).stem, "artist": "", "version": "",
                     "duration": 0, "bpm": None, "key": None, "genres": [], "notes": "", "analysis": None,
                     "revision": None, "cues": [], "overrides": prior.get("overrides", {}) if prior else {},
                     "analysis_state": "missing" if not exists else "pending", "review_state": "unreviewed",
                     "write_state": "unwritten", "created": utcnow()}
                if exists:
                    try:
                        import mutagen
                        tags = mutagen.File(path, easy=True)
                        if tags is not None and getattr(tags, 'info', None):
                            p['duration'] = float(getattr(tags.info, 'length', 0) or 0)
                        if tags:
                            for field in ("title", "artist"):
                                if tags.get(field): p[field] = ", ".join(tags[field])
                            p["genres"] = list(tags.get("genre", []))
                            p["metadata_source"] = "embedded-tags"
                    except Exception:
                        pass
                    if not p['duration']:
                        try:
                            import soundfile as sf
                            p['duration'] = sf.info(path).duration
                        except Exception:
                            pass
                if not p["artist"] and " - " in p["title"]:
                    p["artist"], p["title"] = p["title"].split(" - ", 1)
                if prior:
                    self.invalidate(track_id)
                    p["review_state"] = "stale"
                self.db.execute("INSERT OR REPLACE INTO tracks VALUES(?,?,?)", (track_id, fingerprint, canonical(p)))
            self.db.execute("INSERT OR REPLACE INTO aliases VALUES(?,?)", (path, p["id"]))
            self._commit()
        # Rekordbox import can supply grid metadata before any analysis exists.
        # That seed must not silently prevent importing historical observations.
        if legacy and not p.get("revision") and p.get("analysis_state") == "pending":
            self.apply_analysis(p["id"], legacy, source="legacy-import")
        return self.get(p["id"])

    def save(self, profile):
        with self.lock:
            self.db.execute("UPDATE tracks SET fingerprint=?,profile=? WHERE id=?", (profile["fingerprint"], canonical(profile), profile["id"]))
            self._commit()

    def import_profile(self, record):
        if record.get("schema_version") != 1:
            raise ValueError("Unsupported TrackProfile schema")
        candidates=[record["path"]]+record.get("locations",[])
        path=next((p for p in candidates if Path(p).is_file() and file_hash(p)==record.get("fingerprint")),None)
        if path is None and not any(Path(p).is_file() for p in candidates):
            with self.lock:
                existing=self.db.execute('SELECT fingerprint FROM tracks WHERE id=?',(record['id'],)).fetchone()
                if existing and existing[0]!=record['fingerprint']:raise ValueError('Imported track ID conflicts with local audio')
                if existing:return self.get(record['id'])
                pending=copy.deepcopy(record);pending.update(analysis_state='missing',review_state='stale',write_state='stale')
                for cue in pending.get('cues',[]):cue['review']='stale'
                self.db.execute('INSERT INTO tracks VALUES(?,?,?)',(pending['id'],pending['fingerprint'],canonical(pending)))
                self._commit()
                return pending
        profile = self.import_track(path or record["path"])
        if profile["fingerprint"] != record.get("fingerprint"):
            raise ValueError("Profile audio identity does not match; locate the exact file before importing analysis")
        if profile.get("analysis"):
            return profile
        original_id = record.get("id")
        if isinstance(original_id, str) and 0 < len(original_id) <= 128 and original_id != profile["id"]:
            with self.lock:
                existing = self.db.execute("SELECT fingerprint FROM tracks WHERE id=?", (original_id,)).fetchone()
                if existing:
                    raise ValueError("Imported track ID conflicts with a different local track")
                self.db.execute("UPDATE tracks SET id=? WHERE id=?", (original_id, profile["id"]))
                self.db.execute("UPDATE aliases SET track_id=? WHERE track_id=?", (original_id, profile["id"]))
                profile["id"] = original_id
                self.save(profile)
        if record.get("analysis"):
            source=record.get('analysis_source') or next((c.get('source') for c in record.get('cues',[]) if c.get('source')), 'profile-import')
            profile = self.apply_analysis(profile["id"], record["analysis"], source=source)
        changes = {k:v for k,v in record.get("overrides", {}).items() if k != "cue_positions"}
        if changes:
            profile = self.edit(profile["id"], changes)
        positions = record.get("overrides", {}).get("cue_positions", {})
        for cue in list(profile["cues"]):
            if cue["role"] in positions:
                position = positions[cue["role"]]
                profile = self.review_cue(profile["id"], cue["id"], "proposed", position["time"], position.get("end"))
        profile["import_provenance"] = {"original_id": record.get("id"), "original_revision": record.get("revision"), "imported": utcnow()}
        profile['analysis_state']='imported' if profile.get('analysis') else 'pending'
        self.save(profile)
        return profile

    def relocate(self,track_id,path):
        p=self.get(track_id);path=str(Path(path).resolve())
        if not Path(path).is_file() or file_hash(path)!=p['fingerprint']:
            raise ValueError('Choose the exact audio file with the recorded checksum; filenames alone are insufficient')
        p['path']=path
        if path not in p['locations']:p['locations'].append(path)
        if p.get('analysis'):p['analysis_state']=p['analysis'].get('state','imported')
        with self.lock:
            self.db.execute('INSERT OR REPLACE INTO aliases VALUES(?,?)',(path,track_id));self._commit()
        self.save(p);return p

    def apply_analysis(self, track_id, analysis, source="native-v1"):
        p = self.get(track_id)
        from .metrics import semantic_observation
        revision = digest({"audio": p["fingerprint"], "analysis": semantic_observation(analysis), "source": source})
        if revision == p.get("revision"):
            return p
        self.invalidate(track_id)
        p.update(analysis=analysis, revision=revision, duration=analysis.get("duration", p["duration"]),
                 bpm=analysis.get("bpm", p.get("bpm")), key=analysis.get("key", p.get("key")), analysis_state=analysis.get("state", "complete"),
                 review_state="stale" if p.get("revision") else "unreviewed", write_state="stale")
        p['analysis_source']=source
        if source == "legacy-import":
            p["analysis_state"] = "imported"
        observations=[o for o in p.get("observations",[]) if o.get("source") not in {"energy-baseline", "historical-resolved"}]
        for event,bar in analysis.get("sections",{}).items():
            observations.append({"event":event,"bar":bar,"state":"estimated" if bar is not None else "unknown",
                                 "source":"energy-baseline" if source != "legacy-import" else "historical-resolved",
                                 "family":"energy" if source != "legacy-import" else "historical","confidence":"unverified"})
        p["observations"]=observations
        p["cues"] = [{"id": digest([track_id, revision, c[0]])[:24], "role": c[0], "name": c[1],
                       "time": c[2], "end": c[3] if len(c) > 3 else None, "review": "proposed",
                       "revision": revision, "confidence": "unverified", "source": source}
                      for c in analysis.get("cues", [])]
        for key, value in p.get("overrides", {}).items():
            if key not in {"grid", "sections", "cue_positions"}:
                p[key] = value
        for c in p["cues"]:
            saved = p.get("overrides", {}).get("cue_positions", {}).get(c["role"])
            if saved and 0 <= saved["time"] < p["duration"]:
                c.update(time=saved["time"], end=saved.get("end"), manual=True, review="stale")
        with self.lock:
            self.db.execute("INSERT OR IGNORE INTO revisions VALUES(?,?,?)", (revision, track_id, canonical(p)))
            self._commit()
        self.save(p)
        return p

    def edit(self, track_id, changes):
        p = self.get(track_id)
        before = copy.deepcopy(p)
        allowed = {"title", "artist", "version", "notes", "key", "bpm", "genres", "energy", "sections", "grid", "themes", "hooks", "song_group", "mood_tags", "familiarity", "accept_rekordbox_grid_conflict"}
        if set(changes) - allowed:
            raise ValueError("Unsupported track edit")
        if "bpm" in changes and not 20 <= float(changes["bpm"]) <= 400:
            raise ValueError("BPM must be between 20 and 400")
        if "energy" in changes and not 1 <= float(changes["energy"]) <= 10:
            raise ValueError("Energy must be between 1 and 10")
        if changes.get("key"):
            from .tonal import camelot
            if camelot(changes["key"]) is None:
                raise ValueError("Key must be standard notation (for example A minor or F#m) or Camelot (for example 8A)")
        if "familiarity" in changes and not 0 <= float(changes["familiarity"]) <= 1:
            raise ValueError("Familiarity must be from 0 to 1")
        if "grid" in changes:
            from .models import validate_grid
            changes["grid"] = validate_grid(changes["grid"], p["duration"])
        if "sections" in changes:
            import math
            end = 0.
            for section in changes["sections"]:
                start, stop = float(section["start"]), float(section["end"])
                if not all(math.isfinite(v) for v in (start, stop)) or start < end - .001 or not 0 <= start < stop <= p["duration"] + .001:
                    raise ValueError("Sections must be ordered, non-overlapping, and within audio")
                if not isinstance(section.get("label"), str) or not section["label"]:
                    raise ValueError("A section needs a label")
                end = stop
        if "hooks" in changes:
            for hook in changes["hooks"]:
                if not 0 <= float(hook["time"]) < p["duration"]:
                    raise ValueError("Hook outside audio")
        p["overrides"].update(changes)
        for key, value in changes.items():
            if key not in {"grid", "sections"}:
                p[key] = value
        p["review_state"] = "reviewing"
        if set(changes) & {"grid", "sections"}:
            for c in p["cues"]:
                c["review"] = "stale"
            p["write_state"] = "stale"
        self.record_edit(before, p)
        return p

    def review_cue(self, track_id, cue_id, status, position=None, end=None):
        if status not in {"approved", "rejected", "proposed", "delete-approved"}:
            raise ValueError("Invalid review status")
        p = self.get(track_id)
        before = copy.deepcopy(p)
        cue = next((c for c in p["cues"] if c["id"] == cue_id), None)
        if not cue:
            raise ValueError("Unknown cue")
        if position is not None:
            position = float(position)
            if not 0 <= position < p["duration"]:
                raise ValueError("Cue outside audio")
            cue["time"] = position
        if end is not None:
            if not cue["time"] < float(end) <= p["duration"]:
                raise ValueError("Loop end outside audio")
            cue["end"] = float(end)
        if cue.get("end") is not None and cue["end"] <= cue["time"]:
            raise ValueError("Loop end must follow cue")
        cue["review"] = status
        cue["manual"] = position is not None or end is not None or cue.get("manual", False)
        if cue["manual"]:
            p["overrides"].setdefault("cue_positions", {})[cue["role"]] = {"time": cue["time"], "end": cue["end"]}
        p["review_state"] = "reviewed" if all(c["review"] in {"approved", "rejected", "delete-approved"} for c in p["cues"]) else "reviewing"
        p["write_state"] = "unwritten"
        self.record_edit(before, p)
        return p

    def review_bulk(self, track_id, cue_ids, status, revision=None):
        if status not in {"approved", "rejected", "proposed"}:
            raise ValueError("Bulk decisions support approve, reject or reset")
        p = self.get(track_id)
        if revision is not None and revision != p["revision"]:
            raise ValueError("Analysis changed; review the current cues")
        if not cue_ids or set(cue_ids) - {c["id"] for c in p["cues"]}:
            raise ValueError("Select valid cues before applying a group decision")
        before = copy.deepcopy(p)
        for cue in p["cues"]:
            if cue["id"] in cue_ids: cue["review"] = status
        p["review_state"] = "reviewed" if all(c["review"] in {"approved", "rejected", "delete-approved"} for c in p["cues"]) else "reviewing"
        p["write_state"] = "unwritten"
        self.record_edit(before, p)
        return p

    def record_edit(self, before, after):
        with self.lock:
            self.db.execute("INSERT INTO edits(track_id,before_value,after_value,created) VALUES(?,?,?,?)",
                            (after["id"], canonical(before), canonical(after), utcnow()))
            self._commit()
        self.invalidate(after["id"])
        self.save(after)

    def undo(self, track_id):
        with self.lock:
            row = self.db.execute("SELECT id,before_value FROM edits WHERE track_id=? AND undone=0 ORDER BY id DESC LIMIT 1", (track_id,)).fetchone()
            if not row:
                raise ValueError("Nothing to undo")
            restored = json.loads(row[1])
            if restored.get("revision") != self.get(track_id).get("revision"):
                raise ValueError("Cannot undo across an analysis revision")
            self.db.execute("UPDATE edits SET undone=1 WHERE id=?", (row[0],))
            self._commit()
        self.invalidate(track_id)
        self.save(restored)
        return restored

    def artifact(self, kind, artifact_id, record=None):
        with self.lock:
            if record is not None:
                self.db.execute("INSERT OR REPLACE INTO artifacts VALUES(?,?,?)", (kind, artifact_id, canonical(record)))
                self._commit()
                return record
            row = self.db.execute("SELECT record FROM artifacts WHERE kind=? AND id=?", (kind, artifact_id)).fetchone()
            return json.loads(row[0]) if row else None

    def artifacts(self, kind):
        with self.lock:
            return [json.loads(r[0]) for r in self.db.execute("SELECT record FROM artifacts WHERE kind=?", (kind,))]

    def artifact_batch(self, kind, records):
        """Persist a validated batch in one transaction, including replacement rows."""
        rows = [(kind, key, canonical(record)) for key, record in records]
        with self.lock:
            try:
                self.db.executemany("INSERT OR REPLACE INTO artifacts VALUES(?,?,?)", rows)
                self._commit()
            except Exception:
                self.db.rollback()
                raise

    def invalidate(self, track_id):
        with self.lock:
            for kind in ("transition", "set", "write-preview"):
                for a in self.artifacts(kind):
                    if track_id in a.get("track_ids", []):
                        a["stale"] = True
                        self.artifact(kind, a["id"], a)

    def job(self, record):
        with self.lock:
            record["updated"] = utcnow()
            self.db.execute("INSERT OR REPLACE INTO jobs VALUES(?,?)", (record["id"], canonical(record)))
            self._commit()
        return record

    def jobs(self):
        with self.lock:
            return [json.loads(r[0]) for r in self.db.execute("SELECT record FROM jobs ORDER BY rowid DESC")]
