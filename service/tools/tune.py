#!/usr/bin/env python
"""
tune.py - sweep cues thresholds offline against Gemini's labels.

Uses the per-bar energy curves cached in analysis.json, so each parameter set
re-runs section detection on every track in milliseconds. Agreement = the
detected bar is within 2 bars of Gemini's (bar-snapped) timestamp.

  tune.py analysis.json gemini.json
"""
import itertools, json, sys
from pathlib import Path
import numpy as np
from onset_service import cues as core

analysis = json.loads(Path(sys.argv[1]).read_text())
llm = json.loads(Path(sys.argv[2]).read_text())

tracks = []
for r in analysis:
    g = llm.get(r["path"])
    if not g or "error" in g or "energy_hi" not in r or "downbeats" not in r:
        continue
    db = np.asarray(r["downbeats"])
    truth = {}
    for k in ("drop", "breakdown", "outro"):
        t = core.parse_mmss(g.get(k))
        truth[k] = None if t is None else int(np.argmin(np.abs(db - t)))
    tracks.append((r, truth))
print(f"{len(tracks)} tracks with both energy curves and Gemini labels\n")

def score(params):
    agree = {"drop": 0, "breakdown": 0, "outro": 0}
    both = {"drop": 0, "breakdown": 0, "outro": 0}
    for r, truth in tracks:
        sec = core.find_sections(r["energy"], r["energy_hi"], phrase_anchor=r.get("phrase_anchor", 0), **params)
        for k in agree:
            if truth[k] is not None and sec[k] is not None:
                both[k] += 1
                agree[k] += abs(sec[k] - truth[k]) <= 2
    return agree, both

grid = dict(drop_min_jump=[4, 5, 6, 8], drop_rel=[0.3, 0.4, 0.5, 0.6, 0.8],
            break_min=[4, 5, 6, 8], high_pctl=[80, 85, 90])
rows = []
for combo in itertools.product(*grid.values()):
    params = dict(zip(grid.keys(), combo))
    agree, both = score(params)
    total = agree["drop"] + 0.5 * agree["breakdown"] + 0.5 * agree["outro"]
    rows.append((total, params, agree, both))
rows.sort(key=lambda x: -x[0])

print("current defaults:")
agree, both = score({})
print(f"  drop {agree['drop']}/{both['drop']}  breakdown {agree['breakdown']}/{both['breakdown']}  outro {agree['outro']}/{both['outro']}")
print("\ntop 10:")
for total, params, agree, both in rows[:10]:
    print(f"  {total:6.1f}  {params}  drop {agree['drop']}/{both['drop']} break {agree['breakdown']}/{both['breakdown']} outro {agree['outro']}/{both['outro']}")

print("\nmarginal effect of each parameter (mean score):")
for name, values in grid.items():
    means = {v: np.mean([t for t, p, _, _ in rows if p[name] == v]) for v in values}
    print(f"  {name:14} " + "  ".join(f"{v}:{m:.1f}" for v, m in means.items()))
