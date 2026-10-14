#!/usr/bin/env python
"""
autocue_rb.py - write A-F memory cues straight into the rekordbox database.

Runs on the machine that has rekordbox (Windows/macOS). Uses rekordbox's own
beat grid and phrase analysis (from the ANLZ files), detects drop / breakdown /
outro from the audio (autocue_core), and writes memory cues into master.db the
way CueGen does: backup first, cues tagged with a UUID prefix so re-runs only
replace autocue's own cues, colours per cue letter.

Requirements:
  - rekordbox 6/7 must be CLOSED while this runs
  - tracks must already be imported and analysed in rekordbox (grid + phrases)
  - pip install pyrekordbox numpy scipy soundfile     (ffmpeg on PATH for m4a)
  - first time only:  python -m pyrekordbox download-key

Usage:
  autocue_rb.py --playlist "2025 Top DJ Lineup"          # tracks in a rekordbox playlist
  autocue_rb.py --folder "%USERPROFILE%/Music/library"  # tracks under a folder
  autocue_rb.py --all
  options: --dry-run  --prefer-phrases  --replace-all  --no-backup  --json analysis.json
"""
import argparse
import atexit
import datetime as dt
import json
import logging
import secrets
import shutil
import sys
import time
from pathlib import Path

import numpy as np

from autocue_core import (CUE_SCHEME, HOT_SLOT, UUID_PREFIX, analyze_with_grid, derive_cues, fmt_time, merge_llm,
                          order_sections, print_report, rebase_on_drop, track_comment, vote_sections, write_review)

# pyrekordbox warns once per file about ANLZ tags it doesn't parse (PVB2 etc.); not our problem
logging.getLogger("pyrekordbox.anlz.file").setLevel(logging.ERROR)

MAX_MEMORY_CUES = 10   # rekordbox limit per track

# PSSI phrase kinds
MOOD = {1: "High", 2: "Mid", 3: "Low"}
KIND_HIGH = {1: "Intro", 2: "Up", 3: "Down", 5: "Chorus", 6: "Outro"}
KIND_MIDLOW = {1: "Intro", 2: "Verse", 3: "Verse", 4: "Verse", 5: "Verse", 6: "Verse", 7: "Verse",
               8: "Bridge", 9: "Chorus", 10: "Outro"}


# --- rekordbox data --------------------------------------------------------
def read_grid_and_phrases(db, content):
    """Return (downbeat_times, beat_times, phrases, mood) from the track's ANLZ files.

    phrases: list of (label, start_time_s) in order, or [] if no phrase analysis.
    """
    files = db.read_anlz_files(content)
    grid = None
    phrases = []
    for path, anlz in files.items():
        if "PQTZ" in anlz.tag_types and grid is None:
            grid = anlz.get_tag("PQTZ")
        if "PSSI" in anlz.tag_types and not phrases:
            pssi = anlz.get_tag("PSSI")
            phrases = pssi
    if grid is None:
        raise RuntimeError("no beat grid (PQTZ) - analyse the track in rekordbox first")
    beats, bpms, times = grid.get()
    downbeats = times[beats == 1]
    out = []
    mood = ""
    if phrases:
        c = phrases.content
        mood = MOOD.get(int(c.mood), "")
        names = KIND_HIGH if c.mood == 1 else KIND_MIDLOW
        for e in c.entries:
            b = int(e.beat) - 1          # phrase 'beat' is 1-based into the grid
            if 0 <= b < len(times):
                out.append((names.get(int(e.kind), f"kind{e.kind}"), float(times[b])))
        out.append(("END", float(times[min(int(c.end_beat) - 1, len(times) - 1)])))
    return downbeats, times, out, mood


def nearest_bar(t, downbeats):
    return int(np.argmin(np.abs(np.asarray(downbeats) - t)))


def phrase_sections(phrases, downbeats):
    """rekordbox's view of drop / breakdown / outro as bar indices (or None)."""
    if not phrases:
        return None
    labels = [p[0] for p in phrases]
    drop = breakdown = outro = None
    for i, (lab, t) in enumerate(phrases):
        if lab == "Chorus" and drop is None:
            drop = nearest_bar(t, downbeats)
        elif drop is not None and breakdown is None and lab in ("Down", "Verse", "Bridge", "Intro"):
            breakdown = nearest_bar(t, downbeats)
        if lab == "Outro":
            outro = nearest_bar(t, downbeats)
            break
    return {"drop": drop, "breakdown": breakdown, "outro": outro}


def merge_with_phrases(r, rb, downbeats, prefer_phrases, fill=True):
    """Cross-check energy-based sections against rekordbox phrases; optionally prefer them."""
    if rb is None:
        r["flags"].append("no rekordbox phrase analysis")
        return
    changed = False
    for key in ("drop", "breakdown", "outro"):
        mine, theirs = r["sections"].get(key), rb.get(key)
        if theirs is None:
            continue
        if mine is None and not fill:
            continue
        if mine is None:
            # fill a gap the energy analysis left (same policy as merge_llm); a later
            # second-opinion file can still override it
            r["sections"][key] = theirs
            r["flags"] = [f for f in r["flags"] if not f.startswith(f"no {key}")]
            r["flags"].append(f"{key} from rekordbox phrases ({fmt_time(downbeats[theirs])})")
            changed = True
        elif abs(mine - theirs) > 2:
            r["flags"].append(f"rekordbox phrases put {key} at bar {theirs} ({fmt_time(downbeats[theirs])})"
                              + (" - used" if prefer_phrases else ""))
            if prefer_phrases:
                r["sections"][key] = theirs
                changed = True
                if key == "drop":
                    rebase_on_drop(r, np.asarray(r["energy"]), r.get("energy_hi"))
    if changed:
        order_sections(r)
        r["cues"] = derive_cues(r["sections"], downbeats, duration=r.get("duration"))


# --- database write --------------------------------------------------------
def make_uuid():
    tail = secrets.token_hex(8)
    return f"{UUID_PREFIX}{tail[:4]}-{tail[4:]}"


def is_hot_cue(c):
    """djmdCue.Kind: 0 = memory cue, 1..8 = hot cue A..H, 4 = memory loop (has an out point)."""
    return 1 <= (c.Kind or 0) <= 8 and not (c.Kind == 4 and (c.OutMsec or -1) >= 0)


def write_cues(db, content, cues, replace_all=False, dry_run=False, bpm=128.0, hot_cues=False):
    """Replace autocue's cues on this track. Returns (deleted, added, skipped_reason).

    Memory cues (default): Kind 0, loops Kind 4, max 10 per track.
    --hot-cues: the cues go to hot cue slots A..H in letter order (Kind = slot number),
    existing hot cues in those slots are only replaced if they are autocue's or --replace-all.
    """
    from pyrekordbox.db6 import tables
    existing = list(db.get_cue(ContentID=content.ID))
    ours = [c for c in existing if (c.UUID or "").startswith(UUID_PREFIX)]
    if hot_cues:
        cues = [c for c in cues if c[0] in HOT_SLOT]   # fixed letter -> slot mapping; I (intro loop) has no slot
        slots = [HOT_SLOT[c[0]] for c in cues]
        ours_hot = [c for c in ours if is_hot_cue(c)]        # memory cues from a previous run are left alone
        if replace_all:
            to_delete = ours_hot + [c for c in existing if c not in ours and is_hot_cue(c) and c.Kind in slots]
        else:
            # hand-placed hot cues stay; our cue for that slot is dropped
            taken = {c.Kind for c in existing if c not in ours and is_hot_cue(c)}
            to_delete = ours_hot
            cues = [c for c in cues if HOT_SLOT[c[0]] not in taken]
    else:
        to_delete = [c for c in existing
                     if not is_hot_cue(c) and (replace_all or (c.UUID or "").startswith(UUID_PREFIX))]
        remaining = [c for c in existing if not is_hot_cue(c) and c not in to_delete]
        for optional in ("I", "L", "G"):        # shed the extras first when hand-placed cues eat the 10 slots
            if len(remaining) + len(cues) > MAX_MEMORY_CUES:
                cues = [c for c in cues if c[0] != optional]
        if len(remaining) + len(cues) > MAX_MEMORY_CUES:
            return 0, 0, f"would exceed {MAX_MEMORY_CUES} memory cues ({len(remaining)} manual ones exist)"
    if dry_run:
        return len(to_delete), len(cues), None
    for c in to_delete:
        db.delete(c)
    now = dt.datetime.now()
    for letter, name, t, end in cues:
        msec = int(round(t * 1000))
        loop = end is not None
        out_msec = int(round(end * 1000)) if loop else -1
        beats = int(round((end - t) / (60.0 / max(bpm, 1)))) if loop else 0
        kind = HOT_SLOT[letter] if hot_cues else (4 if loop else 0)
        cue = tables.DjmdCue(
            ID=str(db.generate_unused_id(tables.DjmdCue)),
            ContentID=content.ID,
            InMsec=msec, InFrame=int(msec * 150 / 1000), InMpegFrame=0, InMpegAbs=0,
            OutMsec=out_msec, OutFrame=int(out_msec * 150 / 1000) if loop else 0, OutMpegFrame=0, OutMpegAbs=0,
            Kind=kind, Color=CUE_SCHEME[letter][1], ColorTableIndex=None,
            ActiveLoop=0, Comment=name, BeatLoopSize=(0x10000 * beats + 1) if loop else 0, CueMicrosec=0,
            ContentUUID=content.UUID, UUID=make_uuid(),
            created_at=now, updated_at=now,
        )
        db.add(cue)
    return len(to_delete), len(cues), None


def open_snapshot():
    """Copy master.db (+wal/shm) to a temp dir and open that copy read-only-ish, so a
    --dry-run can run while rekordbox is open."""
    import tempfile
    from pyrekordbox import Rekordbox6Database
    from pyrekordbox.config import get_config
    src = None
    for section in ("rekordbox7", "rekordbox6"):
        try:
            src = Path(get_config(section, "db_path"))
            break
        except Exception:
            continue
    if src is None or not src.exists():
        sys.exit("could not locate master.db")
    for old in Path(tempfile.gettempdir()).glob("autocue-snapshot-*"):
        shutil.rmtree(old, ignore_errors=True)   # leftovers from interrupted runs
    tmp = Path(tempfile.mkdtemp(prefix="autocue-snapshot-"))
    dst = tmp / src.name
    shutil.copy2(src, dst)
    for suffix in ("-wal", "-shm"):
        side = src.with_name(src.name + suffix)
        if side.exists():
            shutil.copy2(side, dst.with_name(dst.name + suffix))
    print(f"rekordbox is running - dry run against a snapshot ({dst})")
    db = Rekordbox6Database(path=dst, db_dir=src.parent)

    def cleanup():
        try:
            db.close()
        finally:
            shutil.rmtree(tmp, ignore_errors=True)
    atexit.register(cleanup)
    return db


def backup_db(db):
    src = Path(db.engine.url.database)
    stamp = dt.datetime.now().strftime("%Y%m%d-%H%M%S")
    dst = src.with_name(f"{src.name}.autocue-{stamp}.bak")
    shutil.copy2(src, dst)
    for suffix in ("-wal", "-shm"):
        side = src.with_name(src.name + suffix)
        if side.exists():
            shutil.copy2(side, dst.with_name(dst.name + suffix))
    return dst


# --- main ------------------------------------------------------------------
def select_contents(db, args):
    from pyrekordbox.db6 import tables
    if args.playlist:
        pls = list(db.get_playlist(Name=args.playlist))
        if not pls:
            sys.exit(f"playlist '{args.playlist}' not found")
        songs = db.get_playlist_songs(PlaylistID=pls[0].ID)
        return [s.Content for s in songs]
    if args.folder:
        folder = args.folder.replace("\\", "/").rstrip("/") + "/"
        return [c for c in db.get_content() if (c.FolderPath or "").replace("\\", "/").startswith(folder)]
    if args.files:
        wanted = {Path(f).name for f in args.files}
        return [c for c in db.get_content() if Path(c.FolderPath or "").name in wanted]
    if args.all:
        return list(db.get_content())
    sys.exit("select tracks with --playlist, --folder, --all or file names")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sel = ap.add_mutually_exclusive_group()
    sel.add_argument("--playlist", help="rekordbox playlist name")
    sel.add_argument("--folder", help="only tracks whose file is under this folder")
    sel.add_argument("--all", action="store_true")
    ap.add_argument("files", nargs="*", help="audio file names to match against the collection")
    ap.add_argument("--dry-run", action="store_true", help="analyse and report, write nothing")
    ap.add_argument("--prefer-phrases", action="store_true",
                    help="where rekordbox phrase analysis disagrees, use its sections instead of the energy analysis")
    ap.add_argument("--replace-all", action="store_true",
                    help="delete ALL existing memory cues on the track, not just autocue's own")
    ap.add_argument("--no-backup", action="store_true")
    ap.add_argument("--db", help="path to master.db (default: auto-detect)")
    ap.add_argument("--plain", action="store_true", help="--db is an unencrypted sqlite copy (testing)")
    ap.add_argument("--json", help="dump analysis to this JSON file")
    ap.add_argument("--gemini", action="append", default=[], help="gemini.json from gemini_check.py (repeat for a second opinion -> majority vote)")
    ap.add_argument("--prefer-gemini", action="store_true", help="on disagreement use Gemini's position")
    ap.add_argument("--vote", action="store_true",
                    help="majority vote between the energy analysis, the --gemini file(s) and rekordbox's phrase analysis "
                         "(ties: energy, then the first --gemini file); flags tracks where nobody agrees")
    ap.add_argument("--second-drop", action="store_true", help="also cue G = second drop")
    ap.add_argument("--end-loop", type=int, default=0, metavar="BARS", help="add an N-bar memory loop on the last phrase")
    ap.add_argument("--intro-loop", type=int, default=0, metavar="BARS",
                    help="add an N-bar loop on the first N bars (cue I) for mixing in over short intros")
    ap.add_argument("--hot-cues", action="store_true",
                    help="write hot cues A-H instead of memory cues (A..G by letter, end loop = H; intro loop not written)")
    ap.add_argument("--write-comments", action="store_true",
                    help="put energy/genre/vocals/flags into the track's Comments field in rekordbox")
    ap.add_argument("--review", default="review.csv", help="review CSV path")
    args = ap.parse_args()

    from pyrekordbox import Rekordbox6Database
    from pyrekordbox.utils import get_rekordbox_pid
    if get_rekordbox_pid() and not args.dry_run:
        sys.exit("rekordbox is running - close it first")

    if args.db:
        db = Rekordbox6Database(path=args.db, unlock=not args.plain)
    elif get_rekordbox_pid():
        # dry run while rekordbox is open: analyse a snapshot of the database (ANLZ files
        # are still read from the real share/ folder); nothing can be written this way
        db = open_snapshot()
    else:
        db = Rekordbox6Database()
    llms = []   # keyed by file name so a gemini.json made on another machine still matches
    for g in args.gemini:
        llms.append({Path(k).name: v for k, v in json.loads(Path(g).read_text()).items()})
    contents = [c for c in select_contents(db, args) if c is not None]
    print(f"{len(contents)} track(s) selected")

    if not args.dry_run and not args.no_backup:
        print("backup:", backup_db(db))

    results, pending = [], 0
    for i, c in enumerate(contents, 1):
        path = Path(c.FolderPath or "")
        print(f"[{i}/{len(contents)}] {path.name}", end=" ... ", flush=True)
        if not path.exists():
            print("FILE MISSING")
            continue
        t0 = time.time()
        try:
            downbeats, beat_times, phrases, mood = read_grid_and_phrases(db, c)
            rb = phrase_sections(phrases, downbeats)
            # rekordbox's bar 0 is just its first beat, not a phrase boundary; only its phrase
            # analysis (first Chorus) tells us where the 4-bar grid really sits
            anchor = rb["drop"] if rb and rb.get("drop") is not None else None
            r = analyze_with_grid(path, downbeats, phrase_anchor=anchor)
            r["bpm"] = (c.BPM or 0) / 100
            r["mood"] = mood
            r["rekordbox_sections"] = rb
            merge_with_phrases(r, rb, downbeats, args.prefer_phrases, fill=not args.vote)
            energy_drop = r["sections"].get("drop")
            opts = dict(second_drop=args.second_drop, end_loop_bars=args.end_loop, intro_loop_bars=args.intro_loop)
            opinions = [l.get(path.name) for l in llms]
            if args.vote and rb:
                opinions.append({**{k: v for k, v in rb.items() if v is not None}, "source_name": "rb-phrases"})
            if len(opinions) >= 2:
                # ties (nobody agrees): the first --gemini file if --prefer-gemini, else energy
                vote_sections(r, opinions, downbeats, tie_pref=("llm1", "energy") if args.prefer_gemini else ("energy",))
                r["cues"] = derive_cues(r["sections"], downbeats, duration=r["duration"], **opts)
            elif llms:
                merge_llm(r, llms[0].get(path.name), downbeats, args.prefer_gemini, **opts)
            else:
                r["cues"] = derive_cues(r["sections"], downbeats, duration=r["duration"], **opts)
            # the read overrode a drop that the energy analysis and rekordbox's phrases agree on:
            # two sources against one - worth a listen (marks the track CHECK)
            final_drop, rb_drop = r["sections"].get("drop"), (rb or {}).get("drop")
            if None not in (energy_drop, rb_drop, final_drop) and abs(energy_drop - rb_drop) <= 2 \
                    and abs(final_drop - energy_drop) > 2:
                r["flags"].append(f"drop: 2/3 sources (energy + phrases) say {fmt_time(downbeats[energy_drop])}, "
                                  f"read's {fmt_time(downbeats[final_drop])} used")
            deleted, added, skip = write_cues(db, c, r["cues"], args.replace_all, args.dry_run, bpm=r["bpm"] or 128.0,
                                              hot_cues=args.hot_cues)
            if args.write_comments and not args.dry_run:
                c.Commnt = track_comment(r)[:255]
        except Exception as e:
            print(f"FAILED: {e}")
            continue
        if skip:
            r["flags"].append("NOT WRITTEN: " + skip)
        print(f"{r['bpm']:.1f} bpm, {r['bars']} bars, -{deleted}/+{added} cues, {time.time() - t0:.1f}s"
              + (f"  [{'; '.join(r['flags'])}]" if r["flags"] else ""))
        results.append(r)
        pending += added

    if args.json:
        Path(args.json).write_text(json.dumps(results, indent=1, default=str))
    print_report(results)
    if results:
        n = write_review(results, args.review)
        print(f"review list: {args.review} ({n} tracks marked CHECK)")

    if args.dry_run:
        print("\ndry run - nothing written")
    elif pending:
        db.commit()
        print(f"\ncommitted {pending} cues on {len(results)} tracks - start rekordbox to see them")
    else:
        print("\nnothing to write")


if __name__ == "__main__":
    main()
