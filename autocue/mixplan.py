#!/usr/bin/env python
"""
mixplan.py - link every track's mix-OUT cue to the mix-IN cue of the tracks that follow it well.

rekordbox has no "mix point link" of its own (checked: none of the 48 tables in master.db
7.2.18 links a cue in one track to a cue in another; Related Tracks lists are rule-based and
store no membership). So the links are computed here and delivered as:

  mixplan.csv   every good transition: from-track + out-cue -> to-track + in-cue, with the
                pitch change and key move it needs, best first
  mixset.csv    one greedy set order through the library using those transitions
  --write-playlist NAME   that set order as a real rekordbox playlist (rekordbox must be closed)

Scoring (see score_pair):
  key   Camelot move - same key, +-1 on the wheel, relative major/minor, or +7 (energy boost)
  bpm   how far the pitch fader has to go; half/double time counted as a match
  and the transition itself is built from the cues autocue already placed: you mix OUT of A at
  its outro (F) or breakdown (E), and IN to B far enough ahead of B's drop that the drop lands
  after the blend - B's 32-bars-before-drop cue (B), else 16 (C), else the intro (A).

  mixplan.py --top 5
  mixplan.py --from "FISHER - Stop It"          # just this track's options
  mixplan.py --set --start "Daft Punk" --write-playlist "autocue set"
"""
import argparse
import csv
import json
import re
import sys
from pathlib import Path

from autocue_core import fmt_time, norm_cues

MAX_PITCH_PCT = 6.0    # most DJs will not pitch a track further than this to match a BPM


# --- musical compatibility -------------------------------------------------
def parse_camelot(key):
    """'8A' -> (8, 'A'). rekordbox stores the Camelot code when the key display is set to it."""
    m = re.match(r"^\s*(\d{1,2})\s*([ABab])\s*$", str(key or ""))
    return (int(m.group(1)), m.group(2).upper()) if m else None


def key_move(a, b):
    """(score, label) for going from Camelot key a to key b. 0 = do not mix these."""
    ka, kb = parse_camelot(a), parse_camelot(b)
    if not ka or not kb:
        return 0, ""
    (na, la), (nb, lb) = ka, kb
    step = (nb - na) % 12
    if la == lb:
        if step == 0:
            return 4, "same key"
        if step in (1, 11):
            return 3, "+1 on the wheel" if step == 1 else "-1 on the wheel"
        if step == 7:
            return 2, "+7 energy boost"
    elif step == 0:
        return 3, "relative major/minor"
    return 0, ""


def bpm_move(a, b):
    """(score, label, pitch_pct).

    Half/double time is a real transition (a 75 BPM track can follow a 150 one and feel the same
    tempo) but it is a deliberate move, not a default - scored below any same-tempo match so it
    never wins a tie, otherwise a set order just ping-pongs between 75 and 150."""
    if not a or not b:
        return 0, "", 0.0
    best = None
    for factor, tag, cap in ((1.0, "", 3), (2.0, " (double-time)", 1), (0.5, " (half-time)", 1)):
        pct = (b * factor - a) / a * 100.0
        if abs(pct) > MAX_PITCH_PCT:
            continue
        score = min(cap, 3 if abs(pct) <= 1.0 else (2 if abs(pct) <= 3.0 else 1))
        if best is None or score > best[0]:
            best = (score, pct, tag)
    if best is None:
        return 0, "", 0.0
    score, pct, tag = best
    return score, f"{pct:+.1f}%{tag}", pct


def score_pair(a, b):
    """Total score for playing b after a, plus the reason. 0 means incompatible."""
    ks, klabel = key_move(a.get("key"), b.get("key"))
    bs, blabel, pct = bpm_move(a.get("bpm"), b.get("bpm"))
    if not ks or not bs:
        return 0, "", pct
    return ks + bs, f"{klabel}, {blabel}", pct


# --- the transition itself -------------------------------------------------
def out_cue(t):
    """Where you start mixing out of this track: the outro, else the breakdown, else nothing."""
    for letter in ("F", "E"):
        if t["cues"].get(letter) is not None:
            return letter, t["cues"][letter]
    return None, None


def in_cue(t, bars_available):
    """Where the next track comes in, given how many bars the outgoing track still has.

    B is 32 bars before its drop and C is 16, so pick the one that fits in the blend - that way
    the incoming drop lands when the outgoing track has finished, not on top of it."""
    for letter, need in (("B", 32), ("C", 16)):
        if t["cues"].get(letter) is not None and bars_available >= need:
            return letter, t["cues"][letter]
    return "A", t["cues"].get("A", 0.0)


def transition(a, b):
    """Describe mixing out of a into b, or None if a has no usable mix-out point."""
    o_letter, o_time = out_cue(a)
    if o_time is None:
        return None
    bar = 4 * 60.0 / a["bpm"] if a.get("bpm") else 2.0
    bars_available = max(0.0, (a.get("duration") or o_time) - o_time) / bar
    i_letter, i_time = in_cue(b, bars_available)
    return {"out_letter": o_letter, "out_time": o_time, "bars": bars_available,
            "in_letter": i_letter, "in_time": i_time}


# --- data ------------------------------------------------------------------
def load_tracks(analysis_path, db=None):
    """Merge autocue's cues with rekordbox's BPM/key/title, keyed by file name."""
    rows = json.loads(Path(analysis_path).read_text(encoding="utf-8"))
    tracks = {}
    for r in rows:
        name = Path(r["path"]).name
        cues = {c[0]: c[2] for c in norm_cues(r["cues"])}
        tracks[name] = {"name": name, "stem": Path(r["path"]).stem, "path": r["path"],
                        "cues": cues, "duration": r.get("duration"), "bpm": r.get("bpm") or None,
                        "key": None, "title": Path(r["path"]).stem, "content_id": None,
                        "flags": r.get("flags", [])}
    if db is not None:
        for c in db.get_content():
            name = Path(c.FolderPath or "").name
            t = tracks.get(name)
            if not t:
                continue
            t["key"] = c.Key.ScaleName if c.Key else None
            t["bpm"] = (c.BPM or 0) / 100 or t["bpm"]
            t["title"] = c.Title or t["title"]
            t["content_id"] = c.ID
    return tracks


def pairs(tracks, top=5, only=None):
    """Every compatible transition, best first, capped at `top` per source track."""
    out = []
    for a in tracks.values():
        if only and only.lower() not in a["stem"].lower():
            continue
        cand = []
        for b in tracks.values():
            if b["name"] == a["name"]:
                continue
            score, why, pct = score_pair(a, b)
            if not score:
                continue
            tr = transition(a, b)
            if tr is None:
                continue
            cand.append((score, pct, why, b, tr))
        cand.sort(key=lambda x: (-x[0], abs(x[1])))
        out.append((a, cand[:top]))
    return out


def write_plan(plan, path):
    with open(path, "w", newline="", encoding="utf-8-sig") as f:
        w = csv.writer(f)
        w.writerow(["from", "out cue", "out at", "bars of blend", "to", "in cue", "in at",
                    "from bpm", "to bpm", "pitch", "from key", "to key", "score", "why"])
        for a, cands in plan:
            for score, pct, why, b, tr in cands:
                w.writerow([a["title"], tr["out_letter"], fmt_time(tr["out_time"]), f"{tr['bars']:.0f}",
                            b["title"], tr["in_letter"], fmt_time(tr["in_time"]),
                            f"{a['bpm']:.1f}" if a["bpm"] else "", f"{b['bpm']:.1f}" if b["bpm"] else "",
                            f"{pct:+.1f}%", a["key"] or "", b["key"] or "", score, why])
    return sum(len(c) for _, c in plan)


def build_set(tracks, start=None):
    """Greedy chain: from each track step to the best-scoring unplayed one. Not optimal, but it
    gives a playable order and shows where the library runs out of good moves."""
    pool = dict(tracks)
    if start:
        first = next((n for n, t in pool.items() if start.lower() in t["stem"].lower()), None)
        if first is None:
            sys.exit(f"no track matching {start!r}")
    else:   # start slow
        first = min(pool, key=lambda n: pool[n]["bpm"] or 999)
    order, cur = [(pool.pop(first), None, None, "")], tracks[first]
    while pool:
        best = None
        for b in pool.values():
            score, why, pct = score_pair(cur, b)
            tr = transition(cur, b) if score else None
            if not tr:
                continue
            # tie-break on the real tempo step, not the pitch-matched one, so the set drifts
            # through the BPM range instead of jumping octaves
            drift = abs((b.get("bpm") or 0) - (cur.get("bpm") or 0))
            rank = (score, -drift)
            if best is None or rank > (best[0], -best[5]):
                best = (score, pct, why, b, tr, drift)
        if best is None:
            break   # nothing left mixes out of here
        score, pct, why, b, tr, _ = best
        order.append((b, tr, score, why))
        cur = pool.pop(b["name"])
    return order, list(pool.values())


def write_set(order, path):
    with open(path, "w", newline="", encoding="utf-8-sig") as f:
        w = csv.writer(f)
        w.writerow(["#", "track", "bpm", "key", "mix in at", "mix out of previous at", "score", "why"])
        for i, (t, tr, score, why) in enumerate(order, 1):
            w.writerow([i, t["title"], f"{t['bpm']:.1f}" if t["bpm"] else "", t["key"] or "",
                        f"{tr['in_letter']} {fmt_time(tr['in_time'])}" if tr else "",
                        f"{tr['out_letter']} {fmt_time(tr['out_time'])}" if tr else "",
                        score or "", why])


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--analysis", default="analysis_rb.json")
    ap.add_argument("--top", type=int, default=5, help="transitions to list per track")
    ap.add_argument("--from", dest="only", help="only show options out of tracks matching this")
    ap.add_argument("--out", default="mixplan.csv")
    ap.add_argument("--set", action="store_true", help="also build one greedy set order")
    ap.add_argument("--start", help="track to start the set on (default: slowest)")
    ap.add_argument("--set-out", default="mixset.csv")
    ap.add_argument("--write-playlist", metavar="NAME",
                    help="write the set order into rekordbox as a playlist (rekordbox must be closed)")
    ap.add_argument("--no-db", action="store_true", help="skip rekordbox (no keys - BPM only)")
    args = ap.parse_args()

    db = None
    if not args.no_db:
        from pyrekordbox import Rekordbox6Database
        from pyrekordbox.utils import get_rekordbox_pid
        if get_rekordbox_pid() and args.write_playlist:
            sys.exit("rekordbox is running - close it to write a playlist")
        db = Rekordbox6Database()

    tracks = load_tracks(args.analysis, db)
    keyed = sum(1 for t in tracks.values() if t["key"])
    mixable = sum(1 for t in tracks.values() if out_cue(t)[1] is not None)
    print(f"{len(tracks)} tracks, {keyed} with a key, {mixable} with a mix-out cue (F or E)")

    plan = pairs(tracks, args.top, args.only)
    n = write_plan(plan, args.out)
    dead = [a["title"] for a, c in plan if not c]
    print(f"{n} transitions -> {args.out}")
    if dead:
        print(f"{len(dead)} track(s) with no good follow-on: " + ", ".join(d[:28] for d in dead[:6]))

    if args.set or args.write_playlist:
        order, left = build_set(tracks, args.start)
        write_set(order, args.set_out)
        print(f"set of {len(order)} tracks -> {args.set_out}" + (f" ({len(left)} left over)" if left else ""))
        if args.write_playlist:
            import datetime as dt
            import shutil
            src = Path(db.engine.url.database)          # back up first, as autocue_rb.py does
            bak = src.with_name(f"{src.name}.mixplan-{dt.datetime.now():%Y%m%d-%H%M%S}.bak")
            shutil.copy2(src, bak)
            for suffix in ("-wal", "-shm"):
                side = src.with_name(src.name + suffix)
                if side.exists():
                    shutil.copy2(side, bak.with_name(bak.name + suffix))
            print("backup:", bak)
            if any(p.Name == args.write_playlist for p in db.get_playlist()):
                sys.exit(f"playlist {args.write_playlist!r} already exists - delete it or pick another name")
            pl = db.create_playlist(args.write_playlist)
            for i, (t, *_) in enumerate(order, 1):
                if t["content_id"]:
                    db.add_to_playlist(pl, t["content_id"], track_no=i)
            db.commit()
            print(f"wrote rekordbox playlist {args.write_playlist!r} ({len(order)} tracks, in mix order)")


if __name__ == "__main__":
    main()
