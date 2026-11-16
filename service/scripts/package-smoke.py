"""Exercise the audit-gap service methods with a packaged runtime and isolated data.

Usage (with the packaged runtime's python.exe):
  python.exe -I scripts/package-smoke.py <resources-dir> <work-dir> <output-json>

Generated audio only; never touches the active workspace, rekordbox or the network.
"""
import json
import shutil
import sys
import time
from pathlib import Path

resources, work, output = Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve(), Path(sys.argv[3]).resolve()
sys.path.insert(0, str(resources / "python-src"))
import numpy as np
import soundfile as sf
import onset_service
from onset_service.service import Service

if work.exists():
    shutil.rmtree(work)
music = work / "old-machine" / "Music"
music.mkdir(parents=True)
results = {"schema_version": 1, "python_src": str(Path(onset_service.__file__).parent), "executable": sys.executable, "checks": {}}


def check(name, condition, detail=None):
    results["checks"][name] = {"passed": bool(condition), "detail": detail}


def song(path, seed):
    sr, seconds = 22050, 96
    t = np.arange(sr * seconds) / sr
    y = .06 * np.sin(2 * np.pi * (220 + seed * 30) * t) + .3 * np.exp(-np.mod(t, .5) * 40) * np.sin(2 * np.pi * 60 * t)
    y[(t > 40) & (t < 56)] *= .1
    sf.write(path, y, sr)


service = Service(work / "app")
ids = []
for i in range(4):
    path = music / f"track-{i}.wav"
    song(path, i)
    ids.append(service.store.import_track(path)["id"])
job = service.dispatch("analyze", {"track_ids": ids, "deep": False})
for _ in range(3000):
    record = next(j for j in service.store.jobs() if j["id"] == job["id"])
    if record["state"] not in {"queued", "running"}:
        break
    time.sleep(.1)
check("light_analysis", record["state"] == "complete", record.get("failures") or record["state"])
tracks = [service.store.get(i) for i in ids]
check("mix_windows_field", all("mix_windows" in t["analysis"]["structure"] for t in tracks),
      [len(t["analysis"]["structure"]["mix_windows"]) for t in tracks])
check("resolution_version", all(t["analysis"]["resolution_version"] == "structure-resolution-3" for t in tracks))
queue = service.dispatch("review-queue", {})
check("review_queue", len(queue) == 4 and all(r["reasons"] for r in queue), [r["score"] for r in queue])
service.store.edit(ids[1], {"key": "C# major"})
brief = {"track_ids": ids, "target_seconds": 240, "tolerance_seconds": 200, "smoothness": .8, "contrast": .2,
         "transpose": {ids[1]: -1}}
plan_job = service.dispatch("plan-job", {"brief": brief})
for _ in range(3000):
    record = next(j for j in service.store.jobs() if j["id"] == plan_job["id"])
    if record["state"] not in {"queued", "running"}:
        break
    time.sleep(.1)
plan_ids = (record.get("result") or {}).get("plan_ids", [])
check("plan_job", record["state"] == "complete" and plan_ids, record.get("result"))
if plan_ids:
    plan = service.store.artifact("set", plan_ids[0])
    links = [service.store.artifact("transition", t) for t in plan["transition_ids"]]
    check("transpose_carried", all(l.get("out_transpose") == brief["transpose"].get(l["track_ids"][0], 0) and
                                   l.get("in_transpose") == brief["transpose"].get(l["track_ids"][1], 0) for l in links))
    check("vocal_overlap_reported", all("vocal_overlap" in l for l in links), [l["vocal_overlap"]["state"] for l in links])
    if len(plan["track_ids"]) > 2:
        replanned = service.dispatch("replan", {"id": plan["id"], "keep": 2})
        check("replan_keeps_prefix", replanned["plans"] and all(p["track_ids"][:2] == plan["track_ids"][:2] for p in replanned["plans"]))
    pinned = {**brief, "pinned": [{"transition_id": plan["transition_ids"][0]}]}
    forced = service.dispatch("plan", {"brief": pinned})
    check("pinned_transition_search", forced["plans"] and all(plan["transition_ids"][0] in p["transition_ids"] for p in forced["plans"]))
slow = service.dispatch("plan-job", {"brief": {**brief, "target_seconds": 300, "mood": "cancel-check"}})
service.job_control(slow["id"], "cancel")
for _ in range(3000):
    record = next(j for j in service.store.jobs() if j["id"] == slow["id"])
    if record["state"] not in {"queued", "running", "cancelling"}:
        break
    time.sleep(.1)
check("plan_job_cancel", record["state"] in {"cancelled", "complete"}, record["state"])
feature = Path(tracks[0]["analysis"]["features"]["path"])
cache_old = feature.parent.parent
exported = work / "export.db"
service.export({"path": str(exported), "kind": "workspace-backup"})
json_export = work / "export.json"
service.export({"path": str(json_export), "kind": "workspace"})
service.close()
moved = work / "moved-cache"
shutil.move(str(cache_old), str(moved))
reopened = Service(work / "app")
relocated = reopened.dispatch("relocate-cache", {"from": str(cache_old), "to": str(moved), "set_default": True})
check("relocate_cache", relocated["updated"] == 4 and not relocated["missing"], relocated)
reopened.close()
new_music = work / "new-machine" / "Library"
shutil.copytree(music, new_music)
shutil.rmtree(music)
for name, source in (("sqlite", exported), ("json", json_export)):
    target = Service(work / f"target-{name}")
    report = target.dispatch("import-workspace", {"path": str(source), "remap": [{"from": str(music), "to": str(new_music)}]})
    check(f"import_workspace_{name}", report["located"] == 4 and not report["missing_audio"],
          {k: report[k] for k in ("imported", "located", "missing_audio", "history_rows")})
    target.close()
results["passed"] = all(c["passed"] for c in results["checks"].values())
output.parent.mkdir(parents=True, exist_ok=True)
output.write_text(json.dumps(results, indent=2, default=str), encoding="utf-8")
print(json.dumps(results, indent=2, default=str))
sys.exit(0 if results["passed"] else 1)
