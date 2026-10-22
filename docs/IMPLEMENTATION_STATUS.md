# OnSet preparation: implementation status

The preparation side of OnSet (version 0.2.0 of the former standalone app) is an engineering candidate. Its screens now run in OnSet's Tauri desktop app; the installer results below were recorded on the former Electron build and need repeating on the Tauri app. The full requested plan is not yet fully verified for personal production. Generated fixtures establish software behavior; they do not establish musical accuracy or mixer equivalence.

Last reviewed 2026-10-19 against the local quality report, package manifest, post-upgrade workspace comparison and current source. Evidence is scoped to the measured machine and fixtures.

The complete specification is [PROJECT_PLAN.md](PROJECT_PLAN.md); [PLAN_PROGRESS.md](PLAN_PROGRESS.md) maps its requirement IDs to current baselines and open work. For continuation commands, source pointers and machine-specific preservation notes, start with [the current handoff](HANDOFF.md).

## Implemented

| Area | Delivered implementation |
|---|---|
| Foundation | Managed Python 3.11, locked native/desktop/core and experimental WSL runtimes, diagnostics, repaired historical CLI regressions |
| Persistence | SQLite identities and aliases, immutable observations, separate human edits, durable jobs, undo, stale dependency invalidation |
| Analysis | Independent rhythm checks; band/onset/timbre/chroma and recurrence features; duration-aware structure baseline; native/Essentia global and local keys; Demucs stems; multidimensional energy |
| Experimental sources | Isolated EDMFormer and all-in-one CPU environments, pinned source/assets, activations/embedding files, finite timing checks and explicit experimental status |
| Review | Waveform and sections, precise cue/loop fields, keyboard adjustments, bulk approval/rejection with single undo, saved-position guard, stem players, metadata/mood/familiarity edits, benchmark labels |
| Library | Standard/Camelot key normalization, BPM/energy/genre/vocal filters, pasted-path import, exact-hash relocation and complete workspace JSON |
| Rekordbox | Consistent SQLCipher snapshots, approved-only exact preview/apply, deletion-only operations, manual/edited cue preservation, explicit historical ownership adoption, manifests/backups/restore and playlists |
| Transitions | Eighteen technique families, multiple entry/exit windows, actual-grid timing, incoming pitch limits, local-overlap harmony/tonal/bass/vocal checks, loop alternatives and supported rehearsal renders |
| Planning | Beam width 128, three alternatives, revision/timing/constraint validation, pins, omission reasons, validated fallback routes and JSON/CSV/HTML/PDF exports |
| ChatGPT | OAuth/PKCE, Windows-protected main-process credentials, account/model selection, completed-event streaming, sourced enrichment and rejected-invalid-plan replacement suggestions |
| Recovery | Owned process cancellation, resumable native-feature stages, queue deduplication, explicit unavailable-model evidence, workspace health guards, consistent snapshots and exact restoration preview, full SQLite history export, checkpoint/quit and recoverable interface errors |

## Current engineering evidence (2026-10-19)

- 93 Python regression tests pass (zero failures; one librosa empty-pitch tuning warning on the silent fixture), including generated audio, encrypted/plain database snapshots, exact loops, cue preservation/deletions, adoption, restore, process cancellation, immutable resources, stale set validation, shutdown pause versus explicit cancellation, queue deduplication, native-stage recovery, workspace snapshots/restore and benchmark metrics.
- 26 desktop tests pass. Tests include locally signed account tokens, PKCE/nonces, refresh/account switching, cancelled sign-in, malformed/interrupted streams, key normalization and missing-evidence filtering, filtered analysis selection, cited Apple song identity and bounded service shutdown. These automated protocol tests use synthetic credentials; separate real-account acceptance is described below.
- TypeScript and production Vite/Electron builds pass.
- Installed 0.2.0 launched through Computer Use and imported 160 live rekordbox tracks. This exposed a zero-duration import bug, now fixed with pre-analysis audio durations, progress events and durable skipped/failed reports. A fresh live-library backend check imported all 160 tracks with durations in 49.63 seconds and no failures or database writes. The user handled the firewall prompt. Installed diagnostics, a fresh 160-record import, advancing/paused local playback and a clean exit/relaunch passed. Filtered analysis now defaults to visible results and disables empty-result actions. Shutdown preserves paused work, verified by installed resume/quit/reopen. The earlier empty-library discrepancy was traced to packaged-host AppData redirection and resolved for ordinary Windows launch; see the desktop verification report. Broader failure/recovery coverage remains open.
- Native-to-WSL CPU inference completed all five requested sources on generated 24-second audio: Beat This!, Demucs, Essentia, EDMFormer and all-in-one. The latest native-analysis-3 resumption run reused native features and completed in 1,833.34 seconds on CPU; reported peak child RSS was 3,422,420,992 bytes. Runtime depends strongly on model loading, caching and concurrent machine work. Experimental outputs remain unverified musical opinions.
- A 128-profile planning run produced three valid alternatives and 26,397 candidate transitions in 46.843 seconds. Later overlap refinements passed regression tests; this time is an observed run, not a universal runtime guarantee.
- Computer Use acceptance is recorded separately in [desktop verification](history/DESKTOP_VERIFICATION.md). Only completed visible workflows are credited there.

The local machine-readable report is `.artifacts/quality-report.json`; regenerate Python checks with `scripts/quality-report.py`. Desktop checks/build, native UI acceptance and model runs are separate evidence and are not rerun by that script. Historical reports are preserved. No live rekordbox library writes have been performed during implementation.

## Remaining requested work and acceptance gates

1. **Listening benchmark and model promotion.** Human-verified beat/downbeat, event, section, key, energy and transition labels are still required. Metrics, one-to-one matching, pairwise energy judgments, split templates and held-out release comparisons now exist. Check duplicate song/version grouping, collect labels, run ablations and assess the +5/+10 point targets. No improvement or calibrated probability is claimed without this evidence.
2. **Real rekordbox inspection and restoration rehearsal.** Verify cue D, memory/hot loops, colours, counts, comments, preservation and playlists in a small test library, then rehearse consistent backup restoration there. Automated database fixtures cannot establish rekordbox's visible behavior. Full-library writes remain approval-based.
3. **Broader ChatGPT acceptance.** User-completed sign-in, account-specific model discovery, plan-funded metadata enrichment, release web research and a validated 210-second single-track plan passed through the installed interface. A later restart retained the account and the saved plan and loaded models again. An earlier post-update research run lacked verified consulted citations and was conservatively downgraded. This citation-retention gap was subsequently repaired: live sourced-claim retention and a validated three-track 9:53 recommendation passed on 2026-10-19 using historical, unverified analyses. The evidence panel retained an actual DIRTYBIRD/Bandcamp source and exact-edit uncertainty. Restricted-policy/expired-token cases still need acceptance; account switching and refresh have isolated protocol coverage.
4. **Broader recovery/performance matrix.** Long real tracks, pause/quit during every heavy stage, interrupted initial downloads, full-disk/cache limits, clean-machine installation, uninstall and rollback need additional destructive-failure testing in isolated environments. Cancellation cannot interrupt every native library call immediately. CUDA remains unverified on this CPU-only machine.
5. **Musical recommendation refinement.** Learned local genre/mood/danceability estimators and learned transition acceptability are not yet delivered. Metadata, manual tags, sourced enrichment, interpretable character features and ChatGPT interpretation are available. Relative energy still needs personal calibration. Wordplay/themes/hooks require verified evidence. Variable-tempo alignment, arbitrary tempo ramps, mixer-specific gestures/effects and listening-based loop/separation quality need further evaluation.
6. **Review and portability polish.** Benchmark ground truth currently uses an editable JSON form. Native full-history Save As export and exact row preservation through the installed upgrade passed. Additional file-dialog cases, restore/import round trips with edited revision history, missing external cache/stem relocation, all keyboard paths and accessibility across display sizes need broader acceptance coverage. All eleven rendered-preview families have generated-audio timing/peak checks; listening quality and the remaining manual gestures still need acceptance. Workspace JSON preserves current profiles, transitions, sets and labels. The new verified SQLite workspace export also preserves revision, correction, undo, job and artifact history. Audio, credentials, model caches and large feature files remain external; importing a portable snapshot from another machine still needs a dedicated workflow.

Complete the end-to-end production gate only after these checks: import, deep analyse, inspect/correct/approve, plan transitions and sets, audition, validate timing, preview/apply/export, then restart with decisions intact. No live-performance control is implemented.

The rebuilt 0.2.0 installer was installed with explicit approval. Windows package AppData redirection inherited from Codex explained the empty normal-launch workspace. The prepared workspace and matching encrypted Electron profile metadata were copied into ordinary AppData while preserving both original states. Explorer-launched account/model access, all 160 tracks, three jobs, the saved plan and a second clean restart passed. A native full-history export passed integrity and full-row comparison across every workspace table. See the verification report for paths and evidence.


## Continuation verification (2026-10-19)

The updated installed application passed real-account model discovery, five-profile release research, citation retention, and three-track set selection through Computer Use. The proposed Losing It -> Stop It -> La Danza route validated at 593.43 seconds with filter/cut transitions, key lock, and the requested pitch limit. Historical observations remain unverified and no cue approvals or real rekordbox writes occurred.

New regression coverage found and repaired an SQLite savepoint escape: explicit outer transactions now protect cue and playlist changes when rekordbox starts before commit. Fault injection verifies no partial cue/comment or playlist mutation. Grid-only rekordbox seeds no longer suppress historical-analysis import, and missing historical key fields preserve imported key metadata. Citation audits survive restart and invalid enrichment batches cannot partially save. Rehearsal renders verify source fingerprints before reusing a cache. All eleven supported preview families passed duration, finite-sample and peak checks on generated audio.

Before packaging, the installed acceptance patch was backed up in `.artifacts/before-citation-patch-20261019`. The current complete-verification installer includes those changes; both its extracted payload and the resulting installed files match the tested source. Full current results are in `.artifacts/quality-report.json`. Musical accuracy, listening calibration and real rekordbox visual/restore acceptance remain separate gates.


A strengthened silence test found that a supplied grid could make digital silence look like a first groove at bar zero. `structure-resolution-2` now classifies near-zero signal as silence, records musical events as absent, and proposes no mix cues. The shared legacy baseline also avoids its bar-zero fallback for silent energy curves. The native feature cache remains version 3; resolved outputs require the new resolution version so old conclusions are recomputed. This is an invented-event regression repair, not a held-out musical accuracy claim.

Expanded rekordbox fixtures also verify optional-loop omission at the memory-cue limit, preservation of eight manual cues, exact sixteen-beat memory/hot loops on a variable grid, and a complete encrypted SQLCipher write/restore round trip. Real rekordbox visual behavior remains unverified.


Final continuation checks pass: 93 Python tests, 26 desktop tests, desktop build, installed bundled-runtime silence analysis and source-hash comparison. The 796,752,056-byte rebuilt installer is `release-0.2.0-complete-verification/Autocue Setup 0.2.0.exe`, SHA-256 `e558bb847222c46b6fee9adadf171d97c5fd807ec92e0a3bc5b508743d880b6e`. The user-approved installer completed through Computer Use on 2026-10-19 and launched the installed app successfully; the resulting installed files match the current tested source. Account/model discovery, retained citation display and the saved three-track plan passed. A fresh native full-history export passed integrity and exact full-row comparison against the pre-upgrade snapshot across all seven tables, retaining 160 tracks, five revisions, three jobs and 105 artifacts. The interrupted `release-0.2.0-citations-multitrack` output is superseded and must not be used.


## Source continuation after the installer (2026-10-19, later)

These changes are in source only and are **not in the verified 0.2.0 installer**. A new installer build and the action-time install confirmation are needed before installed-app credit. Automated record for this source: 101 Python tests, 31 desktop tests, and a successful TypeScript/Vite/Electron build. The UI changes have unit-test and typecheck evidence only; they have not been exercised through Computer Use.

- **Planner late-entry bug (P10.2, P7.2).** `options()` keeps one best window per technique and then the top three. When a set entered a track after an early exit window, every top option could start before the entry point. The route was then dropped, and the planner reported "no valid set" even when a later exit window worked. `options(min_out_start=...)` now searches only playable windows. The planner, `replan_order` and `transition_alternatives` use it. Regression: `test_late_entry_searches_later_exit_windows`.
- **Unsatisfiable briefs (P10.2).** Conflicts now name the cause before the generic message:
  - an opening or closing track that is excluded or filtered out;
  - an unknown or unavailable pinned track, or a pin position beyond the candidates;
  - an identical opening and closing track;
  - candidates too short for the minimum duration.
- **Job control (P1.3, V1).**
  - A pause requested between checkpoints is no longer overwritten with `running` when a track completes.
  - A full disk or quota error (ENOSPC/EDQUOT) pauses the job with a clear error. It no longer fails every remaining track.
  - Resuming a job whose worker has already finished re-enqueues it instead of setting an event nobody waits on.
- **Import boundaries (I2, P1.2).**
  - Workspace JSON transitions, plans and benchmark labels are checked for required IDs and `schema_version` before anything is written, so a malformed file can't partly import.
  - An explicit file with an unsupported type (for example `.db`) is now reported as failed instead of being silently ignored.
  - Key edits must be standard notation or Camelot (zero-padded forms such as `08A` are accepted).
- **Restore (P6.2).** A missing backup file, or a future write-manifest schema, is refused with a clear `ValueError` before any mutation.
- **Desktop (P5.2, P10.2).**
  - The set brief is saved in the `set_brief` setting, which is validated by `SetBrief.parse`, and restored on the next launch. Removed tracks are pruned from it.
  - Each plan card has a "Use this plan's brief" action. This closes the known issue where brief fields reset to defaults in a new session.
  - Cue dragging snaps to the corrected grid, matching the keyboard steps.
  - A cancelled pointer can no longer leave a stuck cue drag.
  - Ctrl+A and Ctrl+R no longer approve or reject a focused cue, and Ctrl+Z works after clicking a button.
  - Settings loads now report errors, and a failed experimental-model toggle rolls back.
  - Icon-only and row controls have accessible names.

- **Interface redesign (P5.1–P5.2).** The theme is new: a neutral graphite palette with one violet accent, semantic status colours, Segoe UI Variable, and consistent 34/28 px controls. Icons are inline SVG. Track review now uses sub-tabs instead of a 12-panel scroll.
  - Removed decorative text: eyebrows, taglines, sidebar slogan and footer.
  - Removed duplication: the second ChatGPT account block, the repeated raw JSON on Track review, the per-card "Audition required" pills and the static 18-family catalogue list.
  - Some readouts are now readable tables or key/value lists instead of raw JSON: doctor output, workspace diagnostics, key estimates and grid evidence. Raw analysis remains under Corrections → Raw analysis data.
  - Fixed squashed character curves and cue chips that had lost their colours.
  - Spectrogram frequency labels now sit at their actual bin rows.
  - The waveform redraws when resized.
  - Notifications are toasts, and a top progress bar shows activity.
  - Must-play and Exclude use chip pickers instead of tall multi-selects.
  - All write, restore and approval safeguards are unchanged.
  - Evidence: TypeScript build, 31 desktop tests, and a visual pass over every page in the browser preview with the dev sample bridge at 1440 and 1000 px. It is installed from `release-0.2.0-redesign-2`. Installed files match the build, and the user checked the UI visually and approved it.

Spectrogram frequency ticks from bin coordinates were completed in the interface redesign above. The rest of that audit list is closed in the next section.


## Audit gaps closed in source (2026-10-19, latest)

Source only. These changes are **not in the verified 0.2.0 installer**. Automated record: 118 Python tests (twice in a row), 33 desktop tests, typecheck and production build. The UI was checked in the browser preview with the development sample bridge, not in the installed Electron app or against real music.

- **Vocal clashes (P7.1, P3.1).**
  - Analysis now stores per-bar vocal activity from the separated vocal stem (`vocal_activity`).
  - Transitions sample both tracks across the actual overlap. More than 4 s of simultaneous vocals removes the option; 1–4 s adds a warning.
  - Cuts warn when the outgoing vocal is cut mid-phrase or the incoming track has a vocal pickup before the downbeat.
  - Without stems, the earlier window-level rule and the "unverified" warning still apply.
  - Thresholds are heuristics, not listening-verified.
- **Mix windows (P7.2, P3.3).**
  - `structure.mix_windows` marks spans of at least 8 bars with steady percussion, little vocal and enough runway before the next strong boundary.
  - The two best become extra entry/exit candidates. Corrected sections take precedence and suppress them.
  - Mix windows that end close to the next major change add a "little runway" warning.
- **Deep results cache and resolution version.**
  - `structure-resolution-3` invalidates earlier resolved results.
  - Completed deep model outputs are reused from `deep-results.json`, or from an older `result.json`, so this version change does not rerun Demucs, Beat This! or other models. Models rerun only if their files are missing.
- **Set brief (P10.1).**
  - New fields: `smoothness` and `contrast` (0–1). Only their difference shifts scores, so the defaults leave earlier rankings unchanged.
  - `playlists`: rekordbox playlist names, read during the rekordbox import (normal playlists only).
  - `transpose`: whole-semitone key shifts per track, from −12 to +12.
  - Transposes affect key and overlap compatibility, appear as deck actions, and are checked by `validate_plan`.
- **Planning (P10.2).**
  - Pinned transitions are now forced during the search, not just checked afterwards. Unknown pins are reported.
  - `replan` keeps the first N tracks and transitions of a saved set and searches the rest.
  - `plan-job` and `transitions-job` run the searches as cancellable light jobs; the synchronous `plan` and `transitions` calls remain.
- **Review queue (P5.2).**
  - `review-queue` ranks tracks by stale cues, unapproved D/A/E/F cues, source disagreements on events, analysis flags, grid instability, weak keys and use in saved sets. Each rank comes with written reasons.
  - The Library "Review queue" view sorts by this ranking and shows the main reason.
- **Portability (I2, P1.2).**
  - `import-workspace` accepts a workspace JSON or SQLite export from another machine, applies old-to-new folder mappings, and still matches every file by checksum.
  - From SQLite exports it also restores revision and edit history.
  - `relocate-cache` rewrites stored feature, stem and model-output paths after you move the cache folder, and reports files that are missing.
- **Benchmark labels (V2).** A structured form replaces raw JSON: event state and bar, "Use playhead", acceptable keys, true BPM and notes. Grid samples, boundaries and sections stay as advanced JSON.
- **Atomic writes.** On Windows, `os.replace` is retried briefly when the file is held. One full-suite run hit a transient "Access is denied" on a workspace marker.

**Package build (2026-10-19, latest).**
- Installer: `release-0.2.0-audit-gaps/Autocue Setup 0.2.0.exe`, 212,965,009 bytes, SHA-256 `70c20400fb5a29d0292eb94a0adc29b9c6cd39530f9cfc00481586b3116f209e`.
- Size: the unpacked app is 822 MB, with a complete 446 MB runtime. The smaller installer reflects NSIS compression; the earlier 796 MB build was nearly uncompressed.
- Payload: `scripts/verify-package.py` matched all 51 packaged Python, worker and app-archive files to the tested source. app.asar SHA-256 is `69fef91c9c9c16ec0abcda9aa16a5209b1e49b6215beae65a88d579f8a9ec67a`. Record: `.artifacts/package-verification-audit-gaps-20261019.json`.
- Smoke test: `scripts/package-smoke.py` used the packaged runtime and packaged sources on generated audio in an isolated workspace. All 13 checks passed: light analysis, mix windows, resolution version, review queue, plan job, transpose carry, vocal-overlap field, prefix replan, pinned-transition search, plan-job cancel, cache relocation, and SQLite/JSON workspace import with remapping. Record: `.artifacts/package-smoke-audit-gaps-20261019.json`.
- **Installed and visibly checked on 2026-10-19** with user approval. The installed payload matches the source, and every listed feature check passed. Findings: a key shift on a kept track blocks replanning; cancelling a large search takes about 20 s; waveform seeking left the playhead at 0:00 (outside these changes). Details and side effects are in [desktop verification](DESKTOP_VERIFICATION.md#audit-gaps-installer-acceptance-2026-10-19).
- The redesign build in `release-0.2.0-redesign` predates these changes and has no recorded verification; do not use it.

**After installed acceptance (source only).** Three acceptance findings are fixed in source and covered by regressions: replan after a key shift, set-search cancel latency, and the set-search progress label. They are listed in [desktop verification](history/DESKTOP_VERIFICATION.md). 120 Python tests, 33 desktop tests and the build pass. Waveform seeking (finding 3) is also fixed: local audio is now served with byte ranges, and the fix was checked in a development Electron build with generated audio. These fixes need a rebuilt installer before they count as installed.

## Genre detection (2026-10-19, source only)

Genre now has three sources, shown side by side on Track review → Profile. Only your own tags count as labels.

- **Audio model.** Deep analysis runs Essentia's Discogs-EffNet (`discogs-effnet-bsdynamic-1`, ONNX) in the WSL worker. It stores the top 10 of 400 Discogs styles as unverified `genre_estimates`, plus a 1280-value sound embedding.
  - The model files are fetched once at runtime, with retries, and must match pinned SHA-256 hashes.
  - The licence is CC BY-NC-SA 4.0: fine for personal, non-commercial use. The files are not redistributed.
  - The worker lock adds `onnxruntime==1.20.1` and its dependencies only. Existing pins are unchanged, and `imageio-ffmpeg` is now also in the non-CPU lock.
  - Deep results cached earlier are reused only if they include every required model, so existing tracks fetch just the genre result.
- **Suggestions from your tags.** `genre-suggestions` compares each untagged track with the tracks you tagged (`overrides.genres`). It uses the EffNet embedding, or, without deep analysis, a native timbre/rhythm summary; the two are never compared with each other. Each suggestion names the similar tracks behind it. Imported or researched tags are not training labels until you confirm them.
- **Sourced research.** The existing ChatGPT enrichment with online research and citations.

Evidence:
- 123 Python tests, including label splitting, nearest-neighbour suggestions in both representations, and the updated required-model set.
- 35 desktop tests and the build pass.
- The worker function ran in WSL on generated audio and on four library tracks, about 3–6 s each on CPU. Results: Darude - Sandstorm → Trance 0.61; Daft Punk - One More Time → House 0.38 and Disco 0.16; Chris Lake & Green Velvet - Deceiver → Bassline 0.74 and House 0.32; &ME & Black Coffee - The Rapture Pt.III → Deep Techno 0.43 and Deep House 0.42.
- A native-to-WSL round trip on isolated generated audio returned a complete result with a mapped embedding path.
- The Profile card was checked in the browser preview with sample data.

These are not accuracy measurements, and genre still needs labels checked by ear. Mood and danceability estimators remain undelivered.

Still open: learned local mood/danceability estimators and learned transition acceptability. Both need listening labels or model assets that have not been gathered. Real-library behaviour of every item above also still needs checking.

## Live-show performance (2026-10-20 to 22)

The goal is that preparation work cannot disturb a show, and that the app beside the visuals costs as little as possible. Changes, each with what was measured:

- **Show mode (plan O4), `61f5da8`.** The service's `show-mode` pauses running and queued jobs at their next checkpoint and remembers them. It refuses to resume jobs and refuses rekordbox writes and restores until the show ends, then resumes exactly the jobs it paused. The desktop's `show_mode` command also drops the service to below-normal priority with Windows' efficiency mode, and restores both afterwards. Tested by `service/tests/test_show_mode.py` (4 tests) and a priority-class check on a real child process. It is not yet switched on automatically: today "Start show" happens inside the separate visuals program, which the app cannot see.
- **Thread priority, `408c590`.** The engine's 120 Hz tick and the visuals' render loop run above normal priority; the audio callback was already time-critical.
- **Live view, `99b0041`.** Updates go out only when something shown changes, plus a resync every half second while playing. Nothing is sent once every window has been hidden for a second. The screen carries the playhead, beat and cue countdown forward from the tempo. With the simulator at 124 BPM, updates fell from 28 to 2.1 a second.
- **Saved sets, `7581d65`.** Listing saved sets loaded all 58,844 stored transitions once per set, so the call took 393 s on a copy of the real workspace. Because the service answers one request at a time, every refresh waited behind it. It now takes 3.3 s, with identical verdicts for all 18 saved sets.
- **rekordbox scans, `93aa400`.** rekordbox builds the player objects that hold a track's content id only for playing decks. With decks loaded but paused, the reader repeated a full heap scan every 30 s and listed every handle in the system every 1.5 s. With rekordbox idle, the installed app spent about 40% of a core on this. Both now run only for decks that play without an id. Against the same idle rekordbox, the file watcher fell from 1,594 ms to 31 ms of CPU a minute, and no deck scans followed the ones at connect.
- **Show bench, `100f64f`.** `onset --bench N --bench-pace show` keeps vsync as in a show, `--frame-latency 1|2` sets the queue depth, and the bench prints how old the music state was at each present.

**Visuals, measured:** release builds of `1361190` (before this work) and current source, each with the bench patch, 60 s per run, scene `crystal`, simulator playing Stop It, on the laptop's 2256×1504 screen, alternating builds over two rounds:

| Run | Mean frame | p99 | Over 20 ms | State age mean / p99 | CPU (all cores) |
|---|---|---|---|---|---|
| Before, 2 frames | 18.8 / 28.5 ms | 46 / 70 ms | 13% / 99% | 6.0 / 8.5 ms, 14 / 26 ms | 0.8–1.2% |
| Current, 2 frames | 21.0 / 23.7 ms | 52 / 66 ms | 40% / 82% | 6.6 / 7.9 ms, 17 / 24 ms | 0.9–1.3% |
| Current, 1 frame | 36.1 / 27.6 ms | 69 / 76 ms | 100% / 92% | 8.2 / 6.4 ms, 22 / 17 ms | 0.7–0.9% |

The GPU is the limit at this resolution: frame times varied more between two runs of the same build than between builds, adaptive resolution kept changing the load, and an installer build was compiling at the same time. The CPU-side changes cannot show up here; they protect against stalls when other work competes. One queued frame made frames slower without fresher data, so two stays the default. A 1080p projector and a run under load (app open, analysis queued) are still to be measured.


## Evidence map

| Record | What it establishes |
|---|---|
| `.artifacts/quality-report.json` | Recorded engineering suites, diagnostics and separately dated acceptance records |
| `.artifacts/package-verification-continuation-20261019.json` | Exact installer identity, extracted/installed source comparison and completed installer UI acceptance |
| `.artifacts/post-installer-workspace-verification-20261019.json` | Native export integrity and exact before/after rows across all seven workspace tables |
| `.artifacts/live-adviser-verification-20261019.json` | Retained consulted URLs and independently validated three-track timing |
| `.artifacts/installed-resolution-smoke-20261019.json` | Bundled Python silence analysis, absent events/no mix cues and SQLCipher availability |
| `.artifacts/real-transition-preview-verification-20261019.json` | Real-source render duration, finite samples, action offsets and peak checks |

The 77-source research count covers consulted pages in the completed request; it is not 77 supporting citations per track. Eight stored web claims retained exactly consulted URLs. Citation retention is provenance evidence, not independent factual or exact-edit verification.
