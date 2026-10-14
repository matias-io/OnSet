# autocue — automatic A–F memory cues for rekordbox

**Documentation map**
- `HANDOFF.md` — requirements, context, decisions, everything attempted and the results. Read first.
- `docs/SESSION_TIMELINE.md` — chronological log of the session.
- `docs/WINDOWS_RUNBOOK.md` — step-by-step on the rekordbox machine.
- `docs/DOWNLOAD.md` — yt-dlp procedure and quality findings.
- `docs/METHOD.md` — how detection, merging, review status and tuning work.
- `docs/TRACKS.md` — the delivered cues for every track.
- `docs/SPECTROGRAM_READS.md` — the second-opinion reads vs the energy analysis, with notes.
- `docs/GEMINI_ATTEMPTS.md` — raw records of every Gemini pass.
- this file — usage of the scripts.

Places six memory cues on every 4/4 dance track:

| Cue | Meaning | Colour |
|---|---|---|
| **A** | first downbeat | green |
| **B** | 32 bars before the first drop | yellow |
| **C** | 16 bars before the first drop | orange |
| **D** | first drop | red |
| **E** | first breakdown after the drop | blue |
| **F** | start of the outro | purple |
| **G** | second drop (opt-in `--second-drop`) | pink |
| **L** | N-bar memory loop on the last phrase (opt-in `--end-loop N`) — lets you extend a radio edit's hard ending | aqua |

B and C are derived arithmetically from D. D, E and F come from a per-bar
energy analysis (bass band 20–150 Hz weighted with full band): the drop is the
earliest large sustained jump, the breakdown the first sustained loss after it
(judged against the drop's own level), the outro the last sustained loss with
no recovery. Section edges are snapped to the 4-bar phrase grid anchored at the
drop. Cues that can't be placed (e.g. intro shorter than 32 bars, hard ending
with no outro) are simply omitted, and each track gets flags telling you what
to double-check.

Accuracy is good on straightforward house / big-room / techno; expect to check
tracks flagged `weak drop`, `no drop found`, `drop is late`, or where rekordbox's
phrase analysis or Gemini disagrees. Every run writes `review.csv` with the
tracks that need a listen (`CHECK`) sorted to the top.

## Second opinion: Gemini actually listens (`gemini_check.py`)

`gemini_check.py` sends each track to Gemini through the `agy` CLI and asks for
the drop / breakdown / second drop / outro, plus BPM, whether there are vocals
in the intro and outro, a 1–10 energy level and a sub-genre. Both `autocue.py`
and `autocue_rb.py` take `--gemini gemini.json`:

- sections the energy analysis couldn't place are filled from Gemini
  (snapped to the beat grid and refined to the nearest energy edge);
- disagreements > 2 bars are flagged (`--prefer-gemini` makes Gemini win);
- energy / genre / vocal flags go into the track comment
  (`E8 | tech house | vocal outro | ...`; `--write-comments` in the DB version).

```bash
.venv/bin/python gemini_check.py analysis.json --out gemini.json --jobs 3 --proxy-dir proxies
```

`--proxy-dir` points at 64 kbps mono MP3 copies (make them with ffmpeg:
`-ac 1 -ar 22050 -b:a 64k`); they upload ~10x faster and timestamps are
unchanged. Results are cached in `gemini.json`; `--retry-errors` re-asks failures.

**Prompt pitfalls learned the hard way** (all produce confident, well-formatted,
*wrong* answers on 30-second multiples like 0:30 / 1:00 / 1:30):

- Telling the agent "do not run any tools" — reading the audio file *is* a tool
  call in `agy`, so it skips the file and invents a structure. Never say that.
- Long, heavily-structured prompts had the same effect. Keep the wording close
  to: *Listen to the audio file "X" in this directory. ... Reply as JSON only.*
- Without `--sandbox` the agent sometimes decides to "run stem separation" and
  never answers. `--sandbox` blocks that and it falls back to listening.
- Suspiciously fast answers (< 40 s) usually mean the file wasn't read.

`tune.py analysis.json gemini.json` sweeps the detection thresholds offline
against Gemini's labels (the energy curves are cached, so 240 parameter sets
run in seconds) and prints agreement counts and per-parameter effects.

## Two ways to run it

### 1. `autocue_rb.py` — on the rekordbox machine (recommended)

Writes cues straight into rekordbox's `master.db`, using rekordbox's own beat
grid and phrase analysis. Cues land exactly on rekordbox's grid, re-runs replace
only the cues autocue wrote (your manual cues are untouched), and a backup of
`master.db` is made before every write.

**One-time setup (Windows, PowerShell):**

```powershell
winget install Python.Python.3.12          # if you don't have Python 3.10–3.12
& "$env:LOCALAPPDATA\Programs\Python\Python312\python.exe" -m venv .venv
.venv\Scripts\python -m pip install -r requirements-windows.txt
```

(pyrekordbox 0.4.4 ships the database key; older docs mention a `download-key`
step that no longer exists. Optional: put `ffmpeg` on PATH if you have `.m4a` files.)

**Each time:**

1. Import the new tracks into rekordbox and let it finish analysing them
   (beat grid + phrases — Preferences → Analysis → enable Phrase analysis).
2. Dry run (rekordbox can stay open — it works on a snapshot of the database).
3. **Close rekordbox**, then write:

```powershell
cd %USERPROFILE%\Music\autocue
.venv\Scripts\python autocue_rb.py --folder "%USERPROFILE%/Music/library" --dry-run   # look first
.venv\Scripts\python autocue_rb.py --folder "%USERPROFILE%/Music/library"             # write
```

Other selectors: `--folder "%USERPROFILE%/Music/library"`, `--all`, or file
names. Options:

- `--dry-run` — analyse and print the report, write nothing.
- `--prefer-phrases` — where rekordbox's phrase analysis puts the drop /
  breakdown / outro more than 2 bars from the energy analysis, use rekordbox's
  position. Without it the disagreement is only reported as a flag.
- `--replace-all` — also delete memory cues you placed by hand (default keeps them).
- `--no-backup` — skip the `master.db.autocue-<timestamp>.bak` copy.
- `--json analysis.json` — dump per-bar energy and sections for inspection.
- `--gemini gemini.json` / `--prefer-gemini` — see "Second opinion" above.
- `--vote` — majority vote between the energy analysis, the `--gemini` file(s) and
  rekordbox's phrase analysis; with `--prefer-gemini` the first opinion file breaks
  ties, otherwise the energy analysis does. Tracks where nobody agrees are flagged.
  Without `--vote`, phrases only fill sections the other sources left empty and
  flag disagreements (`rekordbox phrases put …`, `drop: 2/3 sources …`).
- `--second-drop`, `--end-loop 8` — extra cues G and L.
- `--intro-loop 8` — cue I: an 8-bar loop on the first 8 bars (skipped if the drop is inside them;
  shed first, then L, then G, if hand-placed memory cues would push a track past rekordbox's 10).
- `--hot-cues` — write hot cues instead of memory cues: A–G by letter, end loop = H, no intro loop.
  Hand-placed hot cues keep their slot (our cue for that slot is skipped) unless `--replace-all`.
  Run once without and once with `--hot-cues` to get both; each run only replaces its own kind.
- `--write-comments` — energy/genre/vocals/flags into rekordbox's Comments column.

4. Start rekordbox. Cues appear in the memory cue list with their names as
   comments and their colours.

To restore: close rekordbox, copy the `.bak` back over
`%APPDATA%\Pioneer\rekordbox\master.db`.

### 2. `autocue.py` — anywhere, produces `rekordbox.xml`

Standalone: computes its own beat grid with madmom (Linux/macOS setup is in the
`.venv` here), writes a rekordbox XML you import via
Preferences → Advanced → Database → *rekordbox xml* → Imported Library.
Use `--prefix` to point the XML at where the files live on the rekordbox
machine, and note that XML import only adds cues to tracks *not already in the
collection* — remove them first if re-importing.

```bash
.venv/bin/python autocue.py "/path/to/folder" --json analysis.json \
    -o rekordbox.xml --prefix "%USERPROFILE%/Music/library"
```

## Mix point links (`mixplan.py`)

rekordbox has no "mix point link" of its own — none of the 48 tables in `master.db` 7.2.18
links a cue in one track to a cue in another, and Related Tracks lists are rule-based
(`Criteria` JSON) with no stored membership. So `mixplan.py` computes the links instead,
from the cues autocue placed plus rekordbox's BPM and Camelot key:

```powershell
.venv\Scripts\python mixplan.py --top 5 --set
.venv\Scripts\python mixplan.py --from "FISHER - Stop It"        # options out of one track
.venv\Scripts\python mixplan.py --set --start "Daft Punk" --write-playlist "autocue set"
```

- `mixplan.csv` — every compatible transition: **out of** track A at its F (outro) or E
  (breakdown), **into** track B at B's 32-bars-before-drop cue (or 16, or the intro — whichever
  fits the bars left in A), with the pitch change and the Camelot move it needs.
- `mixset.csv` — one greedy set order through the library using those transitions.
- `--write-playlist NAME` — that order as a real rekordbox playlist (rekordbox must be closed).

Scoring: key (same 4, ±1 on the wheel 3, relative major/minor 3, +7 energy boost 2) plus BPM
(≤1 % 3, ≤3 % 2, ≤6 % 1). Half- and double-time matches are allowed but capped at 1 so they
never win a tie — otherwise the set order ping-pongs between 75 and 150 BPM.

## Files

- `autocue_core.py` — shared analysis (energy curve, section detection, cue scheme, colours).
- `autocue_rb.py` — rekordbox database version (Windows/macOS).
- `autocue.py` — madmom + XML version (same options; `--from-json` rebuilds the XML without re-analysing).
- `gemini_check.py` — Gemini listening pass via `agy`; `run_gemini.cmd` — the long pass, launched so it outlives the calling shell; `tune.py` — threshold sweep; `refresh.py` — recompute curves after core changes.
- `mixplan.py` — mix point links: cue-to-cue transitions, set order, rekordbox playlist.
- `testdb/` — synthetic `master.db` fixture and an end-to-end test of the DB writer.
- `requirements-windows.txt` — deps for the rekordbox version.

## Tuning

Thresholds live at the top of `autocue_core.py` (`DROP_MIN_JUMP_DB`,
`BREAK_MIN_DROP_DB`, `HIGH_PCTL`, `BASS_WEIGHT`). Dump `--json` and look at the
`energy` array (dB per bar) for any track that comes out wrong.
