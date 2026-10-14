#!/usr/bin/env python
"""
verify.py - second opinion for the tracks that need one.

Picks tracks where the first Gemini pass is missing/errored or disagrees with the
energy analysis on the drop by > 2 bars, writes them to verify_subset.json and asks
Gemini again (hybrid mode, supervisor retries) into gemini2.json. Then regenerate
with:  autocue.py --from-json analysis.json --gemini gemini.json --gemini gemini2.json ...

  verify.py analysis.json gemini.json [--out gemini2.json] [--proxy-dir DIR] [--all-flagged]
"""
import argparse, json, subprocess, sys
from pathlib import Path
import numpy as np
import autocue_core as core

ap = argparse.ArgumentParser()
ap.add_argument("analysis"); ap.add_argument("gemini")
ap.add_argument("--out", default="gemini2.json")
ap.add_argument("--proxy-dir", default="proxies")
ap.add_argument("--all-flagged", action="store_true", help="also re-ask every track whose review status is CHECK")
args = ap.parse_args()

A = json.loads(Path(args.analysis).read_text()); G = json.loads(Path(args.gemini).read_text())
subset = []
for r in A:
    g = G.get(r["path"])
    why = None
    if not g or "error" in g:
        why = "no first opinion"
    else:
        t = core.parse_mmss(g.get("drop")); mine = r["sections"].get("drop")
        if t is not None and mine is not None:
            theirs = int(np.argmin(np.abs(np.asarray(r["downbeats"]) - t)))
            if abs(theirs - mine) > 2:
                why = f"drop disagrees (energy {core.fmt_time(r['downbeats'][mine])} vs gemini {g['drop']})"
        elif t is None or mine is None:
            why = "drop missing on one side"
        if why is None and args.all_flagged and core.review_status(r) == "CHECK":
            why = "flagged"
    if why:
        subset.append(r)
        print(f"  {Path(r['path']).name[:60]:60} {why}")
print(f"\n{len(subset)} tracks need a second opinion")
Path("verify_subset.json").write_text(json.dumps(subset))
if subset:
    cmd = [sys.executable, "-u", "gemini_check.py", "verify_subset.json", "--out", args.out, "--jobs", "3",
           "--proxy-dir", args.proxy_dir, "--hybrid", "--until-done", "--max-passes", "50"]
    print(" ".join(cmd), flush=True)
    subprocess.run(cmd)
