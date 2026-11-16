#!/usr/bin/env python
"""refresh.py analysis.json - recompute energy curves/sections for every entry with the current core.
Reuses cached madmom grids when present; runs madmom only for entries without downbeats."""
import json, sys, time
from pathlib import Path
from onset_service import cues as core
from tools import cues_xml
from onset_service.util import atomic_json

p = Path(sys.argv[1])
results = json.loads(p.read_text())
for i, r in enumerate(results, 1):
    t0 = time.time()
    if "downbeats" not in r:
        downbeats, bpm, first, cv = cues_xml.beat_grid(r["path"])
        r.update(bpm=round(bpm, 2), first_beat=first, tempo_cv=round(cv, 4))
    else:
        downbeats = r["downbeats"]
    new = core.analyze_with_grid(r["path"], downbeats, phrase_anchor=r.get("phrase_anchor", 0))
    for k in ("sections", "flags", "cues", "energy", "energy_hi", "downbeats", "duration", "bars"):
        r[k] = new[k]
    if r.get("tempo_cv", 0) > 0.03:
        r["flags"].append(f"tempo unstable (cv {r['tempo_cv']:.1%}) - sync may drift")
    atomic_json(p, results)
    print(f"[{i}/{len(results)}] {Path(r['path']).name[:60]:60} {time.time() - t0:5.1f}s", flush=True)
