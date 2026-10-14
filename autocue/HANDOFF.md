# HANDOFF — autocue (rekordbox memory-cue automation)

Written 2026-09-18 for whoever picks this up (human or agent). Everything below is
from one working session; where a number is quoted it was measured in that session.
Read this before `README.md` (usage) — this file is the *why* and the *what happened*.

---

## 1. Requirements, in the order the user stated them

1. **Download YouTube (Music) audio at the highest quality YouTube Premium allows**,
   for use in rekordbox. Account: `[personal account]` (Chrome `Default` profile
   on this Linux box; `[personal account]` is `Profile 2`).
2. Output must play in rekordbox at the best available quality: user asked for
   320 kbps, then "higher than 320", then "best available for rekordbox".
3. Only keep tracks whose source stream is "performance quality" (the Premium
   256 k tier); tracks that only had the 135 k stream should be excluded.
4. Download entire playlists at that quality.
5. **Automatically place rekordbox memory cues** using the standard scheme:
   - A first beat · B 32 bars before first drop · C 16 bars before first drop ·
     D first drop · E first breakdown · F start of outro.
6. Cue placement should be as accurate as possible; the user asked whether an AI
   model could do it and later asked to use **Gemini via the `agy` CLI** as a check
   ("sometimes better at analyzing audio"), and then whether **Claude Opus 5** could
   help.
7. Compare with **CueGen** (github.com/mganss/CueGen) and borrow what's useful.
8. The pipeline should run **on the Windows machine that has rekordbox**
   (files at `%USERPROFILE%\Music\library`, user wrote it as
   `~/Music/library`).
9. Brainstorm and implement further improvements; add other things useful for
   DJing.
10. The long-running Gemini job must **auto-resume after brief network drops** and
    must not sit making no progress. "It can be slow but it must work, and work as
    well as possible."
11. Finally: produce this handoff documentation.

## 2. Context and constraints discovered

- **Machine:** Fedora Linux (kernel 7.2.5), 16 cores, no GPU, Python 3.14 system +
  3.11 available, `uv` installed to `~/.local/bin/uv`. rekordbox is *not* on this
  machine; it's on the user's Windows PC.
- **yt-dlp** 2026.08.19. Premium audio (format `774`, Opus ~262 k; `141`, AAC 258 k)
  only appears with `--extractor-args "youtube:player_client=web"`; default clients
  top out at `251` (135 k). No PO-token plugin needed (verified download).
- **rekordbox constraints:** does not play Opus → must transcode. Memory cue limit
  10/track. Cue positions in XML are absolute seconds; in `master.db` they're
  `InMsec` + `InFrame = msec*150/1000`. Memory-cue colours are indices 0–7
  (Pink Red Orange Yellow Green Aqua Blue Purple). Loops: `Kind=4`,
  `OutMsec`, `BeatLoopSize = 0x10000*beats + 1` (from CueGen source).
- **`agy`** = Google Antigravity CLI (`~/.local/bin/agy`, OAuth via the user's
  Google AI Pro subscription, config under `~/.gemini/antigravity`). Models seen:
  gemini-3.8/3.7/3.6-flash (high/medium/low), gemini-3.1-pro, claude-*, gpt-oss.
  It is an *agent* (has file/shell tools), not a plain model call.
- **Gemini API key** provided by the user (stored at
  `~/.config/autocue/gemini_api_key`, mode 600; **never commit or print it**). No
  billing set up → free tier: **20 requests/day per model** for `gemini-3.8-flash`;
  3.8/3.7 also returned 503 "high demand" for long stretches;
  `gemini-flash-latest` accepted requests.
- **Subscription quota:** `agy` itself hits "Individual quota reached … Resets in
  Xh Ym" after a few dozen calls (many of ours were wasted by failures).
- **Source material** is YouTube video/visualiser/radio edits, not extended mixes:
  short intros (often < 32 bars → B/C omitted), hard endings (no outro), fake-out
  drops. This is inherent and affects every method.
- The user kept adding tracks to the folder during the session (48 → 56 → 70 → 99).

## 3. Decisions taken and why

| Decision | Rationale |
|---|---|
| Download format `774` → **FLAC** (16-bit via `-sample_fmt s16`) | Lossless wrapper of the best stream; no second lossy pass; rekordbox plays FLAC (CDJ-2000NXS2+). AIFF suggested if older CDJs. |
| Keep the one 135 k-only track out (moved to `~/Downloads/_low-bitrate/`) | User's "performance quality only" rule. `rm` was blocked by the permission classifier, so it was moved. |
| Section detection = **per-bar energy curves on a beat grid**, not `allin1` | `allin1` took 8 min/track on CPU and labelled an EDM track as all "chorus" — useless for drops. Energy (bass-weighted 20–150 Hz + full band) is fast (seconds) and interpretable. |
| Beat grid from **madmom** on Linux; from **rekordbox's own ANLZ files** on Windows | madmom is painful to build on Windows; rekordbox already has the grid + phrase analysis, and cues then land exactly on its grid. |
| Two front-ends sharing one core (`autocue_core.py`) | XML route (any OS) vs direct `master.db` route (rekordbox machine). |
| Direct DB writes copy **CueGen's conventions**: backup first, UUID-prefix tag so re-runs replace only our cues, refuse while rekordbox runs | Learned from CueGen source; keeps manual cues safe. |
| B/C are **arithmetic** from D (D−32, D−16 bars), omitted if they'd land before bar 0 | The scheme defines them that way; short intros make them impossible. |
| Sections snapped to the **4-bar phrase grid anchored at the drop**; medians not means in windows | Removes 1–2-bar "drop-out fill" errors seen in the curves. |
| Breakdown judged **relative to the drop's own level**, not a global percentile | Fixed a vocal-verse breakdown (Titanium) that was only ~6 dB under the drop. |
| Added a **mid/high-band curve** for outros | Outros where the kick continues but vocal/lead leave (FISHER 2:30) are invisible to the bass curve. |
| Gemini prompt kept close to the *original short wording* | Long prompts or any mention of "do not run tools" made the agent skip the audio and hallucinate structure (confident, on 30-s multiples). |
| Serialise `agy` calls; sandbox; isolated proxy folder | Concurrency made the agent "launch analysis scripts"; it found and tried to use our `allin1` venv when proxies lived under `autocue/`. |
| **Spectrogram reading by Claude** as the second opinion | Claude can't hear audio but can read a bar-gridded mel spectrogram; this needs no quota and was verified on 3 known tracks before use. |
| Thresholds set by **offline sweep** against the spectrogram reads | Energy curves are cached in JSON, so 240 parameter sets run in seconds. |
| `review.csv` CHECK = drop/grid genuinely in doubt only | First version flagged 83/99; useless. Now 42/99. |
| Extras opt-in: G (second drop), L (8-bar end loop), comments | Beyond the user's A–F scheme, so not on by default in the scripts (but used in the delivered XML per the user's "implement those improvements"). |

## 4. Everything attempted, with results

### Download / quality
- `-f bestaudio` default → only 135 k Opus. **Fixed** with `player_client=web` → 774/141.
- `--audio-format mp3 --audio-quality 0` → lossy→lossy; replaced by FLAC. 24-bit FLAC
  from ffmpeg was fake precision → `-sample_fmt s16`.
- Playlist download of "2025 Top DJ Lineup" (RDCLAK5uy_lKSrV3_23yjnOb7UNhGtBoCOyM_rJY5GM):
  47/48 got 774; "Alan Walker – Faded" only 251 → moved out.

### Structure analysis
- **allin1** (+natten CPU wheel for torch 2.6, patched `get_device_cc` for no-CUDA,
  shim for renamed `natten1dav` etc.): works but 8 min/track, labels useless on EDM.
  Venv still has it; not used.
- **madmom** (git master, numpy<2, Python 3.11): ~15–30 s/track. Two BPM errors seen
  on 99 tracks (double-time on Moonshine, half-time question on Alone — Alone is
  actually 97 BPM, so only Moonshine is wrong). Irrelevant on Windows (rekordbox grid).
- **Energy method iterations:** v1 mean windows (off by 1–2 bars, fooled by 1-bar
  fills) → v2 medians + edge refine → v3 phrase snap → v4 breakdown relative to drop
  level → v5 mid/high curve for outros → v6 second-drop detection → v7 tuned
  thresholds. Tuned defaults: `DROP_MIN_JUMP_DB 4, DROP_REL_TO_MAX 0.3,
  BREAK_MIN_DROP_DB 4, HIGH_PCTL 90`.
- **Agreement, energy vs spectrogram reads (within 2 bars), after tuning:**
  drop 79/98, breakdown ~46/86, outro ~14/32. Remaining drop errors: "picked a later
  bigger drop" and "picked an early kick-only section".

### CueGen study
- CueGen does **no audio analysis**; it converts rekordbox phrase analysis / Mixed In
  Key cues into cue points, writing `master.db` directly. Can't express A–F.
  Borrowed: DB write conventions, backup, UUID tagging, colours, loop encoding,
  bar-snapping. `pyrekordbox` 0.4.4 is the Python equivalent (handles the SQLCipher
  key via `python -m pyrekordbox download-key`, ANLZ parsing incl. the PSSI XOR mask).

### rekordbox integration
- `autocue_rb.py` tested end-to-end against a **synthetic** `master.db` built from
  pyrekordbox's SQLAlchemy models (`testdb/make_testdb.py`, `testdb/run_test.py`):
  dry-run writes nothing; real run backs up + adds cues; re-run replaces only our
  cues (manual cue survives); loops encode correctly; comments written; USNs bump.
  **Not yet tested on a real rekordbox database or real ANLZ files** — first Windows
  run must be `--dry-run`.

### Gemini via `agy` (chronological)
1. Single call, original short prompt, FLAC in music folder → **correct** (FISHER
   0:45/1:15/2:30), ~1 min.
2. Batch of 70 at 3-wide, FLACs → ~5 min/call, 2/8 errors (timeouts, "launched
   analysis, waiting"). Stopped.
3. Extended prompt with "Do NOT write or run any code or tools" → **hallucinated**
   (0:30/1:00/1:30 template answers, confidence "high"), fast (<40 s). 47 wrong
   results discarded (`gemini_bad_prompt.json`).
4. MP3 proxies (64 k mono, 22.05 kHz, ~2 MB) verified **fine** with the original prompt.
5. Prompt v3 ("read it directly; do not write analysis scripts") → still hallucinated.
6. Prompt v4 = original wording + extra fields, no tool talk → **correct** on FISHER
   and Abyss; Samsara wandered into "stem separation" (no sandbox).
7. `--sandbox` single call → Samsara **correct** (0:31/2:03/3:47) but 464 s.
8. Batch, sandboxed, 3-wide, proxies under `autocue/` → 29 good / 47 errors:
   43 empty outputs, agent tried "allin1" (found our venv). Stopped.
9. Isolated proxy folder, sandbox, 4-wide → 5/5 failed ("launched a command").
   `--mode plan` test hung 600 s.
10. Diagnosis with stderr captured: **`agy` subscription quota exhausted**
    ("Resets in 2h20m"); empty outputs were quota errors retried internally ~10 min.
11. Result: `gemini_check.py` now has: serialised `agy` (lock), sandbox, quota
    parsing + sleep-until-reset, network wait, supervisor passes, API circuit breaker.
    The last run (`--hybrid --until-done`, 1 worker) was sleeping for quota when the
    session ended and **did not survive the restart**. `gemini.json` contains only
    quota errors; nothing usable from Gemini exists except the 7 first-run answers in
    `gemini_old_prompt.json` (old prompt, FLAC) and `gemini_agent_partial.json`.

### Gemini via API
- `ask_api()` implemented (inline base64 audio, `responseMimeType: application/json`,
  temperature 0). Samsara answered in 30 s with a nuanced, plausible read.
- Blocked by free tier: `gemini-3.8-flash` = **20 requests/day**, plus 503s.
  `gemini-flash-latest` accepted requests but its audio accuracy was never verified
  (the test was interrupted by the user).

### Claude spectrogram reads
- `spectro.py` renders a mel spectrogram (0–8 kHz, 96 mels) with bar-grid lines
  (thin every 4, thick every 16) and bar-index/time ticks. Blind test on FISHER,
  Samsara, Abyss matched the trusted answers. All 99 tracks were read and recorded
  in `claude_spec.json` (same schema as gemini.json; `confidence` reflects my
  certainty; no vocal/energy/genre fields).
- Known weak spots: pop structures (Habits, International Love, Roses, Stay,
  Cha Cha Cha, Dragostea), live recordings (Aoki×Zhangye, Titanium live edit),
  tracks where the sub never stops (Ocean, Fragments, Azukita, 10:35, Veldt radio).

## 5. Current deliverables and files

```
~/Downloads/2025 Top DJ Lineup/     99 FLACs (16-bit, from format 774) + rekordbox.xml + review.csv
~/Downloads/_low-bitrate/           Alan Walker – Faded (135 k only, excluded)
~/Downloads/autocue/
  autocue_core.py      shared analysis: curves, find_sections, derive_cues, merge_llm, vote_sections, review
  autocue.py           madmom grid → rekordbox.xml (any OS). --from-json regenerates without re-analysis
  autocue_rb.py        rekordbox-machine version: ANLZ grid/phrases → master.db (needs rekordbox closed)
  gemini_check.py      Gemini second opinion (API / agy / hybrid), supervisor, quota+network handling
  verify.py            re-ask only disagreeing tracks → gemini2.json
  tune.py              threshold sweep vs a labels file (gemini.json / claude_spec.json)
  refresh.py           recompute curves for all entries after core changes (reuses cached grids)
  spectro.py           render bar-gridded spectrograms; spec_record.py records manual reads
  analysis.json        per-track: bpm, downbeats, energy, energy_hi, sections, cues, flags (99 tracks)
  claude_spec.json     spectrogram reads for all 99 (keyed by Linux path; rb script re-keys by file name)
  gemini*.json         partial/failed Gemini runs (see §4)
  proxies/             64 k mono MP3 proxies (also copied to the session scratchpad for isolation)
  spectrograms/        99 PNGs
  testdb/              synthetic master.db fixture + end-to-end DB write test
  README.md, requirements-windows.txt, HANDOFF.md (this file)
  .venv                Python 3.11: torch 2.6 cpu, natten 0.17.5 (patched), allin1, madmom, librosa,
                       matplotlib, pyrekordbox 0.4.4, soundfile, scipy
```

The delivered XML was produced by:
```
autocue.py --from-json analysis.json --gemini claude_spec.json --prefer-gemini \
  --second-drop --end-loop 8 --prefix "%USERPROFILE%/Music/library" \
  --playlist "2025 Top DJ Lineup" -o ".../2025 Top DJ Lineup/rekordbox.xml"
```
(`--prefer-gemini` here means "prefer the spectrogram read on disagreement"; with two
`--gemini` files the scripts switch to majority vote via `vote_sections`.)

## 6. Data formats

- `analysis.json`: list of dicts — `path, bpm, first_beat, tempo_cv, duration, bars,
  downbeats[s], energy[dB per bar], energy_hi[dB per bar], sections{drop,breakdown,
  outro,second_drop → bar index or null}, flags[], cues[(letter,name,start_s,end_s|null)]`.
- Opinion files (`gemini.json`, `claude_spec.json`, `gemini2.json`): dict keyed by
  path → `{drop,breakdown,second_drop,outro: "m:ss"|null, bpm, vocals_intro,
  vocals_outro, energy 1-10, genre, confidence, notes, error?, via?}`.
- `review.csv`: `status,track,bpm,A..G,L,energy,genre,vocals_intro,vocals_outro,flags,llm_notes`,
  CHECK rows first.
- rekordbox XML: `POSITION_MARK Type=0 Num=-1` memory cue, `Type=4` with `End` = loop;
  `Location=file://localhost/C:/...`; `Comments` carries energy/genre/flags.

## 6b. Windows session, 2026-09-19 (picking up from §8 step 1)

Done on the rekordbox PC (`%USERPROFILE%\Music\autocue`, this folder):

- Python 3.12 + `.venv` installed; pyrekordbox 0.4.4 opens the real `master.db`
  (rekordbox 7.2.18) and parses the real ANLZ files. **PQTZ grids read fine for all
  122 tracks**; the SQLCipher key is built into pyrekordbox (no `download-key`).
- `autocue_rb.py --dry-run` now works **while rekordbox is open** (snapshot copy of
  the DB; ANLZ read from the real `share/`). Dry run on all 122 tracks: 0 failures,
  ~1 s/track. Nothing written yet — a real write still needs rekordbox closed.
- rekordbox's grid matches madmom's bar-for-bar (energy curves identical) but its
  bar 0 is often 1 bar earlier / a beat off. **Snapping the drop to a 4-bar grid
  anchored at bar 0 hurt on the rekordbox grid** (51 vs 61 of 97 drops within 1 bar
  of the spectrogram reads) while it helped on madmom's (70 vs 63). `find_sections`
  now takes `phrase_anchor` (0 for madmom, None or the first Chorus phrase bar for
  rekordbox). Drop refinement rule kept as "sharpest single-bar rise" — 7 variants
  tested, none better.
- Bug fixed: when a second opinion moved the drop, the energy-based breakdown /
  second drop stayed where they were (10:35 had E before D). Sections are now
  re-derived relative to the new drop (`rebase_on_drop`) and ordering is enforced
  (`order_sections`); same fix in `vote_sections` and `--prefer-phrases`.
- The 23 MP3s the user keeps in the library root got spectrogram reads
  (`spectrograms_extra/`, `spec_reads_mp3.txt`); `claude_spec.json` now has 122 keys.
- `review.csv` is written as UTF-8 with BOM (Excel/PowerShell read the accents).
- No phrase analysis exists in this rekordbox library (PSSI absent on all tracks).

**Written to the real `master.db` (2026-09-19 03:39): 816 cues on 122 tracks**
(A–G + 8-bar end loop, comments), with rekordbox phrase analysis present on all 122.
Backups `master.db.autocue-20260919-*.bak` next to the DB (latest 033932). Fixed on
the way: end loop was clipped to the audio end (24 loops of 28–30 beats, one of 92);
now always exactly N bars. Added: phrase gap-filling, `--vote` (energy / read /
phrases majority, `tie_pref`), and a `drop: 2/3 sources` flag for the 15 tracks where
energy + phrases agree against the spectrogram read (the read was kept; listen to
these first). Three-way agreement on 122 tracks (within 2 bars): drop — all three
agree 77/120, energy~read 93, energy~phrases 92, read~phrases 82; breakdown — 30/80
all three; outro — phrases are the best source (energy is the outlier 19×).

**04:33 write:** memory cues re-written with `--intro-loop 8` (926 memory cues incl. loops I/L),
then a second pass with `--hot-cues` (811 hot cues A–H; hand-placed hot cues on Stop It,
Dashstar, Charlotte de Witte, W&W The One, Lost Frequencies kept their slots). Hot cue D and
memory loops both use `Kind=4` (distinguished by `OutMsec`) — check in rekordbox that hot
cue D shows as a cue and I/L as loops. User's hand cues are *not* reference drops (mixed).

**Gemini on Windows — SOLVED (2026-09-27).** `agy` 1.2.7 at `%LOCALAPPDATA%\agy\bin`, logged in
via OAuth. Headless `-p` mode *aborts the whole turn* when any tool needs a permission it cannot
prompt for, so the agent's habit of running a shell command before opening the audio killed every
run. The fix is a **deny** list (denied tools return an error the model can recover from, instead
of aborting) in `~\.gemini\antigravity-cli\settings.json` — Claude Code's classifier refuses to
write this file, the user must:

```json
"permissions": {
  "allow": ["command(Get-ChildItem)"],
  "deny":  ["command(*)", "read_url_content(*)", "read_url(*)", "write_file(*)"]
}
```

Verified: FISHER STAY returned 0:45 / 1:15 / 2:30 (the known-correct answer) in 142 s.

Facts learned about `agy` permissions, from its binary and transcripts:
- Permission *categories* are not tool names. Only `command(...)`, `read_file(...)`,
  `write_file(...)`, `read_url(...)` and `mcp(...)` exist. The tool is `write_to_file`, the
  permission is `write_file(*)`.
- `unsandboxed(*)` is rejected: *"unsandboxed is no longer a valid rule, use command() instead"*.
- **`search_web` has no permission category, so it cannot be denied.** The agent still runs 5-9
  web searches (Beatport, 1001tracklists) *after* reading the audio on most tracks. It is still
  listening — Diamonds came back drop 1:15 / outro 3:07 against the spectrogram read's 1:16 / 3:05
  — but treat round 30-second answers (0:30/1:00/1:30) as the old hallucination signature and
  cross-check them. `enabledTools`/`disabledTools` in settings apply only to MCP servers.
- Verify any run with the step transcript at
  `~\.gemini\antigravity-cli\brain\<id>\.system_generated\logs\transcript.jsonl`: a good run has
  `view_file` on the .mp3 as step 1. A fast answer (<40 s) is fine *if* view_file preceded it.
- `gemini_check.py` now builds ASCII-safe proxies on demand in `%USERPROFILE%\Music\gemini-proxies`
  (`safe_stem`/`proxy_for`): the original YouTube titles' `[brackets]` are PowerShell wildcards
  (they silently broke `Copy-Item` without `-LiteralPath`) and the agent's file tool chokes on them.
  It also rejects agy's metadata block (`conversationId`, no `drop`) as `error: no answer`.

The Python SDK `google-antigravity` needs a Gemini API key (Developer API, free tier) — it is
**not** a route to the subscription; the CLI is.

## 7. Known gaps / risks

- The write path is verified on the real DB only by reading it back with pyrekordbox
  (cue count, positions, loop sizes, manual cues untouched) — not yet by opening
  rekordbox. Keep the `.bak` until a few tracks have been checked in rekordbox.
- XML import only adds cues to tracks *not already in* the collection.
- Gemini's unique contributions (vocals in intro/outro, energy 1–10, genre) are
  **absent** from the delivered comments because no Gemini run completed.
- madmom BPM can be double/half-time (flagged via `tempo unstable` only when CV is
  high; Moonshine's 171 BPM was not caught). Windows route avoids this.
- `--end-loop` places the loop on the last phrase aligned to the drop's 4-bar grid,
  not at a musically chosen point; it's a utility for hard endings.
- The spectrogram reads are one reader's judgement; ~23 tracks are `confidence low`.

## 8. Suggested next steps

1. ~~Install, import, dry run~~ done 2026-09-19. Remaining: **close rekordbox** and run
   `autocue_rb.py --folder "%USERPROFILE%/Music/library" --gemini claude_spec.json
   --prefer-gemini --second-drop --end-loop 8 --write-comments` (no playlist exists;
   `--folder` selects all 122). Then enable Phrase analysis in rekordbox, re-analyse,
   and re-run to get the phrase cross-check flags.
2. Listen to the 42 CHECK tracks (review.csv) and correct in rekordbox.
3. If a Gemini pass is still wanted: run `gemini_check.py … --hybrid --until-done`
   overnight from a detached shell (`setsid nohup … &`), one worker; expect quota
   sleeps. Then regenerate with both `--gemini gemini.json --gemini claude_spec.json`
   for a 3-way vote, and `tune.py` against `gemini.json` to re-check thresholds.
4. If a paid Gemini API tier becomes available, use the API path exclusively
   (`ask_api`, no agent) — it was fast, deterministic and accurate in the one test.
5. Consider a third independent detector (e.g. onset-density / kick-presence) to
   break 1-vs-1 ties without an LLM.

## 9. Operational notes for agents

- `pkill -f <pattern>` matching your own shell's command line kills the shell (exit
  144). Use `pgrep -f "^\.venv/bin/python -u gemini_check"` (anchored) or PIDs.
- Long jobs: `setsid nohup … > log 2>&1 < /dev/null & disown`, and use `python -u`
  or the log looks empty.
- Don't put anything runnable (venvs, scripts) near files you hand to `agy`.
- `--dangerously-skip-permissions` on `agy` is blocked by the Claude Code classifier.
- The Gemini API key lives only in `~/.config/autocue/gemini_api_key` (0600).
