> Recorded before the preparation app became part of OnSet, when it was a separate app called Autocue. Folder and script names below are from that time; current names are in [desktop operation](../DESKTOP.md).

> Archived September 2026 runbook, retained as historical evidence. Do not use its environment setup, direct writes, manual database replacement or source-edit workarounds as current instructions. Use [the current Windows runbook](../WINDOWS_RUNBOOK.md). No music acquisition is part of the desktop release.

# Windows runbook (rekordbox machine)

Target layout assumed by the delivered `rekordbox.xml`: the FLACs sit **directly**
in `%USERPROFILE%\Music\library\`. If you keep them in a subfolder instead, use
`rekordbox-subfolder.xml` (paths `%USERPROFILE%\Music\library\2025 Top DJ Lineup\`)
or regenerate (step 6).

## 0. Unpack

Unzip `autocue-windows.zip`. You get:

```
autocue-windows\
  library\2025 Top DJ Lineup\*.flac      the music (copy where you like)
  autocue\                               code, data, docs
```

Copy the FLACs to `%USERPROFILE%\Music\library\` (flat) — or keep the subfolder.

## 1. One-time setup

**Done on this machine (2026-09-19):** Python 3.12.10 via winget (no `py` launcher —
use `%LOCALAPPDATA%\Programs\Python\Python312\python.exe`), `.venv` in
`%USERPROFILE%\Music\autocue`, requirements + librosa/matplotlib installed.
pyrekordbox 0.4.4 has the database key built in — there is no `download-key`
command any more; ignore that step in older docs.

```powershell
winget install Python.Python.3.12
cd %USERPROFILE%\Music\autocue
& "$env:LOCALAPPDATA\Programs\Python\Python312\python.exe" -m venv .venv
.venv\Scripts\python -m pip install -r requirements-windows.txt
```

If you have `.m4a` files, install ffmpeg and put it on PATH (it is).

## 2. Import and analyse in rekordbox

Import the tracks into rekordbox (drag the folder in), let it finish analysing.
Preferences → Analysis: enable **Phrase** analysis (gives the cross-check).
Optionally make a playlist "2025 Top DJ Lineup" — the script can target it by name.

**State found on 2026-09-19:** rekordbox 7.2.18; all 99 FLACs (in the
`2025 Top DJ Lineup` subfolder) plus 23 MP3s in the library root are imported and
beat-gridded (122 tracks). **No phrase analysis on any track** — enable it in
Preferences → Analysis and re-analyse if you want the cross-check. There is no
"2025 Top DJ Lineup" playlist, so select by folder (below).

## 3. Dry run (rekordbox may stay open)

A `--dry-run` no longer needs rekordbox closed: it copies `master.db` to a temp
folder and analyses the copy. Only a real write needs rekordbox closed.

```powershell
.venv\Scripts\python onset_service/cuewriter.py --folder "%USERPROFILE%/Music/library" --gemini claude_spec.json --prefer-gemini --second-drop --end-loop 8 --write-comments --dry-run
```

Read the report. Lines with `FAILED: no beat grid` mean rekordbox hasn't analysed
that track. `rekordbox phrases put drop at …` flags show where rekordbox's phrase
analysis disagrees. `review.csv` is written next to the script.

`claude_spec.json` now covers all 122 tracks (the 23 MP3s were read from
`spectrograms_extra/` on 2026-09-19; the raw reads are in `spec_reads_mp3.txt`).

## 4. Close rekordbox

The script refuses to write while rekordbox is running.

## 5. Write

Same command without `--dry-run`. It backs up `master.db` to
`master.db.autocue-<timestamp>.bak` in the same folder first, writes memory cues
(A–G + loop L), and sets the track Comments to e.g.
`E8 | tech house | vocal outro | <flags>`.

Start rekordbox; cues appear in the memory cue list with their names.

Re-running is safe: only cues tagged with autocue's UUID prefix are replaced; cues
you placed by hand are kept. `--replace-all` removes all memory cues first.

Restore: close rekordbox, copy the `.bak` back over
`%APPDATA%\Pioneer\rekordbox\master.db`.

## 6. Alternative: XML import (no database writes)

Preferences → Advanced → Database → rekordbox xml → point at `rekordbox.xml`; the
"rekordbox xml" tree appears in the browser; drag tracks/playlist into Collection.
Only works for tracks **not already** in the collection. To regenerate for another
path (from the shipped `analysis.json`, no re-analysis):

```powershell
.venv\Scripts\python autocue.py --from-json analysis.json --gemini claude_spec.json ^
  --prefer-gemini --second-drop --end-loop 8 --prefix "D:/Music/whatever" -o rekordbox.xml
```

(`autocue.py` imports madmom at start; on Windows without madmom, use the
`--from-json` path only after commenting out the madmom import, or just use
`onset_service/cuewriter.py`.)

## 7. Review

Open `review.csv`: `CHECK` rows first. Listen to those (42 in the delivered set),
fix cues in rekordbox by hand. The `flags` column says why each was flagged;
`llm_notes` has the reader's comment.

## 8. New tracks later

1. Add the new audio files to the library folder.
2. (Optional) second opinion: `spectro.py` works in this venv now (librosa installed):
   `onset_service/cuewriter.py --folder … --dry-run --json a.json` then
   `spectro.py a.json spectrograms_new`, read the PNGs, record with `spec_record.py`.
   Gemini via `gemini_check.py` needs `agy` or an API key — see `README.md` pitfalls.
3. Import → analyse → close rekordbox → `onset_service/cuewriter.py --folder "C:/…/library"`.
   Without an opinion file it still writes energy-based cues; add
   `--prefer-phrases` to let rekordbox's phrase analysis win on disagreements.
