from __future__ import annotations
import concurrent.futures
import csv
import errno
import html
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import threading
import time
import traceback
import uuid
from datetime import datetime, timezone
from .store import Store
from . import workspace
from .util import atomic_json, utcnow, basename, digest, check_version
from .catalogue import CATALOGUE
from .models import SetBrief


class Cancelled(Exception):
    pass


# A running job that has not checkpointed for this long is stalled. Checkpoints come at least
# every stage, and the longest stage (a cold import of the audio tools, or decoding a track)
# takes well under a minute.
STALL_S = 300
STALL_LOG = "service-stalls.log"


def job_age(job, now):
    """Seconds since the job record was last saved; None when unknown."""
    try:
        updated = datetime.fromisoformat(str(job.get("updated")).replace("Z", "+00:00"))
    except (TypeError, ValueError):
        return None
    if updated.tzinfo is None:
        updated = updated.replace(tzinfo=timezone.utc)
    return (now - updated).total_seconds()


def stalled_jobs(jobs, now, limit=STALL_S):
    """Running jobs whose record has not changed for `limit` seconds."""
    return [j for j in jobs if j.get("state") == "running" and (job_age(j, now) or 0) > limit]


# Everything a job or an RPC call imports lazily from the numeric and audio stack. Imported
# once, on the thread that constructs the service, before it answers anything: later imports
# are cache hits, so no two threads import these packages at the same time (see WARM_UP).
WARM_MODULES = (
    "numpy", "scipy.signal", "scipy.optimize", "scipy.stats", "scipy.interpolate", "scipy.fft",
    "scipy.ndimage", "scipy.linalg", "scipy.sparse", "soundfile", "librosa", "librosa.feature",
    "librosa.onset", "librosa.beat", "librosa.effects", "librosa.decompose", "librosa.segment",
    "onset_service.tonal", "onset_service.cues", "onset_service.structure",
    "onset_service.transitions", "onset_service.planning", "onset_service.evidence",
    "onset_service.mixplan", "onset_service.analysis", "onset_service.preview",
    "onset_service.genre",
)


# Requests that read the database only and import nothing from the numeric stack: the
# screens make these at startup, and they answer while the warm-up runs. Everything else
# waits for `imports_ready` (a cold numba cache can take minutes to compile into).
LIGHT_METHODS = frozenset({
    "tracks", "jobs", "job-control", "settings", "catalogue", "review-queue",
    "workspace-health", "workspace-backups", "workspace-restore-preview",
})


def warm_imports():
    """Imports WARM_MODULES and touches every name librosa attaches lazily (except `display`,
    which needs matplotlib), so no later attribute access imports a submodule from a job
    thread. A package that fails to import is reported on stderr and fails again, with its
    own message, where it is first used. Returns the seconds taken."""
    import importlib
    started = time.monotonic()
    for name in WARM_MODULES:
        try:
            importlib.import_module(name)
        except Exception as exc:  # noqa: BLE001 - the real failure surfaces where it is used
            print(f"warm-up: {name}: {exc}", file=sys.stderr)
    librosa = sys.modules.get("librosa")
    for name in getattr(librosa, "__all__", ()):
        if name != "display":
            try:
                getattr(librosa, name)
            except Exception as exc:  # noqa: BLE001
                print(f"warm-up: librosa.{name}: {exc}", file=sys.stderr)
    return time.monotonic() - started


def thread_stacks():
    """Every thread's stack, as text: what a stalled job is waiting on."""
    names = {t.ident: t.name for t in threading.enumerate()}
    parts = []
    for ident, frame in sys._current_frames().items():
        parts.append(f"--- thread {names.get(ident, ident)}\n" + "".join(traceback.format_stack(frame)))
    return "\n".join(parts)


class StorageFull(Exception):
    pass


def storage_full(exc):
    return isinstance(exc, OSError) and exc.errno in {errno.ENOSPC, getattr(errno, "EDQUOT", errno.ENOSPC)}


class Service:
    def __init__(self, home=None, notify=lambda event: None, warm=True):
        self.store = Store(home)
        self.notify = notify
        self.pool = concurrent.futures.ThreadPoolExecutor(max_workers=int(self.store.setting("light_jobs", 2)))
        self.heavy_pool = concurrent.futures.ThreadPoolExecutor(max_workers=int(self.store.setting("heavy_jobs", 1)))
        self.heavy = threading.Semaphore(1)
        self.controls = {}
        self.shutting_down = threading.Event()
        # Show mode (plan O4): while visuals run, jobs wait at their next checkpoint and nothing
        # writes to rekordbox. `show_paused` holds the jobs show mode paused, to resume after.
        self.show = False
        self.show_paused = set()
        for job in self.store.jobs() if workspace.health(self.store)["healthy"] else []:
            if job["state"] in {"running", "queued", "cancelling"}:
                job.update(state="paused", error="Interrupted at application shutdown; resume to continue")
                self.store.job(job)
        # WARM_UP: a job stuck at "loading audio tools" with no CPU, twice, was the job thread's
        # `import librosa` deadlocked against the RPC thread importing numpy for a UI refresh.
        # One thread loads it all first; requests and jobs that could import any of it wait
        # (`dispatch`, `run_job`), while LIGHT_METHODS answer at once, so the library shows
        # straight away instead of a blank screen while numba compiles into a cold cache.
        self.imports_ready = threading.Event()
        if warm:
            threading.Thread(target=self._warm_up, name="warm-up", daemon=True).start()
        else:
            self.imports_ready.set()
        # Should a job still stall, its threads' stacks go to a log in the workspace.
        self._stalls_logged = set()
        threading.Thread(target=self._watch_stalls, name="stall-watchdog", daemon=True).start()

    def _warm_up(self):
        self.notify({"method": "service-startup", "params": {"stage": "loading audio tools"}})
        seconds = None
        try:
            seconds = warm_imports()
        finally:
            self.imports_ready.set()
        print(f"audio tools loaded in {seconds or 0:.1f} s", file=sys.stderr)
        self.notify({"method": "service-startup", "params": {"stage": "ready", "seconds": round(seconds or 0, 1)}})

    def _watch_stalls(self):
        while not self.shutting_down.wait(30):
            try:
                self.log_stalls()
            except Exception:
                pass

    def log_stalls(self, now=None):
        """Appends the stacks of every thread to the stall log, once per stalled checkpoint."""
        now = now or datetime.now(timezone.utc)
        logged = []
        for job in stalled_jobs(self.store.jobs(), now):
            key = (job["id"], job.get("updated"))
            if key in self._stalls_logged:
                continue
            self._stalls_logged.add(key)
            with open(Path(self.store.home) / STALL_LOG, "a", encoding="utf-8") as log:
                log.write(f"=== {utcnow()} job {job['id']} ({job.get('kind')}) stalled at "
                          f"{job.get('stage')!r} {job.get('progress')} since {job.get('updated')}\n")
                log.write(thread_stacks() + "\n\n")
            logged.append(job["id"])
        return logged

    def close(self):
        self.shutting_down.set()
        try:
            if workspace.health(self.store)["healthy"]:
                for job in self.store.jobs():
                    if job["state"] in {"running","queued","cancelling"}:
                        job.update(state="cancelled" if job["state"]=="cancelling" else "paused",
                                   error="Stopped at application shutdown; resume to continue")
                        self.store.job(job)
                if self.store.known_counts.get("tracks"):
                    workspace.backup(self.store,"quit-checkpoint")
        finally:
            for control in self.controls.values():
                control["cancel"].set()
                control["pause"].set()
            self.pool.shutdown(wait=True, cancel_futures=True)
            self.heavy_pool.shutdown(wait=True, cancel_futures=True)
            self.store.close()

    def dispatch(self, method, p=None):
        p = p or {}
        if method not in LIGHT_METHODS:
            self.imports_ready.wait()
        if method not in {"doctor", "workspace-health", "workspace-backups", "workspace-restore-preview", "workspace-restore"}:
            workspace.guard(self.store)
        methods = {
            "workspace-health": lambda: workspace.health(self.store),
            "workspace-backups": lambda: workspace.backups(self.store),
            "workspace-backup": lambda: workspace.backup(self.store),
            "workspace-restore-preview": lambda: self.workspace_restore_preview(p),
            "workspace-restore": lambda: self.workspace_restore(p),
            "doctor": lambda: self.doctor(), "tracks": self.store.tracks,
            "track": lambda: self.store.get(p["id"]), "import": lambda: self.import_files(p["paths"]),
            "relocate":lambda:self.store.relocate(p["id"],p["path"]),
            "analyze": lambda: self.enqueue("analyze", p), "edit": lambda: self.store.edit(p["id"], p["changes"]),
            "review": lambda: self.store.review_cue(p["id"], p["cue_id"], p["status"], p.get("position"), p.get("end")),
            "review-bulk": lambda: self.store.review_bulk(p["id"], p["cue_ids"], p["status"], p.get("revision")),
            "undo": lambda: self.store.undo(p["id"]), "jobs": self.store.jobs,
            "job-control": lambda: self.job_control(p["id"], p["action"]),
            "show-mode": lambda: self.show_mode(bool(p.get("on"))),
            "catalogue": lambda: CATALOGUE,
            "transitions": lambda: self.transitions(p), "plan": lambda: self.plan(p),
            "plan-job": lambda: self.enqueue("plan", {**p, "deep": False}),
            "transitions-job": lambda: self.enqueue("transitions", {**p, "deep": False}),
            "replan": lambda: self.replan(p),
            "review-queue": lambda: self.review_queue(p),
            "genre-suggestions": lambda: self.genre_suggestions(p),
            "import-workspace": lambda: self.import_workspace(p),
            "relocate-cache": lambda: self.relocate_cache(p),
            "sets": lambda: self.saved_sets(),
            "transition": lambda: self.store.artifact("transition",p["id"]),
            "validate-plan": lambda: self.validate_plan(p["plan"]),
            "import-plan": lambda: self.import_plan(p["plan"]),
            "import-plan-file": lambda: self.import_plan(json.loads(Path(p["path"]).read_text(encoding="utf-8-sig"))),
            "preview": lambda: self.preview(p), "export": lambda: self.export(p),
            "calibrate-energy": lambda: self.calibrate_energy(p),
            "energy-judgment": lambda: self.energy_judgment(p),
            "benchmark": lambda: self.benchmark(p), "evidence": lambda: self.evidence(p),
            "benchmark-label": lambda: self.benchmark_label(p),
            "reliability": lambda: self.reliability(p), "replan-order": lambda: self.replan_order(p),
            "transition-alternatives":lambda:self.transition_alternatives(p),
            "ai-context": lambda: self.ai_context(p), "enrichment-import": lambda: self.enrichment(p),
            "spectrogram-image": lambda: self.spectrogram_image(p),
            "observations-import": lambda: self.observations_import(p),
            "rekordbox-preview": lambda: self.rb_preview(p), "rekordbox-apply": lambda: self.rb_apply(p),
            "restore": lambda: self.restore(p), "rekordbox-import": lambda: self.rb_import(p),
            "manifests": lambda: self.store.artifacts("write-manifest"),
            "settings": lambda: self.settings(p), "worker-setup": lambda: self.enqueue("worker-setup", p),
        }
        if method not in methods:
            raise ValueError(f"Unknown method: {method}")
        return methods[method]()

    def workspace_restore_preview(self, p):
        if any(control.get("active", False) for control in self.controls.values()):
            raise ValueError("Quit and reopen OnSet before restoring; an analysis worker still owns jobs")
        return workspace.preview(self.store, p["id"])

    def workspace_restore(self, p):
        if p.get("approved") is not True:
            raise ValueError("Approve the exact workspace restoration preview first")
        self.workspace_restore_preview(p)
        return workspace.restore(self.store, p["id"], p["token"])

    def saved_sets(self):
        values=self.store.artifacts('set')[-12:][::-1]
        tracks=self.store.tracks()
        for plan in values:
            try:self.validate_plan(plan,tracks)
            except (ValueError,KeyError) as error:
                plan.update(stale=True,validation_error=str(error))
        return values

    def settings(self, p):
        allowed = {"cache_dir", "model_dir", "light_jobs", "heavy_jobs", "wsl_distro", "rekordbox_db", "experimental_models", "set_brief"}
        if p.get("key") not in allowed:
            if p:
                raise ValueError("Unknown setting")
            return {k: self.store.setting(k) for k in allowed}
        value = p.get("value")
        if p["key"]=="set_brief" and value is not None:
            if not isinstance(value,dict):raise ValueError("Set brief must be an object")
            from .models import SetBrief
            value=SetBrief.parse(value).json()
        if p["key"]=="experimental_models" and (not isinstance(value,list) or set(value)-{"edmformer","allin1"}):raise ValueError("Unknown experimental model")
        if p["key"] in {"light_jobs", "heavy_jobs"} and value is not None:
            if not isinstance(value, int) or not 1 <= value <= (2 if p["key"] == "heavy_jobs" else 4):
                raise ValueError("Resource limit is outside the supported range")
        return self.store.setting(p["key"], value=value)

    def import_files(self, paths):
        imported, failed = [], []
        audio = {".flac", ".wav", ".mp3", ".m4a", ".aiff", ".aif", ".ogg", ".opus", ".aac"}
        for raw in paths:
            path = Path(raw)
            try:
                if not path.exists(): raise ValueError("Local path does not exist")
                if path.suffix.lower() == ".json":
                    value = json.loads(path.read_text(encoding="utf-8-sig"))
                    if isinstance(value, list):
                        from onset_service.cues import norm_cues
                        for r in value:
                            r = dict(r)
                            r["cues"] = norm_cues(r.get("cues", []))
                            r["source_provenance"] = {"kind": "historical-import", "file": str(path), "imported": utcnow()}
                            imported.append(self.store.import_track(r["path"], r))
                    elif isinstance(value,dict) and "tracks" in value:
                        check_version(value)
                        if value.get('kind')=='workspace':
                            # Validate every nested record before writing anything, so a malformed file cannot half-import.
                            for section,required in (('transitions','id'),('plans','id'),('benchmark_labels','track_id')):
                                for item in value.get(section,[]):
                                    if not isinstance(item,dict) or not item.get(required):
                                        raise ValueError(f"Workspace {section} entry is missing {required}")
                                    if item.get('schema_version',1)!=1:
                                        raise ValueError(f"Unsupported schema version in workspace {section}; expected 1")
                        for r in value["tracks"]:
                            imported.append(self.store.import_profile(r))
                        if value.get('kind')=='workspace':
                            from .transitions import validate_option
                            lookup={t['id']:t for t in self.store.tracks()}
                            for option in value.get('transitions',[]):
                                try:validate_option(option,lookup)
                                except (ValueError,KeyError):option={**option,'stale':True}
                                self.store.artifact('transition',option['id'],option)
                            for plan in value.get('plans',[]):
                                try:self.validate_plan(plan)
                                except (ValueError,KeyError):plan={**plan,'stale':True}
                                self.store.artifact('set',plan['id'],plan)
                            for label in value.get('benchmark_labels',[]):
                                if label['track_id'] in lookup:self.store.artifact('benchmark-label',label['track_id'],label)
                    elif isinstance(value, dict) and any(isinstance(v, dict) and ("drop" in v or "error" in v) for v in value.values()):
                        self.import_opinions(value, str(path))
                    else:
                        raise ValueError("Expected legacy analysis list or versioned profile export")
                else:
                    if path.is_file() and path.suffix.lower() not in audio:
                        raise ValueError("Unsupported file type; import audio, a legacy analysis list or a versioned profile/workspace JSON")
                    files = sorted(path.rglob("*")) if path.is_dir() else [path]
                    for f in files:
                        if f.is_file() and f.suffix.lower() in audio:
                            p = self.store.import_track(f)
                            if not p.get("duration"):
                                try:
                                    import soundfile as sf
                                    p["duration"] = sf.info(f).duration
                                    self.store.save(p)
                                except Exception:
                                    pass
                            imported.append(p)
            except Exception as exc:
                failed.append({"path": str(path), "error": str(exc)})
        return {"imported": len(imported), "failed": failed, "track_ids": list(dict.fromkeys(p["id"] for p in imported))}

    def enqueue(self, kind, params, record=None):
        if kind=="analyze" and params.get("deep",True) and "experimental" not in params:
            params={**params,"experimental":self.store.setting("experimental_models",[])}
        if record is None:
            existing=next((j for j in self.store.jobs() if j["kind"]==kind and j["state"] in {"queued","running"} and digest(j["params"])==digest(params)),None)
            if existing: return existing
        record = record or {"id": uuid.uuid4().hex, "kind": kind, "params": params, "created": utcnow(), "completed": []}
        record.update(state="queued", stage="queued", progress=0, error=None)
        control = {"cancel": threading.Event(), "pause": threading.Event(), "active": True, "started": False, "requested_cancel": False}
        control["pause"].set()
        if self.show:
            # Queued during a show: it starts once the show is over.
            control["pause"].clear()
            record.update(state="paused", stage="waiting for the show to end")
            self.show_paused.add(record["id"])
        self.controls[record["id"]] = control
        self.store.job(record)
        pool = self.heavy_pool if kind == "worker-setup" or kind == "analyze" and params.get("deep", True) else self.pool
        pool.submit(self.run_job, record, control)
        return record

    def run_job(self, record, control):
        def save():
            # A pause requested between checkpoints must not be overwritten by this thread's running copy.
            if not control["pause"].is_set() and record["state"] in {"running", "queued"}:
                record["state"] = "paused"
            self.store.job(record)
        def checkpoint(stage, progress):
            if control["cancel"].is_set():
                raise Cancelled()
            while not control["pause"].wait(.2):
                if control["cancel"].is_set():
                    raise Cancelled()
            record.update(state="running", stage=stage, progress=progress)
            self.store.job(record)
            self.notify({"method": "job-progress", "params": record.copy()})
        try:
            control["started"] = True
            self.imports_ready.wait()
            checkpoint("loading native analysis", 0)
            if record["kind"] == "analyze":
                from .analysis import analyse
                ids = record["params"].get("track_ids") or [p["id"] for p in self.store.tracks()]
                failures = []
                partial = []
                for index, tid in enumerate(ids):
                    if tid in record["completed"]:
                        continue
                    try:
                        result = analyse(self.store.get(tid), self.store.home, record["params"].get("deep", True),
                                         lambda s, v: checkpoint(s, (index + v) / max(1, len(ids))), record["params"].get("experimental", []),
                                         {k:self.store.setting(k) for k in ("cache_dir", "model_dir")})
                        self.store.apply_analysis(tid, result)
                        if result.get("state") != "complete":
                            partial.append(tid)
                        record["completed"].append(tid)
                        save()
                    except Cancelled:
                        raise
                    except Exception as exc:
                        if storage_full(exc):
                            raise StorageFull(f"Disk or cache storage is full while analysing {tid}; free space or change the cache location, then resume") from exc
                        failures.append({"track_id": tid, "error": str(exc)})
                record["failures"] = failures
                record["partial_tracks"] = partial
                if failures or partial:
                    record["state"] = "partial" if record["completed"] else "failed"
                else:
                    record["state"] = "complete"
            elif record["kind"] in {"plan", "transitions"}:
                stage = "searching sets" if record["kind"] == "plan" else "scoring transitions"
                last = [0.]
                def stop(progress=None):
                    # Called many times per second: check cancellation cheaply, persist progress at most 4x/s.
                    if control["cancel"].is_set():
                        raise Cancelled()
                    if progress is not None:
                        record["progress"] = max(record.get("progress", 0), progress)
                    if time.monotonic() - last[0] >= .25 or not control["pause"].is_set():
                        last[0] = time.monotonic()
                        checkpoint(stage, record.get("progress", 0))
                params = {k: v for k, v in record["params"].items() if k != "deep"}
                if record["kind"] == "plan":
                    result = self.plan(params, cancelled=stop)
                    record["result"] = {"plan_ids": [plan["id"] for plan in result["plans"]], "conflicts": result["conflicts"],
                                        "transitions": len(result["transitions"])}
                else:
                    result = self.transitions(params, cancelled=stop)
                    record["result"] = {"transition_ids": [o["id"] for o in result[:50]], "count": len(result)}
                record["state"] = "complete"
            elif record["kind"] == "worker-setup":
                from .deep import wsl_path, worker_file
                from .processes import run_process
                model=record["params"].get("model")
                if model not in (None,"edmformer","allin1"):raise ValueError("Unsupported experimental setup")
                script = worker_file("setup_"+model+".sh" if model else "setup.sh")
                binary = Path(self.store.home) / "worker-bootstrap" / "uv"
                run_process([sys.executable, str(script.with_name("bootstrap.py")), str(binary.parent)], checkpoint,
                            "Downloading verified worker bootstrap", 180)
                run_process(["wsl.exe", "--exec", "setsid", "--wait", "bash", wsl_path(script), wsl_path(binary)], checkpoint,
                            "Installing locked WSL runtime", 3600, wsl_group=True)
                record["state"] = "complete"
            record.update(progress=1, stage="finished")
        except Cancelled:
            if self.shutting_down.is_set() and not control.get("requested_cancel"):
                record.update(state="paused", error="Stopped at application shutdown; resume to continue")
            else:
                record.update(state="cancelled", stage="cancelled")
        except StorageFull as exc:
            record.update(state="paused", stage="storage full", error=str(exc))
        except Exception as exc:
            record.update(state="failed", error=str(exc))
        finally:
            control["active"] = False
            self.store.job(record)
            self.notify({"method": "job-progress", "params": record.copy()})

    def show_mode(self, on):
        """Starts or ends show mode and returns which jobs it paused or resumed."""
        changed = []
        if on and not self.show:
            self.show = True
            for record in self.store.jobs():
                control = self.controls.get(record["id"])
                if control and control["active"] and control["pause"].is_set()                         and record["state"] in {"running", "queued"}:
                    control["pause"].clear()
                    record["state"] = "paused"
                    self.store.job(record)
                    self.show_paused.add(record["id"])
                    changed.append(record["id"])
        elif not on and self.show:
            self.show = False
            for job_id in sorted(self.show_paused):
                control = self.controls.get(job_id)
                record = next((j for j in self.store.jobs() if j["id"] == job_id), None)
                if control and control["active"] and record and record["state"] == "paused":
                    control["pause"].set()
                    record["state"] = "running" if control.get("started") else "queued"
                    self.store.job(record)
                    changed.append(job_id)
            self.show_paused.clear()
        return {"show": self.show, "jobs": changed}

    def job_control(self, job_id, action):
        record = next((j for j in self.store.jobs() if j["id"] == job_id), None)
        if not record:
            raise ValueError("Unknown job")
        if self.show and action == "resume":
            raise ValueError("Jobs resume when the show ends")
        # A job the DJ pauses or cancels during the show stays that way afterwards.
        self.show_paused.discard(job_id)
        control = self.controls.get(job_id)
        if action == "resume" and (not control or not control["active"] or record["state"] in {"failed", "partial", "cancelled"}):
            record["completed"] = [tid for tid in record["completed"] if tid not in record.get("partial_tracks", [])]
            return self.enqueue(record["kind"], record["params"], record)
        if record["state"] == "complete":
            raise ValueError("This job has already completed")
        if not control or not control["active"]:
            if action=="cancel" and record["state"]=="paused":
                record.update(state="cancelled",stage="cancelled",error=None)
                return self.store.job(record)
            raise ValueError("Job is not active")
        if action == "pause":
            control["pause"].clear()
            record["state"] = "paused"
        elif action == "resume":
            control["pause"].set()
            record["state"] = "running" if control.get("started") else "queued"
        elif action == "cancel":
            control["requested_cancel"] = True
            control["cancel"].set()
            control["pause"].set()
            record["state"] = "cancelling"
        else:
            raise ValueError("Invalid job action")
        return self.store.job(record)

    def transitions(self, p, cancelled=lambda progress=None: None):
        from .transitions import options
        a = self.store.get(p["from"])
        targets = [self.store.get(p["to"])] if p.get("to") else [t for t in self.store.tracks() if t["id"] != a["id"]]
        result = []
        for index, b in enumerate(targets):
            cancelled(index / max(1, len(targets)))
            result.extend(options(a, b, p.get("brief", {})))
        self.store.artifact_batch("transition", [(o['id'],o) for o in result])
        return sorted(result, key=lambda x: (-x["score"], x["id"]))

    def known_transitions(self, brief, extra=()):
        ids = {pin.get("transition_id") for pin in (brief or {}).get("pinned", []) if pin.get("transition_id")} | set(extra)
        return {i: o for i in ids if (o := self.store.artifact("transition", i))}

    def plan(self, p, cancelled=lambda progress=None: None):
        from .planning import build_plans
        brief = p.get("brief", p)
        result = build_plans(self.store.tracks(), brief, self.known_transitions(brief), cancelled=cancelled)
        self.store.artifact_batch("transition", [(o['id'],o) for o in result['transitions']])
        self.store.artifact_batch("set", [(plan['id'],plan) for plan in result['plans']])
        return result

    def validate_plan(self, plan, tracks=None):
        """Validation needs only the transitions the plan names. Loading every stored
        transition (tens of thousands after a few set searches) took close to a minute."""
        from .planning import validate_plan
        transition_map = {}
        for link in plan.get("transition_ids") or []:
            option = self.store.artifact("transition", link) if isinstance(link, str) else None
            if option is not None:
                transition_map[link] = option
        return validate_plan(plan, tracks if tracks is not None else self.store.tracks(), transition_map, plan.get("brief"))

    def import_plan(self, plan):
        check_version(plan)
        if not plan.get("revisions"):
            plan = {**plan,"revisions":{i:self.store.get(i).get("revision") for i in plan.get("track_ids",[])}}
        result = self.validate_plan(plan)
        plan = {**plan, "duration_seconds": result["duration_seconds"], "stale": False}
        plan.pop('id',None)
        plan["id"] = digest(plan)[:24]
        return self.store.artifact("set", plan["id"], plan)

    def preview(self, p):
        from .preview import render
        option = self.store.artifact("transition", p["id"])
        if not option:
            raise ValueError("Unknown transition")
        return render(option, self.store.tracks(), self.store.home, p.get("click", False))

    def ai_context(self, p):
        ids = p.get("track_ids") or [t["id"] for t in self.store.tracks()]
        profiles = []
        for tid in ids:
            t = self.store.get(tid)
            a = t.get("analysis") or {}
            profiles.append({k: t.get(k) for k in ("id", "title", "artist", "version", "duration", "bpm", "key", "genres", "notes", "revision", "energy")})
            energy=a.get("energy_profile",{})
            compact_energy={k:energy.get(k) for k in ("rating","confidence","calibration","note","acoustic_summary")}
            for dimension in ("relative_curve","drive","density","intensity","tension","contrast"):
                curve=energy.get(dimension,[])
                stride=max(1,len(curve)//16)
                compact_energy[dimension]=curve[::stride][:16]
            profiles[-1].update(sections=a.get("structure", {}).get("sections", []), events=a.get("events", []),
                                energy_profile=compact_energy, local_keys=a.get("local_keys", []),
                                evidence=self.store.artifact("enrichment", tid), flags=a.get("flags", []))
            profiles[-1]['experimental_sources']={name:{'state':value.get('state'),'confidence':'unverified',
                'segments':value.get('data',{}).get('segments',[]),'error':value.get('error')}
                for name,value in a.get('deep',{}).get('models',{}).items() if name in {'edmformer','allin1'}}
        routes=[]
        route_options=[]
        if p.get("brief"):
            # Generate valid choices first; the model selects IDs rather than inventing timings.
            from .transitions import options as pair_options
            full = [self.store.get(tid) for tid in ids]
            generated={}
            for a in full:
                nearby = sorted((b for b in full if b["id"] != a["id"]), key=lambda b: abs((b.get("bpm") or 0) - (a.get("bpm") or 0)))[:24]
                for b in nearby:
                    for option in pair_options(a, b, p["brief"]):
                        generated[option['id']]=option
            # A deck's played rate carries into the next transition. Supply
            # complete validated routes as well as independent pair choices,
            # so a model can select a feasible chain at differing tempos.
            from .planning import build_plans
            seed=build_plans(full,{**p['brief'],'track_ids':ids})
            for option in seed['transitions']:generated[option['id']]=option
            routes=[{k:plan.get(k) for k in ('track_ids','transition_ids','duration_seconds','explanation')} for plan in seed['plans']]
            route_ids={link for route in routes for link in route['transition_ids']}
            route_options=[generated[link] for link in sorted(route_ids)]
            self.store.artifact_batch('transition',list(generated.items()))
        options = [t for t in self.store.artifacts("transition") if set(t["track_ids"]).issubset(ids) and not t.get("stale")]
        compact=[]
        for tid in ids:
            candidates=sorted([o for o in options if o["track_ids"][0]==tid],key=lambda o:-o["score"])
            selected=[];targets=set()
            for o in candidates:
                if o["track_ids"][1] not in targets:
                    selected.append(o);targets.add(o["track_ids"][1])
                if len(selected)>=8:break
            # Preserve technique alternatives for each retained pair. A cut
            # and a long blend have different prerequisites and musical uses.
            for target in sorted(targets):
                pair=[o for o in candidates if o['track_ids'][1]==target]
                techniques={o['technique'] for o in selected if o['track_ids'][1]==target}
                for option in pair:
                    if option['technique'] not in techniques:
                        selected.append(option);techniques.add(option['technique'])
                    if len(techniques)>=3:break
            compact.extend({k:o[k] for k in ("id","track_ids","technique","score","out_start","out_end","in_start","in_end","in_rate","out_rate","wall_seconds","warnings","confidence","actions","pitch_pct","key_lock")} for o in selected)
        compact_ids={o['id'] for o in compact}
        compact.extend({k:o[k] for k in ("id","track_ids","technique","score","out_start","out_end","in_start","in_end","in_rate","out_rate","wall_seconds","warnings","confidence","actions","pitch_pct","key_lock")} for o in route_options if o['id'] not in compact_ids)
        return {"schema_version": 1, "tracks": profiles, "transitions": compact, "brief": p.get("brief"),
                "validated_routes":routes,
                "instructions": "Use only supplied track IDs and validated transition IDs. Unknown evidence remains unknown. Return a schema_version=1 plan with track_ids, transition_ids, brief and explanation. No cue approvals or writes."}

    def spectrogram_image(self, p):
        """A bar-gridded spectrogram PNG of a track's current analysis, for AI structure reads."""
        from .aireads import render
        track = self.store.get(p["id"])
        name = f"{track['id']}-{track.get('revision') or 'none'}.png"
        result = render(track.get("analysis") or {}, Path(self.store.home) / "ai-reads" / name)
        return {**result, "track_id": track["id"], "revision": track.get("revision"),
                "title": track.get("title"), "artist": track.get("artist")}

    def observations_import(self, p):
        """Stores an AI provider's structure read as observations from its own source, beside
        rekordbox, the analysis and earlier opinions. A read of an older analysis is refused."""
        from .aireads import observations
        track = self.store.get(p["track_id"])
        if p.get("revision") is not None and p["revision"] != track.get("revision"):
            raise ValueError("The analysis changed since this read; read the track again")
        source, family = str(p.get("source") or ""), str(p.get("family") or "")
        if not source or not family:
            raise ValueError("Name the source and family of the read")
        track["observations"] = observations(track, source, family, p.get("events"),
                                             p.get("confidence", "unverified"), p.get("notes", ""))
        self.store.save(track)
        return {"track_id": track["id"], "observations": [o for o in track["observations"] if o["source"] == source]}

    def evidence(self, p):
        track = self.store.get(p["id"])
        from .evidence import resolve
        return {"track": track, "enrichment": self.store.artifact("enrichment", p["id"]),
                "resolved_opinions": resolve(track.get("observations", []),self.store.setting("source_reliability",{}))}

    def import_opinions(self, opinions, source):
        from .util import match_opinion
        from onset_service.cues import parse_mmss
        import numpy as np
        tracks=self.store.tracks()
        for track in tracks:
            opinion=match_opinion(opinions,track["path"],[t["path"] for t in tracks])
            if opinion is None: continue
            db=np.asarray((track.get("analysis") or {}).get("downbeats",[]))
            observations=[o for o in track.get("observations",[]) if o.get("source")!=source]
            for event in ("drop","breakdown","second_drop","outro"):
                t=parse_mmss(opinion.get(event))
                observations.append({"event":event,"source":source,"family":opinion.get("source",source),
                    "state":"failed" if "error" in opinion else "estimated" if t is not None else "unknown",
                    "original_time":t,"bar":int(np.argmin(abs(db-t))) if t is not None and len(db) else None,
                    "confidence":opinion.get("confidence","unverified"),"notes":opinion.get("notes","")})
            track["observations"]=observations;self.store.save(track)

    def reliability(self,p):
        from .evidence import learn_reliability
        result=learn_reliability(p.get("examples",[]));self.store.setting("source_reliability",value=result)
        return result

    def replan(self, p):
        """Keep the first `keep` tracks of a saved set and search the rest again."""
        from .planning import build_plans
        original = self.store.artifact("set", p["id"])
        if not original or original.get("stale"):
            raise ValueError("Set is missing or stale")
        self.validate_plan(original)
        keep = p.get("keep")
        if not isinstance(keep, int) or isinstance(keep, bool) or not 1 <= keep <= len(original["track_ids"]):
            raise ValueError("Keep between one track and the whole set")
        brief = p.get("brief") or original["brief"]
        prefix = {"track_ids": original["track_ids"][:keep], "transition_ids": original["transition_ids"][:keep - 1]}
        result = build_plans(self.store.tracks(), brief, self.known_transitions(brief, prefix["transition_ids"]), prefix)
        self.store.artifact_batch("transition", [(o['id'], o) for o in result['transitions']])
        self.store.artifact_batch("set", [(plan['id'], plan) for plan in result['plans']])
        return result

    def genre_suggestions(self, p):
        from .genre import suggest
        ids = p.get("track_ids")
        if ids is not None and (not isinstance(ids, list) or not all(isinstance(i, str) for i in ids)):
            raise ValueError("track_ids must be a list of track IDs")
        return suggest(self.store.tracks(), set(ids) if ids else None, int(p.get("neighbours", 3)))

    def review_queue(self, p):
        from .review import queue
        return queue(self.store.tracks(), self.store.setting("source_reliability", {}), self.store.artifacts("set"),
                     bool(p.get("include_done")))

    def import_workspace(self, p):
        """Import a workspace JSON or SQLite export from another machine with folder remapping."""
        from . import portable
        rules = portable.parse_rules(p.get("remap", []))
        source = Path(p["path"])
        if not source.is_file():
            raise ValueError("Workspace export file does not exist")
        if source.resolve() == self.store.db_path.resolve():
            raise ValueError("Choose an exported workspace, not the active database")
        history = {"revisions": [], "edits": []}
        if source.suffix.lower() == ".db":
            value, history = portable.read_sqlite_export(source)
        else:
            value = json.loads(source.read_text(encoding="utf-8-sig"))
            check_version(value)
            if value.get("kind") != "workspace" or not isinstance(value.get("tracks"), list):
                raise ValueError("Expected an OnSet workspace export")
        value = {**value, "tracks": [portable.remap_profile(t, rules) for t in value["tracks"]]}
        staging = self.store.home / "imports" / (uuid.uuid4().hex + ".json")
        atomic_json(staging, value)
        try:
            report = self.import_files([str(staging)])
        finally:
            staging.unlink(missing_ok=True)
        local = {t["id"]: t for t in self.store.tracks()}
        found = [t["id"] for t in value["tracks"] if t["id"] in local and local[t["id"]].get("analysis_state") != "missing"]
        missing = [t["id"] for t in value["tracks"] if t["id"] in local and local[t["id"]].get("analysis_state") == "missing"]
        restored = 0
        with self.store.lock:
            for revision, track_id, profile in history["revisions"]:
                if track_id in local:
                    restored += self.store.db.execute("INSERT OR IGNORE INTO revisions VALUES(?,?,?)", (revision, track_id, profile)).rowcount
            known_edits = {tuple(r) for r in self.store.db.execute("SELECT track_id, before_value, after_value, created FROM edits")}
            for track_id, before, after, undone, created in history["edits"]:
                if track_id in local and (track_id, before, after, created) not in known_edits:
                    self.store.db.execute("INSERT INTO edits(track_id,before_value,after_value,undone,created) VALUES(?,?,?,?,?)",
                                          (track_id, before, after, undone, created))
                    restored += 1
            if restored:
                self.store._commit()
        return {**report, "located": len(found), "missing_audio": missing, "history_rows": restored,
                "note": "Audio was matched by checksum after remapping; missing tracks can be relocated individually."}

    def relocate_cache(self, p):
        from .portable import relocate_cache
        result = relocate_cache(self.store, p["from"], p["to"], p.get("remap"))
        if p.get("set_default"):
            self.store.setting("cache_dir", value=str(Path(p["to"])))
        return result

    def replan_order(self,p):
        from .transitions import options
        original=self.store.artifact("set",p["id"])
        if not original or original.get("stale"): raise ValueError("Set is missing or stale")
        ids=p["track_ids"]
        if set(ids)!=set(original["track_ids"]) or len(ids)!=len(set(ids)): raise ValueError("Reordering must retain the same tracks")
        rate,entry=1.,0.;links=[]
        for a,b in zip(ids,ids[1:]):
            choices=options(self.store.get(a),self.store.get(b),original["brief"],outgoing_rate=rate,min_out_start=entry)
            if not choices: raise ValueError("This order has no validated transition for an adjacent pair")
            option=choices[0];self.store.artifact("transition",option["id"],option)
            links.append(option["id"]);rate=option["in_rate"];entry=option["in_end"]
        return self.import_plan({"schema_version":1,"track_ids":ids,"transition_ids":links,
            "brief":original["brief"],"explanation":"Manually reordered; adjacent transitions recalculated and validated."})

    def transition_alternatives(self,p):
        from .transitions import options
        original=self.store.artifact('set',p['id'])
        if not original or original.get('stale'):raise ValueError('Set is missing or stale')
        self.validate_plan(original)
        index=int(p['index'])
        if not 0<=index<len(original['transition_ids']):raise ValueError('Invalid transition position')
        current=self.store.artifact('transition',original['transition_ids'][index])
        a,b=[self.store.get(tid) for tid in current['track_ids']]
        alternatives=[]
        entry=self.store.artifact('transition',original['transition_ids'][index-1])['in_end'] if index else 0.
        for option in options(a,b,original['brief'],limit=18,outgoing_rate=current['out_rate'],min_out_start=entry):
            if option['id']==current['id']:continue
            self.store.artifact('transition',option['id'],option)
            changed={**original,'transition_ids':list(original['transition_ids']),'explanation':'Alternative transition selected; complete set timing revalidated.'}
            changed['transition_ids'][index]=option['id'];changed.pop('id',None)
            try:
                result=self.validate_plan(changed)
                plan={**changed,'duration_seconds':result['duration_seconds'],'stale':False,
                      'explanation':'Alternative transition; complete set timing validated.'}
                plan['id']=digest(plan)[:24]
            except ValueError:continue
            alternatives.append({'transition':option,'plan':plan})
            if len(alternatives)>=3:break
        return alternatives

    def enrichment(self, p):
        import copy
        from urllib.parse import urlparse
        check_version(p)
        records = copy.deepcopy(p.get("records", []))
        allowed = {t["id"] for t in self.store.tracks()}
        seen = set()
        for record in records:
            if record["track_id"] in seen:
                raise ValueError("Duplicate enrichment record for a track")
            seen.add(record["track_id"])
            if record["track_id"] not in allowed:
                raise ValueError("Enrichment references an unknown track")
            if p.get("allowed_ids") is not None and record["track_id"] not in p["allowed_ids"]:
                raise ValueError("Enrichment references a track outside this request")
            for claim in record.get("claims", []):
                if claim.get("kind") not in {"genre", "mood", "instrumentation", "release", "description"}:
                    raise ValueError("Enrichment cannot override measured facts")
                if claim.get("source_type") not in {"web", "model", "manual"}:
                    raise ValueError("Unknown enrichment source type")
                if claim.get("source_type") == "web":
                    url = urlparse(str(claim.get("url") or ""))
                    if url.scheme != "https" or not url.hostname or url.username or url.password:
                        raise ValueError("Web claims require HTTPS citations")
                claim["retrieved"] = utcnow()
        self.store.artifact_batch("enrichment", [(r["track_id"], r) for r in records])
        return {"saved": len(records)}

    def calibrate_energy(self, p):
        import numpy as np
        tracks = [t for t in self.store.tracks() if (t.get("analysis") or {}).get("energy_profile")]
        if len(tracks) < 3:
            raise ValueError("Analyse at least three tracks before library-relative calibration")
        vectors = np.array([t["analysis"]["energy_profile"].get("acoustic_summary",[t["analysis"]["energy_profile"]["raw_summary"]]*5) for t in tracks])
        lo,hi=np.percentile(vectors,[10,90],axis=0)
        features = np.clip((vectors-lo)/np.maximum(hi-lo,1e-8),0,1)
        weights = np.array([.2,.25,.15,.2,.2])
        judgments = self.store.artifacts("energy-judgment")
        by_id = {t["id"]:i for i,t in enumerate(tracks)}
        usable = [j for j in judgments if j["a"] in by_id and j["b"] in by_id]
        rated = [(i,t["energy"]) for i,t in enumerate(tracks) if t.get("energy") is not None]
        if usable or rated:
            from scipy.optimize import minimize
            prior = weights.copy()
            def loss(w):
                value = .1 * float(np.sum((w-prior)**2))
                for j in usable:
                    delta = float((features[by_id[j["a"]]]-features[by_id[j["b"]]]) @ w) * 5
                    value += delta**2 if j["winner"] == "equal" else float(np.logaddexp(0, -delta if j["winner"] == j["a"] else delta))
                for index,rating in rated:
                    value += 2 * float((features[index] @ w - (rating-1)/9)**2)
                return value
            fit = minimize(loss, weights, bounds=[(0,2)]*5)
            if fit.success: weights = fit.x
        values = (features @ weights).tolist()
        if len(values) < 3:
            raise ValueError("Analyse at least three tracks before library-relative calibration")
        reference = digest([sorted((t["fingerprint"], v) for t, v in zip(tracks, values)), usable, rated])
        for track, value in zip(tracks, values):
            ep = track["analysis"]["energy_profile"]
            ep["rating"] = round(1 + 9 * float(np.mean(np.asarray(values) <= value)), 1)
            ep["calibration"] = reference
            ep["weights"] = weights.tolist()
            ep["judgment_count"] = len(usable)
            ep["note"] = "Relative to this collection; fitted to supplied judgments" if usable or rated else "Relative to this collection; unverified against DJ judgments"
            self.store.invalidate(track["id"])
            self.store.save(track)
        return {"calibration": reference, "tracks": len(tracks)}

    def energy_judgment(self, p):
        for tid in (p["a"], p["b"]):
            self.store.get(tid)
        if p["winner"] not in (p["a"], p["b"], "equal"):
            raise ValueError("Invalid energy judgment")
        record = {**p, "created": utcnow(), "id": uuid.uuid4().hex}
        return self.store.artifact("energy-judgment", record["id"], record)

    def benchmark(self, p):
        from .benchmark import split, event_metrics
        if p.get('baseline') and p.get('candidate'):
            from .benchmark import compare_releases
            return compare_releases(p['baseline'],p['candidate'])
        if p.get("annotation"):
            from .benchmark import evaluate
            return evaluate(self.store.tracks(), p["annotation"], p.get("subset"))
        if p.get("evaluate"):
            from .benchmark import evaluate
            return evaluate(self.store.tracks(), self.benchmark({}), p.get("subset"))
        if p.get("labels"):
            labels = p["labels"]
            predictions = {t["id"]: (t.get("analysis") or {}).get("sections", {}) for t in self.store.tracks()}
            return {str(tol): event_metrics(predictions, labels, tol) for tol in (0, 1, 2)}
        template = split(self.store.tracks())
        for index,record in enumerate(template["records"]):
            saved = self.store.artifact("benchmark-label", record["track_id"])
            if saved: template["records"][index] = {**record, **saved, "split":record["split"],"group":record["group"]}
        template["energy_judgments"] = self.store.artifacts("energy-judgment")
        return template

    def benchmark_label(self, p):
        self.store.get(p["track_id"])
        check_version(p)
        if not isinstance(p.get("verified"),bool):raise ValueError("Specify whether the annotation was verified by listening")
        allowed = {"schema_version","track_id","verified","events","keys","bpm","sections","grid_samples","boundaries","notes"}
        if set(p)-allowed:raise ValueError("Unsupported benchmark annotation fields")
        for event in p.get("events",{}).values():
            if event.get("state") not in {"present","absent","unknown"}:raise ValueError("Event state must be present, absent or unknown")
            if event["state"] == "present" and (not isinstance(event.get("bar"),int) or event["bar"]<0):raise ValueError("Present events need a nonnegative bar index")
        return self.store.artifact("benchmark-label",p["track_id"],{**p,"verified_at":utcnow()})

    def rb_preview(self, p):
        from .rekordbox import prepare, prepare_playlist, prepare_adoption, prepare_library_adoption, prepare_hot_slot_repair
        if p.get("repair_hot_slots"):
            return prepare_hot_slot_repair(self.store, p.get("db_path"), p.get("plain", False))
        if p.get("adopt") and p.get("all"):
            return prepare_library_adoption(self.store, p.get("db_path"), p.get("plain", False))
        if p.get("adopt"):return prepare_adoption(self.store,p["track_ids"][0],p.get("db_path"),p.get("plain",False),p.get("hot",False))
        if p.get("plan_id"):
            return prepare_playlist(self.store, p["plan_id"], p["playlist_name"], p.get("db_path"), p.get("plain", False))
        return prepare(self.store, p.get("track_ids") or [t["id"] for t in self.store.tracks()], p.get("db_path"), p.get("plain", False), p.get("hot", False), p.get("comments", False))

    def rb_apply(self, p):
        if self.show:
            raise ValueError("rekordbox is not written to during a show")
        from .rekordbox import apply
        return apply(self.store, p["id"])

    def restore(self, p):
        if self.show:
            raise ValueError("rekordbox is not written to during a show")
        from .rekordbox import restore
        return restore(self.store, p["id"])

    def rb_import(self, p):
        from .rekordbox import open_db, consistent_backup, supported
        from onset_service.cuewriter import read_grid_and_phrases, phrase_sections
        import tempfile
        self.notify({'method':'import-progress','params':{'stage':'Opening rekordbox snapshot'}})
        db = open_db(p.get("db_path"), p.get("plain", False))
        try:
            supported(db)
            with tempfile.TemporaryDirectory(prefix="onset-snapshot-") as tmp:
                location = consistent_backup(db, Path(tmp) / "master.db")
                snapshot = open_db(location, p.get("plain", False))
                snapshot._share_dir = db.share_directory
                result, skipped, failed = [], [], []
                try:
                    contents = list(snapshot.get_content())
                    for index, content in enumerate(contents):
                        try:
                            if not content.FolderPath or not Path(content.FolderPath).is_file():
                                skipped.append({'rekordbox_id':str(content.ID),'path':content.FolderPath,'reason':'Audio file is missing'})
                                continue
                            track = self.store.import_track(content.FolderPath)
                            rb_metadata = {"title": content.Title or track["title"], "bpm": (content.BPM or 0) / 100,
                                           "key": content.Key.ScaleName if content.Key else None}
                            track["rekordbox_id"] = content.ID
                            track["rekordbox_metadata"] = rb_metadata
                            if not track.get('duration'):
                                track['duration'] = float(content.Length or 0)
                            if not track.get("revision"):
                                track.update({k:v for k,v in rb_metadata.items() if k not in track.get("overrides", {})})
                            try:
                                downbeats, beats, phrases, mood = read_grid_and_phrases(snapshot, content)
                                rb = phrase_sections(phrases, downbeats)
                                rb_observation = {"downbeats": downbeats.tolist(), "beats": beats.tolist(), "bpm": rb_metadata["bpm"],
                                                     "grid_source": "rekordbox", "phrase_anchor": (rb or {}).get("drop"),
                                                     "phrases": phrases, "rekordbox_sections": rb}
                                track["rekordbox_observation"] = rb_observation
                                track.pop('grid_error',None)
                                if not track.get("revision"):
                                    track["analysis"] = rb_observation
                            except Exception as exc:
                                track["grid_error"] = str(exc)
                            self.store.save(track)
                            result.append(track["id"])
                        except Exception as exc:
                            failed.append({'rekordbox_id':str(content.ID),'path':content.FolderPath,'error':str(exc)})
                        finally:
                            if (index+1)%5==0 or index+1==len(contents):
                                self.notify({'method':'import-progress','params':{'stage':'Reading rekordbox','completed':index+1,'total':len(contents),'imported':len(result),'skipped':len(skipped),'failed':len(failed)}})
                    memberships, playlist_error = {}, None
                    try:
                        for playlist in snapshot.get_playlist():
                            if getattr(playlist, "Attribute", 0) not in (0, None):
                                continue  # folders and smart playlists
                            for song in snapshot.get_playlist_songs(PlaylistID=playlist.ID):
                                memberships.setdefault(str(song.ContentID), set()).add(playlist.Name)
                    except Exception as exc:
                        playlist_error = 'Playlist membership could not be read: ' + str(exc)
                finally:
                    snapshot.close()
                    snapshot.engine.dispose()
                if playlist_error is None:
                    for track_id in dict.fromkeys(result):
                        track = self.store.get(track_id)
                        names = sorted(memberships.get(str(track.get("rekordbox_id")), ()))
                        if track.get("rekordbox_playlists") != names:
                            track["rekordbox_playlists"] = names
                            self.store.save(track)
                report = {"imported":len(result),"track_ids":list(dict.fromkeys(result)),"skipped":skipped,"failed":failed,
                          "playlists":len({n for names in memberships.values() for n in names}),"playlist_error":playlist_error}
                self.store.artifact('import-report',uuid.uuid4().hex,report)
                return report
        finally:
            db.close()
            db.engine.dispose()

    def doctor(self):
        import platform
        import psutil
        result = {"schema_version": 1, "python": sys.version.split()[0], "platform": platform.platform(),
                  "cpu": platform.processor(), "ram_bytes": psutil.virtual_memory().total,
                  "disk_free_bytes": shutil.disk_usage(self.store.home).free, "home": str(self.store.home),
                  "ffmpeg": shutil.which("ffmpeg"), "packages": {}, "models": []}
        for package in ("numpy", "scipy", "librosa", "soundfile", "pyrekordbox", "torch"):
            result["packages"][package] = bool(importlib.util.find_spec(package))
        try:
            output = subprocess.run(["wsl.exe", "--list", "--quiet"], capture_output=True, timeout=10)
            result["wsl"] = {"available": output.returncode == 0, "distributions": output.stdout.decode("utf-16-le", errors="replace").strip(),
                             "error": output.stderr.decode("utf-16-le", errors="replace").strip() if output.returncode else None}
        except (OSError, subprocess.TimeoutExpired) as exc:
            result["wsl"] = {"available": False, "error": str(exc)}
        if result["packages"]["pyrekordbox"]:
            try:
                from pyrekordbox.config import get_config
                config = get_config("rekordbox7") or get_config("rekordbox6")
                result["rekordbox"] = {"db_path": str(config.get("db_path", "")), "detected": bool(config)}
            except Exception as exc:
                result["rekordbox"] = {"detected": False, "error": str(exc)}
        result["models"] = [str(p.relative_to(self.store.home)) for p in (self.store.home / "models").glob("**/*.pt")]
        result["workspace"] = workspace.health(self.store)
        return result

    def export(self, p):
        path = Path(p["path"])
        kind = p.get("kind", "profiles")
        if path.resolve() == self.store.db_path.resolve() or path.name.lower() in {"master.db", "accounts.enc", "workspace-state.json"}:
            raise ValueError("Choose an export file outside active application or rekordbox data")
        if kind == "workspace-backup":
            record = workspace.backup(self.store, "portable-export")
            _, source = workspace.verified_backup(self.store, record["id"])
            if path.resolve() == source.resolve(): raise ValueError("Choose a separate export destination")
            path.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source,path)
            from .util import file_hash
            if file_hash(path) != record["sha256"]: raise ValueError("Exported workspace checksum differs")
            return {"path":str(path),"sha256":record["sha256"],"counts":record["counts"],"schema_version":1}
        tracks = self.store.tracks()
        if kind == "workspace":
            value={"schema_version":1,"kind":"workspace","tracks":tracks,"transitions":self.store.artifacts('transition'),
                   "plans":self.store.artifacts('set'),"benchmark_labels":self.store.artifacts('benchmark-label')}
        elif kind == "ai-context":
            value = self.ai_context(p)
        elif kind == "benchmark":
            value = self.benchmark({})
        elif kind == "plan":
            value = self.store.artifact("set", p["id"])
            if not value:
                raise ValueError("Unknown set")
            self.validate_plan(value)
        else:
            value = {"schema_version": 1, "tracks": tracks}
        if path.suffix.lower() == ".csv":
            with path.open("w", newline="", encoding="utf-8-sig") as f:
                writer = csv.writer(f)
                if kind == "plan":
                    lookup={t["id"]:t for t in tracks}
                    writer.writerow(["order","track","bpm","key","technique","outgoing_start","incoming_start","handover_seconds","incoming_pitch_pct","instructions"])
                    for i,tid in enumerate(value["track_ids"]):
                        t=lookup[tid]
                        link=self.store.artifact("transition",value["transition_ids"][i]) if i<len(value["transition_ids"]) else None
                        writer.writerow([i+1,t["title"],t.get("bpm"),t.get("key"),link["name"] if link else "finish",
                            link["out_start"] if link else "",link["in_start"] if link else "",link["wall_seconds"] if link else "",
                            link["pitch_pct"] if link else "","; ".join(a["action"] for a in link["actions"]) if link else ""])
                else:
                    writer.writerow(["id", "title", "bpm", "key", "analysis", "review", "write"])
                    for t in tracks:
                        writer.writerow([t["id"], t["title"], t.get("bpm"), t.get("key"), t["analysis_state"], t["review_state"], t["write_state"]])
        elif path.suffix.lower() == ".html" and kind == "plan":
            lookup = {t["id"]: t for t in tracks}
            rows = []
            for i, tid in enumerate(value["track_ids"]):
                t = lookup[tid]
                link = self.store.artifact("transition", value["transition_ids"][i]) if i < len(value["transition_ids"]) else None
                detail = "<br>".join(html.escape(a["action"]) for a in link["actions"]) if link else "Finish"
                rows.append(f"<tr><td>{i + 1}</td><td>{html.escape(t['title'])}</td><td>{t.get('bpm')}</td><td>{html.escape(str(t.get('key')))}</td><td>{detail}</td></tr>")
            path.write_text("<!doctype html><meta charset='utf-8'><title>OnSet set sheet</title><style>body{font:14px system-ui}td,th{padding:12px;border-bottom:1px solid #ccc}table{border-collapse:collapse;width:100%}</style><h1>OnSet set sheet</h1><p>" + html.escape(value.get("explanation", "")) + "</p><table><tr><th>#</th><th>Track</th><th>BPM</th><th>Key</th><th>Transition</th></tr>" + "".join(rows) + "</table>", encoding="utf-8")
        else:
            atomic_json(path, value)
        return {"path": str(path.resolve())}
