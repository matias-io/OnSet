#!/usr/bin/env python
"""
autocue.py - generate rekordbox memory cues (as rekordbox.xml) for 4/4 dance tracks.

Standalone version: beat grid from madmom, sections from autocue_core.
For the direct-to-rekordbox version (uses rekordbox's own grid and phrase
analysis, writes into master.db) see autocue_rb.py.

Usage:
  autocue.py DIR_OR_FILES... [-o rekordbox.xml] [--prefix C:/Users/me/Music/library] [--json analysis.json]
  autocue.py --from-json analysis.json -o rekordbox.xml --prefix ...
"""
import argparse
import json
import time
import urllib.parse
import warnings
from datetime import date
from pathlib import Path
from xml.sax.saxutils import quoteattr

import numpy as np
import soundfile as sf

from autocue_core import CUE_SCHEME, analyze_with_grid, derive_cues, merge_llm, norm_cues, print_report, track_comment, vote_sections, write_review

warnings.filterwarnings("ignore")

AUDIO_EXT = {".flac", ".mp3", ".m4a", ".wav", ".aiff", ".aif", ".ogg", ".opus"}


def beat_grid(path):
    """Return (downbeat_times, bpm, first_beat_time) using madmom."""
    from madmom.features.downbeats import RNNDownBeatProcessor, DBNDownBeatTrackingProcessor
    act = RNNDownBeatProcessor()(path)
    proc = DBNDownBeatTrackingProcessor(beats_per_bar=[4], fps=100, min_bpm=80, max_bpm=180)
    beats = proc(act)                      # columns: time, beat-in-bar (1..4)
    times, pos = beats[:, 0], beats[:, 1]
    downbeats = times[pos == 1]
    ibi = np.diff(times)
    bpm = 60.0 / np.median(ibi)
    tempo_cv = float(np.std(ibi) / np.mean(ibi))      # >3 % = drifting / live / not straight 4/4
    return downbeats, float(bpm), float(times[0]), tempo_cv


def analyze(path):
    t0 = time.time()
    downbeats, bpm, first_beat, tempo_cv = beat_grid(path)
    r = analyze_with_grid(path, downbeats)
    r.update(bpm=round(bpm, 2), first_beat=first_beat, tempo_cv=round(tempo_cv, 4), seconds=round(time.time() - t0, 1))
    if tempo_cv > 0.03:
        r["flags"].append(f"tempo unstable (cv {tempo_cv:.1%}) - sync may drift")
    return r


# --- rekordbox xml --------------------------------------------------------
def write_xml(results, out_path, prefix=None, playlist_name="autocue"):
    lines = ['<?xml version="1.0" encoding="UTF-8"?>',
             '<DJ_PLAYLISTS Version="1.0.0">',
             '  <PRODUCT Name="rekordbox" Version="6.0.0" Company="Pioneer DJ"/>',
             f'  <COLLECTION Entries="{len(results)}">']
    for tid, r in enumerate(results, start=1):
        p = Path(r["path"])
        loc = p.as_posix()
        if prefix:
            loc = prefix.replace("\\", "/").rstrip("/") + "/" + p.name
        if not loc.startswith("/"):          # Windows drive letter
            loc = "/" + loc
        loc = "file://localhost" + urllib.parse.quote(loc, safe="/:")
        name = p.stem
        artist = ""
        if " - " in name:
            artist, name = name.split(" - ", 1)
        info = sf.info(r["path"])
        lines.append(
            f'    <TRACK TrackID="{tid}" Name={quoteattr(name)} Artist={quoteattr(artist)} '
            f'Kind={quoteattr(p.suffix[1:].upper() + " File")} Size="{p.stat().st_size}" '
            f'TotalTime="{int(r["duration"])}" AverageBpm="{r["bpm"]:.2f}" '
            f'DateAdded="{date.today().isoformat()}" SampleRate="{info.samplerate}" '
            f'Comments={quoteattr(track_comment(r))} Location={quoteattr(loc)}>')
        lines.append(f'      <TEMPO Inizio="{r["first_beat"]:.3f}" Bpm="{r["bpm"]:.2f}" Metro="4/4" Battito="1"/>')
        for letter, label, t, end in norm_cues(r["cues"]):
            rgb = CUE_SCHEME[letter][2]
            if end is not None:   # memory loop
                lines.append(f'      <POSITION_MARK Name={quoteattr(label)} Type="4" Start="{t:.3f}" End="{end:.3f}" Num="-1" '
                             f'Red="{rgb[0]}" Green="{rgb[1]}" Blue="{rgb[2]}"/>')
            else:
                lines.append(f'      <POSITION_MARK Name={quoteattr(label)} Type="0" Start="{t:.3f}" Num="-1" '
                             f'Red="{rgb[0]}" Green="{rgb[1]}" Blue="{rgb[2]}"/>')
        lines.append('    </TRACK>')
    lines += ['  </COLLECTION>', '  <PLAYLISTS>', '    <NODE Type="0" Name="ROOT" Count="1">',
              f'      <NODE Name={quoteattr(playlist_name)} Type="1" KeyType="0" Entries="{len(results)}">']
    lines += [f'        <TRACK Key="{tid}"/>' for tid in range(1, len(results) + 1)]
    lines += ['      </NODE>', '    </NODE>', '  </PLAYLISTS>', '</DJ_PLAYLISTS>']
    Path(out_path).write_text("\n".join(lines) + "\n", encoding="utf-8")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("inputs", nargs="*")
    ap.add_argument("-o", "--out", default="rekordbox.xml")
    ap.add_argument("--prefix", help="directory that holds these files on the rekordbox machine")
    ap.add_argument("--playlist", default=None)
    ap.add_argument("--json", help="cache/dump raw analysis to this JSON file (re-runs skip analyzed files)")
    ap.add_argument("--from-json", help="skip analysis; rebuild the XML from a previous --json dump")
    ap.add_argument("--gemini", action="append", default=[], help="gemini.json from gemini_check.py (repeat for a second opinion -> majority vote)")
    ap.add_argument("--prefer-gemini", action="store_true", help="on disagreement use Gemini's position")
    ap.add_argument("--second-drop", action="store_true", help="also cue G = second drop")
    ap.add_argument("--end-loop", type=int, default=0, metavar="BARS", help="add an N-bar memory loop on the last phrase")
    ap.add_argument("--review", default=None, help="write a review CSV (default: next to the XML)")
    args = ap.parse_args()
    opts = dict(second_drop=args.second_drop, end_loop_bars=args.end_loop)

    def finalize(results):
        llms = [json.loads(Path(g).read_text()) for g in args.gemini]
        for r in results:
            r["cues"] = norm_cues(r["cues"])
            if "downbeats" not in r:   # older cache entries: rebuild grid from bpm
                bar = 4 * 60 / r["bpm"]
                r["downbeats"] = [r["first_beat"] + k * bar for k in range(r["bars"])]
            if len(llms) >= 2:
                vote_sections(r, [l.get(r["path"]) for l in llms], r["downbeats"])
                r["cues"] = derive_cues(r["sections"], r["downbeats"], duration=r.get("duration"), **opts)
            elif llms:
                merge_llm(r, llms[0].get(r["path"]), r["downbeats"], args.prefer_gemini, **opts)
            else:
                r["cues"] = derive_cues(r["sections"], r["downbeats"], duration=r.get("duration"), **opts)
        review = args.review or str(Path(args.out).with_name("review.csv"))
        n = write_review(results, review)
        print(f"review list: {review} ({n} tracks marked CHECK)")

    if args.from_json:
        results = json.loads(Path(args.from_json).read_text())
        finalize(results)
        playlist = args.playlist or (Path(results[0]["path"]).parent.name if results else "autocue")
        write_xml(results, args.out, args.prefix, playlist)
        print_report(results)
        print(f"\nwrote {args.out} ({len(results)} tracks)")
        return

    files = []
    for inp in args.inputs:
        p = Path(inp)
        if p.is_dir():
            files += sorted(x for x in p.iterdir() if x.suffix.lower() in AUDIO_EXT)
        else:
            files.append(p)
    if not files:
        ap.error("no input files")

    # incremental: reuse previous results for files that haven't changed
    cache = {}
    if args.json and Path(args.json).exists():
        for r in json.loads(Path(args.json).read_text()):
            cache[r["path"]] = r
    results = []
    for i, f in enumerate(files, 1):
        prev = cache.get(str(f))
        if prev and prev.get("size") in (None, f.stat().st_size):
            results.append(prev)
            continue
        print(f"[{i}/{len(files)}] {f.name}", end=" ... ", flush=True)
        try:
            r = analyze(str(f))
            r["size"] = f.stat().st_size
        except Exception as e:  # keep going on one bad file
            print(f"FAILED: {e}")
            continue
        print(f"{r['bpm']:.1f} bpm, {r['bars']} bars, {r['seconds']}s" + (f"  [{'; '.join(r['flags'])}]" if r["flags"] else ""))
        results.append(r)
        if args.json:   # save progress as we go
            Path(args.json).write_text(json.dumps(results, indent=1))

    finalize(results)
    playlist = args.playlist or files[0].parent.name
    write_xml(results, args.out, args.prefix, playlist)
    print_report(results)
    print(f"\nwrote {args.out} ({len(results)} tracks)")


if __name__ == "__main__":
    main()
