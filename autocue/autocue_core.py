"""
autocue_core.py - shared section detection for 4/4 dance tracks.

Given a bar grid (downbeat times) and the audio, find:
  drop      = earliest large, sustained energy jump
  breakdown = first sustained energy drop after the drop (relative to drop level)
  outro     = last sustained energy drop with no recovery

and derive the six-cue scheme:
  A  first downbeat
  B  32 bars before first drop
  C  16 bars before first drop
  D  first drop
  E  first breakdown after the drop
  F  start of outro
"""
import shutil
import subprocess
from pathlib import Path

import numpy as np
from scipy.signal import butter, sosfiltfilt

# --- tunables -------------------------------------------------------------
DROP_MIN_JUMP_DB = 4.0     # minimum energy jump (pre 4 bars -> post 8 bars) to count as a drop
DROP_REL_TO_MAX = 0.3      # earliest candidate with jump >= this * biggest jump wins
DROP_SEARCH_FRAC = 0.7     # only look for the first drop in the first 70 % of the track
BREAK_MIN_DROP_DB = 4.0    # sustained energy loss to count as breakdown / outro
HIGH_PCTL = 90             # "high energy" reference level percentile
BASS_WEIGHT = 0.65         # bass band vs full band in the combined energy curve

# rekordbox memory-cue colour indices: 0 Pink 1 Red 2 Orange 3 Yellow 4 Green 5 Aqua 6 Blue 7 Purple
CUE_SCHEME = {
    # letter: (name, memory colour index, rgb for xml)
    "A": ("A intro",     4, (40, 226, 20)),
    "B": ("B drop -32",  3, (255, 213, 0)),
    "C": ("C drop -16",  2, (255, 140, 0)),
    "D": ("D drop",      1, (224, 0, 0)),
    "E": ("E breakdown", 6, (48, 90, 255)),
    "F": ("F outro",     7, (160, 48, 255)),
    "G": ("G drop 2",    0, (255, 120, 200)),   # optional (--second-drop)
    "L": ("L end loop",  5, (0, 200, 200)),     # optional (--end-loop N), a memory loop
    "I": ("I intro loop", 4, (40, 226, 20)),    # optional (--intro-loop N), a loop on the first N bars
}
# --hot-cues: cue letter -> hot cue slot (djmdCue.Kind 1..8 = hot cue A..H); the end loop takes slot H
HOT_SLOT = {"A": 1, "B": 2, "C": 3, "D": 4, "E": 5, "F": 6, "G": 7, "L": 8}
UUID_PREFIX = "a0c0e5e5-a070-4cfe-"   # marks cues written by autocue (so re-runs can replace them)


# --- audio ----------------------------------------------------------------
def load_audio(path):
    """Return (mono float32 array, sample rate). Uses soundfile, falls back to ffmpeg."""
    try:
        import soundfile as sf
        y, sr = sf.read(str(path), dtype="float32", always_2d=True)
        return y.mean(axis=1), sr
    except Exception as e:  # e.g. m4a/aac which libsndfile cannot decode
        if not shutil.which("ffmpeg"):
            raise RuntimeError(f"cannot decode {path} ({e}); install ffmpeg for m4a/aac support")
        sr = 44100
        raw = subprocess.run(
            ["ffmpeg", "-v", "error", "-i", str(path), "-f", "f32le", "-ac", "1", "-ar", str(sr), "-"],
            capture_output=True, check=True).stdout
        return np.frombuffer(raw, dtype=np.float32), sr


def bar_energy(path, downbeats):
    """Per-bar energy curves in dB. Returns (combined bass-weighted, mid/high band, duration_s)."""
    y, sr = load_audio(path)
    sos = butter(4, [20, 150], btype="band", fs=sr, output="sos")
    bass = sosfiltfilt(sos, y)
    sos_hi = butter(4, [500, min(8000, sr / 2 - 100)], btype="band", fs=sr, output="sos")
    high = sosfiltfilt(sos_hi, y)

    def rms_db(sig, a, b):
        seg = sig[a:b]
        if len(seg) == 0:
            return -80.0
        return 20 * np.log10(np.sqrt(np.mean(seg ** 2)) + 1e-9)

    edges = (np.asarray(downbeats) * sr).astype(int)
    edges = np.append(edges, len(y))
    full = np.array([rms_db(y, edges[i], edges[i + 1]) for i in range(len(downbeats))])
    low = np.array([rms_db(bass, edges[i], edges[i + 1]) for i in range(len(downbeats))])
    hi = np.array([rms_db(high, edges[i], edges[i + 1]) for i in range(len(downbeats))])
    E = BASS_WEIGHT * low + (1 - BASS_WEIGHT) * full
    duration = len(y) / sr
    bar_len = np.median(np.diff(downbeats)) if len(downbeats) > 1 else 2.0
    if duration - downbeats[-1] < 0.5 * bar_len:   # trailing partial bar
        E, hi = E[:-1], hi[:-1]
    return E, hi, duration


# --- section detection ----------------------------------------------------
def find_sections(E, E_hi=None, drop_min_jump=None, drop_rel=None, break_min=None, high_pctl=None, drop=None,
                  phrase_anchor=0):
    """Return dict with bar indices for drop / breakdown / outro plus flags.

    E     : combined (bass-weighted) energy per bar, dB
    E_hi  : optional mid/high-band energy per bar, dB - used to find outros where
            the kick keeps going but the vocal/lead leaves
    drop  : if given, skip the drop search and place breakdown / outro / second drop
            relative to this bar (used when a second opinion overrides the drop)
    phrase_anchor : a bar index known to sit on a 4-bar phrase boundary; the drop is
            snapped (+-1 bar) onto that 4-bar grid. 0 is right for madmom's grid (its
            first downbeat is phrase-aligned on 70/97 tracks vs 63 unsnapped); pass
            None for rekordbox's grid, whose bar 0 is wherever the first beat fell
            (snapping to it made things worse: 51 vs 61 agreeing with the reads).
    """
    drop_min_jump = DROP_MIN_JUMP_DB if drop_min_jump is None else drop_min_jump
    drop_rel = DROP_REL_TO_MAX if drop_rel is None else drop_rel
    break_min = BREAK_MIN_DROP_DB if break_min is None else break_min
    high_pctl = HIGH_PCTL if high_pctl is None else high_pctl
    E = np.asarray(E)
    n = len(E)
    flags = []
    hi = np.percentile(E, high_pctl)

    def med(a, b):
        a, b = max(0, a), min(n, b)
        return float(np.median(E[a:b])) if b > a else -80.0

    def refine(i, lo, hi_):
        """Snap to the bar with the sharpest single-bar rise in [i+lo, i+hi_]."""
        ks = [k for k in range(i + lo, i + hi_ + 1) if 1 <= k < n]
        return max(ks, key=lambda k: E[k] - E[k - 1]) if ks else i

    def refine_down(i, lo, hi_):
        ks = [k for k in range(i + lo, i + hi_ + 1) if 1 <= k < n]
        return max(ks, key=lambda k: E[k - 1] - E[k]) if ks else i

    def snap(i, anchor, tol):
        """Snap bar i to the 4-bar phrase grid counted from `anchor` if within tol bars."""
        near = anchor + 4 * round((i - anchor) / 4)
        return near if abs(near - i) <= tol and 0 <= near < n else i

    # --- first drop: sustained jump (median of 4 bars before vs 8 after)
    cands = []
    if drop is None:
        for i in range(4, int(n * DROP_SEARCH_FRAC)):
            post, pre = med(i, i + 8), med(i - 4, i)
            jump = post - pre
            if jump >= drop_min_jump and post >= hi - 4 and med(i, i + 4) >= hi - 4:
                cands.append((i, jump))
    drop_jump = 0.0
    if drop is not None:
        pass   # fixed by the caller
    elif cands:
        best = max(j for _, j in cands)
        for i, j in cands:
            if j >= drop_rel * best:
                drop = refine(i, -2, 5)
                if phrase_anchor is not None:
                    snapped = snap(drop, phrase_anchor, 1)
                    if snapped != drop:
                        flags.append(f"drop snapped {drop}->{snapped}")
                        drop = snapped
                    elif (drop - phrase_anchor) % 4:
                        flags.append("drop not on 4-bar phrase - check grid")
                drop_jump = j
                break
        if drop_jump < 8:
            flags.append(f"weak drop (+{drop_jump:.1f} dB)")
    else:
        # track may start hot (radio edit) - treat bar 0 as the drop if energy is high
        if med(0, 8) >= hi - 4:
            drop = 0
            flags.append("starts at full energy; no intro")
        else:
            flags.append("no drop found")

    # --- first breakdown after the drop: sustained loss for >= 8 bars, judged against the drop level
    breakdown = None
    if drop is not None:
        lvl = med(drop, drop + 8) - break_min
        for j in range(drop + 8, n - 8):
            if med(j - 4, j) - med(j, j + 8) >= break_min and med(j, j + 8) < lvl and med(j, j + 4) < lvl:
                breakdown = snap(refine_down(j, -2, 5), drop, 2)
                break
        if breakdown is None:
            flags.append("no breakdown found")

    # --- outro: last sustained drop with no recovery afterwards, on the combined curve
    #     or (kick keeps going, vocal/lead leaves) on the mid/high curve
    def last_drop(X, min_db):
        X = np.asarray(X)
        x_hi = np.percentile(X, high_pctl)
        for k in range(len(X) - 4, 8, -1):
            pre, post = float(np.median(X[k - 4:k])), float(np.median(X[k:]))
            if pre - post >= min_db and post < x_hi - 5 and np.percentile(X[k:], 75) < x_hi - 4:
                ks = [q for q in range(k - 2, k + 6) if 1 <= q < len(X)]
                return max(ks, key=lambda q: X[q - 1] - X[q])
        return None

    outro = last_drop(E, break_min)
    if outro is None and E_hi is not None:
        outro = last_drop(E_hi, break_min + 1)
        if outro is not None:
            flags.append("outro from mid/high band (layers leave, kick stays)")
    if outro is not None:
        outro = snap(outro, drop if drop is not None else 0, 2)
    if outro is None:
        flags.append("no outro found (hard ending?)")
    elif breakdown is not None and outro <= breakdown:
        outro = None
        flags.append("breakdown runs to end (E = outro)")

    # --- second drop: next sustained jump after the breakdown
    second = None
    if breakdown is not None:
        for i in range(breakdown + 4, n - 4):
            if med(i, i + 8) - med(i - 4, i) >= drop_min_jump and med(i, i + 4) >= hi - 4:
                second = snap(refine(i, -2, 5), drop, 2)
                break

    if drop is not None and drop > n * 0.5:
        flags.append("drop is late in track - check")
    if drop is not None and 0 < drop < 16:
        flags.append(f"short intro ({drop} bars)")

    return {"drop": drop, "breakdown": breakdown, "outro": outro, "second_drop": second, "flags": flags}


def derive_cues(sections, downbeats, second_drop=False, end_loop_bars=0, duration=None, intro_loop_bars=0):
    """Map detected sections to cues: [(letter, name, start_s, end_s_or_None), ...]."""
    downbeats = np.asarray(downbeats, dtype=float)
    n = len(downbeats)

    def t(i):
        return float(downbeats[i]) if i is not None and 0 <= i < n else None

    def cue(letter, i):
        return (letter, CUE_SCHEME[letter][0], t(i), None)

    cues = [cue("A", 0)]
    d = sections.get("drop")
    if intro_loop_bars and n > intro_loop_bars and (d is None or d >= intro_loop_bars):
        # loop over the first N bars, for mixing in over a short intro (skipped if the drop is inside it)
        bar_len = float(np.median(np.diff(downbeats)))
        cues.append(("I", CUE_SCHEME["I"][0], t(0), t(0) + intro_loop_bars * bar_len))
    if d is not None:
        if d - 32 > 0:
            cues.append(cue("B", d - 32))
        if d - 16 > 0:
            cues.append(cue("C", d - 16))
        cues.append(cue("D", d))
    if sections.get("breakdown") is not None:
        cues.append(cue("E", sections["breakdown"]))
    if sections.get("outro") is not None:
        cues.append(cue("F", sections["outro"]))
    if second_drop and sections.get("second_drop") is not None:
        cues.append(cue("G", sections["second_drop"]))
    if end_loop_bars and n > end_loop_bars + 1:
        # last full N-bar phrase aligned to the drop's 4-bar grid. The loop length is
        # exactly N bars (rekordbox stores it as a beat count); if the audio ends before
        # the last gridded phrase completes, step back a phrase so the loop is complete.
        anchor = d if d is not None else 0
        bar_len = float(np.median(np.diff(downbeats)))
        k = n - end_loop_bars
        k -= (k - anchor) % 4
        while k > anchor and duration is not None and t(k) + end_loop_bars * bar_len > duration + 0.05:
            k -= 4
        if k > anchor:
            cues.append(("L", CUE_SCHEME["L"][0], t(k), t(k) + end_loop_bars * bar_len))
    return [c for c in cues if c[2] is not None]


def norm_cues(cues):
    """Accept older JSON cue formats: (name, t) or (letter, name, t)."""
    out = []
    for c in cues:
        if len(c) == 2:
            out.append((c[0][0], c[0], c[1], None))
        elif len(c) == 3:
            out.append((c[0], c[1], c[2], None))
        else:
            out.append(tuple(c))
    return out


def track_comment(r):
    """One line for rekordbox's Comments column: energy / genre / vocals / flags."""
    x = r.get("extra") or {}
    parts = []
    if x.get("energy"):
        parts.append(f"E{x['energy']}")
    if x.get("genre"):
        parts.append(str(x["genre"]))
    if x.get("vocals_intro"):
        parts.append("vocal intro")
    if x.get("vocals_outro"):
        parts.append("vocal outro")
    parts += r.get("flags", [])
    return " | ".join(parts)


# a track needs a listen when the DROP is in doubt or the grid is: sources disagree on the drop,
# no drop found, the second opinion was unsure, tempo unstable / double-half-time. Nudges on
# E/F/G and cosmetic flags stay visible in the flags column but don't mark the track CHECK.
CHECK_WORDS = ("LLM puts drop", "drop: no agreement", "drop: 2/3", "no drop", "no LLM", "BPM",
               "confidence low", "tempo unstable", "late in track")


def review_status(r):
    return "CHECK" if any(w in f for f in r.get("flags", []) for w in CHECK_WORDS) else "OK"


def write_review(results, path):
    """CSV sorted with the tracks that need a listen at the top."""
    import csv
    rows = []
    for r in results:
        c = {x[0]: x[2] for x in norm_cues(r["cues"])}
        x = r.get("extra") or {}
        rows.append({
            "status": review_status(r),
            "track": Path(r["path"]).stem,
            "bpm": r.get("bpm", ""),
            **{k: (fmt_time(c[k]) if c.get(k) is not None else "") for k in "ABCDEFGLI"},
            "energy": x.get("energy", ""), "genre": x.get("genre", ""),
            "vocals_intro": x.get("vocals_intro", ""), "vocals_outro": x.get("vocals_outro", ""),
            "flags": "; ".join(r.get("flags", [])),
            "llm_notes": x.get("notes", ""),
        })
    rows.sort(key=lambda z: (z["status"] != "CHECK", z["track"].lower()))
    with open(path, "w", newline="", encoding="utf-8-sig") as f:   # BOM so Excel/PowerShell read the accents
        w = csv.DictWriter(f, fieldnames=list(rows[0].keys()) if rows else ["status"])
        w.writeheader()
        w.writerows(rows)
    return sum(z["status"] == "CHECK" for z in rows)


def analyze_with_grid(path, downbeats, phrase_anchor=0):
    """Full analysis given a bar grid. Returns dict (sections, cues, energy, ...).
    phrase_anchor: see find_sections (0 for madmom grids, None or a known phrase bar for rekordbox's)."""
    downbeats = np.asarray(downbeats, dtype=float)
    E, E_hi, duration = bar_energy(path, downbeats)
    sec = find_sections(E, E_hi, phrase_anchor=phrase_anchor)
    return {
        "path": str(path),
        "duration": duration,
        "bars": int(len(downbeats)),
        "phrase_anchor": phrase_anchor,
        "sections": {k: v for k, v in sec.items() if k != "flags"},
        "flags": sec["flags"],
        "cues": derive_cues(sec, downbeats, duration=duration),
        "energy": [round(float(x), 1) for x in E],
        "energy_hi": [round(float(x), 1) for x in E_hi],
        "downbeats": [round(float(x), 3) for x in downbeats],
    }


def fmt_time(t):
    m, s = divmod(int(t), 60)
    return f"{m}:{s:02d}"


def print_report(results):
    print()
    print(f"{'track':50} {'bpm':>6} {'A':>5} {'B':>5} {'C':>5} {'D':>5} {'E':>5} {'F':>5} {'G':>5}  flags")
    for r in results:
        c = {x[0]: x[2] for x in norm_cues(r["cues"])}
        cols = [fmt_time(c[k]) if c.get(k) is not None else "-" for k in "ABCDEFG"]
        name = Path(r["path"]).stem[:50]
        print(f"{name:50} {r.get('bpm', 0):6.1f} " + " ".join(f"{x:>5}" for x in cols) + "  " + "; ".join(r["flags"]))


# --- second opinion from an audio-capable LLM (see gemini_check.py) ---------
def parse_mmss(v):
    import re
    if v in (None, "", "null"):
        return None
    m = re.match(r"^\s*(\d+):(\d\d(?:\.\d+)?)\s*$", str(v))
    return int(m.group(1)) * 60 + float(m.group(2)) if m else None


def merge_llm(r, llm, downbeats, prefer=False, tol_bars=2, **derive_opts):
    """Cross-check r['sections'] against an LLM's {drop, breakdown, outro} timestamps.

    Fills sections the energy analysis could not place; flags disagreements larger
    than tol_bars; with prefer=True the LLM's position wins. LLM timestamps are
    whole seconds, so they are snapped to the nearest bar and then refined to the
    sharpest energy edge within +-2 bars (best of both: LLM semantics, grid precision).
    """
    if not llm or "error" in llm:
        r["flags"].append("no LLM check" + (f" ({llm['error']})" if llm else ""))
        return
    downbeats = np.asarray(downbeats, dtype=float)
    E = np.asarray(r.get("energy", []), dtype=float)
    n = len(E)

    def refine(i, rising):
        ks = [k for k in range(i - 2, i + 3) if 1 <= k < n]
        if not ks:
            return i
        return max(ks, key=lambda k: (E[k] - E[k - 1]) if rising else (E[k - 1] - E[k]))

    if llm.get("bpm") and r.get("bpm"):
        ratio = float(llm["bpm"]) / r["bpm"]
        if 1.8 < ratio < 2.2 or 0.45 < ratio < 0.55:
            r["flags"].append(f"BPM {r['bpm']:.0f} vs LLM {llm['bpm']} - grid may be double/half time")
    r["extra"] = {k: llm.get(k) for k in ("energy", "genre", "vocals_intro", "vocals_outro", "notes", "confidence")}
    changed = False
    for key in ("drop", "breakdown", "outro", "second_drop"):
        t = parse_mmss(llm.get(key))
        if t is None:
            continue
        theirs = refine(int(np.argmin(np.abs(downbeats - t))), rising=(key in ("drop", "second_drop")))
        mine = r["sections"].get(key)
        if mine is None:
            r["sections"][key] = theirs
            r["flags"] = [f for f in r["flags"] if not f.startswith(f"no {key}")]
            r["flags"].append(f"{key} from LLM ({fmt_time(t)})")
            changed = True
        elif abs(mine - theirs) > tol_bars:
            r["flags"].append(f"LLM puts {key} at {fmt_time(t)} (bar {theirs}, ours {mine})" + (" - used" if prefer else ""))
            if prefer:
                r["sections"][key] = theirs
                changed = True
        if key == "drop" and r["sections"]["drop"] != mine:
            rebase_on_drop(r, E, r.get("energy_hi"))
    if llm.get("confidence") == "low":
        r["flags"].append("LLM confidence low")
    order_sections(r)
    if changed or derive_opts:
        r["cues"] = derive_cues(r["sections"], downbeats, duration=r.get("duration"), **derive_opts)


def rebase_on_drop(r, E, E_hi=None):
    """The drop moved (second opinion won): the energy analysis' breakdown / outro /
    second drop were placed relative to the old drop, so recompute them from the
    curves with the new drop fixed. Later merge steps may still override them."""
    sec = find_sections(E, E_hi, drop=r["sections"]["drop"], phrase_anchor=r.get("phrase_anchor", 0))
    for key in ("breakdown", "outro", "second_drop"):
        r["sections"][key] = sec[key]
    keep = [f for f in r["flags"] if not (f.startswith("no breakdown") or f.startswith("no outro")
                                          or f.startswith("breakdown runs") or f.startswith("outro from mid"))]
    r["flags"] = keep + [f for f in sec["flags"] if f.startswith(("no breakdown", "no outro", "breakdown runs", "outro from mid"))]


def order_sections(r):
    """Drop anything that ends up out of order (E before D, G before E, F before G...)."""
    s = r["sections"]
    d = s.get("drop")
    if d is not None and s.get("breakdown") is not None and s["breakdown"] <= d:
        r["flags"].append(f"breakdown (bar {s['breakdown']}) before drop - dropped")
        s["breakdown"] = None
    if s.get("second_drop") is not None:
        floor = max(x for x in (d, s.get("breakdown"), -1) if x is not None)
        if s["second_drop"] <= floor:
            r["flags"].append(f"second drop (bar {s['second_drop']}) not after breakdown - dropped")
            s["second_drop"] = None
    if s.get("outro") is not None:
        floor = max(x for x in (d, s.get("breakdown"), s.get("second_drop"), -1) if x is not None)
        if s["outro"] <= floor:
            r["flags"].append(f"outro (bar {s['outro']}) not after the other sections - dropped")
            s["outro"] = None


def vote_sections(r, llms, downbeats, tol_bars=2, tie_pref=("energy",)):
    """Majority vote between the energy analysis and several LLM opinions.

    For each of drop / breakdown / outro / second_drop, every source that has a value
    casts a vote (LLM times are bar-snapped and edge-refined; an int is taken as a bar
    index). The value with the most agreeing sources (within tol_bars) wins; ties go to
    the sources named in tie_pref in order (default: the energy analysis), then vote
    order. Fills gaps from any source. Flags 3-way disagreements.
    """
    downbeats = np.asarray(downbeats, dtype=float)
    E = np.asarray(r.get("energy", []), dtype=float)
    n = len(E)

    def refine(i, rising):
        ks = [k for k in range(i - 2, i + 3) if 1 <= k < n]
        return max(ks, key=lambda k: (E[k] - E[k - 1]) if rising else (E[k - 1] - E[k])) if ks else i

    good = [l for l in llms if l and "error" not in l]
    if good:
        r["extra"] = {k: good[0].get(k) for k in ("energy", "genre", "vocals_intro", "vocals_outro", "notes", "confidence")}
    for key in ("drop", "breakdown", "outro", "second_drop"):
        votes = []
        if r["sections"].get(key) is not None:
            votes.append(("energy", r["sections"][key]))
        for j, l in enumerate(good):
            v = l.get(key)
            src = l.get("source_name") or f"llm{j + 1}"
            if isinstance(v, (int, np.integer)):          # already a bar index (e.g. rekordbox phrases)
                votes.append((src, int(v)))
                continue
            t = parse_mmss(v)
            if t is not None:
                votes.append((src, refine(int(np.argmin(np.abs(downbeats - t))), key in ("drop", "second_drop"))))
        if not votes:
            continue
        def rank(v):
            support = sum(abs(v[1] - w[1]) <= tol_bars for w in votes)
            pref = len(tie_pref) - tie_pref.index(v[0]) if v[0] in tie_pref else 0
            return (support, pref, -votes.index(v))
        best = max(votes, key=rank)
        support = sum(abs(best[1] - w[1]) <= tol_bars for w in votes)
        if support < len(votes):
            others = ", ".join(f"{s}={fmt_time(downbeats[b])}" for s, b in votes if abs(b - best[1]) > tol_bars)
            if support == 1:
                r["flags"].append(f"{key}: no agreement ({best[0]} used; {others})")
            else:
                r["flags"].append(f"{key}: {support}/{len(votes)} agree ({best[0]} used; {others})")
        elif best[0] != "energy" and r["sections"].get(key) is None:
            r["flags"] = [f for f in r["flags"] if not f.startswith(f"no {key}")]
            r["flags"].append(f"{key} from {best[0]} ({fmt_time(downbeats[best[1]])})")
        mine = r["sections"].get(key)
        r["sections"][key] = int(best[1])
        if key == "drop" and mine != int(best[1]):
            rebase_on_drop(r, E, r.get("energy_hi"))
    order_sections(r)
