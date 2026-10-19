#!/usr/bin/env python
"""spec_record.py analysis.json claude_spec.json  <  lines of:  <name substring> | drop | breakdown | second | outro | conf | note
Bar indices (or - for none). Writes/merges a gemini.json-style file keyed by path with m:ss times."""
import json, sys
from pathlib import Path
A = json.loads(Path(sys.argv[1]).read_text()); out = Path(sys.argv[2])
D = json.loads(out.read_text()) if out.exists() else {}
def mmss(t): return f"{int(t)//60}:{int(t)%60:02d}"
for line in sys.stdin:
    line = line.lstrip("﻿")   # PowerShell pipes prepend a BOM
    if not line.strip() or line.startswith("#"): continue
    parts = [x.strip() for x in line.split("|")]
    name, vals, conf = parts[0], parts[1:5], (parts[5] if len(parts) > 5 else "medium")
    note = parts[6] if len(parts) > 6 else ""
    hits = [r for r in A if name.lower() in Path(r["path"]).stem.lower()]
    exact = [r for r in hits if Path(r["path"]).stem.lower() == name.lower()]
    hits = exact or hits
    if len(hits) != 1:
        print(f"!! {name}: {len(hits)} matches"); continue
    r = hits[0]; db = r["downbeats"]
    def T(v):
        if v in ("-", ""): return None
        i = int(v); return mmss(db[min(i, len(db) - 1)])
    D[r["path"]] = {"drop": T(vals[0]), "breakdown": T(vals[1]), "second_drop": T(vals[2]), "outro": T(vals[3]),
                    "confidence": conf, "notes": note, "source": "claude-spectrogram"}
    print(f"ok {Path(r['path']).stem[:50]:50} D={D[r['path']]['drop']} E={D[r['path']]['breakdown']} G={D[r['path']]['second_drop']} F={D[r['path']]['outro']}")
out.write_text(json.dumps(D, indent=1)); print(len(D), "recorded")
