# Autocue agent continuation handoff

Last updated 2026-10-19. Workspace: `%USERPROFILE%\Music\autocue`. This is the **current handoff** for an agent continuing the user's Windows DJ preparation app. The old September handoff is preserved in [docs/HANDOFF_2026-09.md](docs/HANDOFF_2026-09.md). Do not treat its Linux environment, download instructions or old cue reports as current operating state.

## Read first

1. [Complete requested project plan](docs/PROJECT_PLAN.md): durable specification with requirement IDs, phases 0 through 10, interfaces, milestones and acceptance criteria. It is a consolidated restatement of the user's plan, not a completion claim.
2. [Plan-to-implementation map](docs/PLAN_PROGRESS.md): current baselines, source/evidence scope and unclosed requirements.
3. [Implementation status](docs/IMPLEMENTATION_STATUS.md): measured results and remaining production gates.
4. [Desktop verification](docs/DESKTOP_VERIFICATION.md): dated native UI results, past failures and subsequent resolutions.
5. [Desktop operation](docs/DESKTOP.md) and [Windows runbook](docs/WINDOWS_RUNBOOK.md): build/use/recovery.

The user wants implementation and verification continued until the intended scope is fulfilled. They value accurate claims, context-rich DJ advice, persistence and usable Windows workflows. Do not restart the project, replace it with an autonomous DJ, or mistake a source fixture/model smoke test for musical verification. This handoff request itself is documentation work; it does not authorize new uploads, credential handling or live-library writes.

## Current build and recorded evidence

| Item | State as of 2026-10-19 |
|---|---|
| Version | 0.2.0 engineering candidate, not full production acceptance |
| Verified installer | `release-0.2.0-complete-verification/Autocue Setup 0.2.0.exe` |
| Installer identity | 796,752,056 bytes; SHA-256 `e558bb847222c46b6fee9adadf171d97c5fd807ec92e0a3bc5b508743d880b6e` |
| Installed executable | `%USERPROFILE%\AppData\Local\Programs\Autocue\Autocue.exe` |
| Automated record | Installer source: 93 Python, 26 desktop, build. Current source (later 2026-10-19, not yet packaged): 101 Python, 30 desktop, build. See IMPLEMENTATION_STATUS "Source continuation after the installer" |
| Installed payload | All 25 Python source files and app.asar match the tested manifest |
| UI archive hash | `1d09af8e1b9acee5f93bfd966d8b0f86a92ba5e35fa3a8c73b3a2964fa826250` |
| Native analysis versions | `native-analysis-3`; resolved result requires `structure-resolution-2` |
| Active workspace acceptance | 160 tracks, 160 aliases, five historical imported revisions, zero edits, three paused jobs, 105 artifacts, zero approved cues |
| ChatGPT acceptance | Retained signed-in account; model discovery returned GPT-6.1-Sol; no token plaintext exposed |
| Multi-track acceptance | Losing It → Stop It → La Danza, filter/cut, 593.4328450249433 seconds (9:53), key lock and ±6% constraints, uncertainty visible |
| Citation acceptance | Eight stored web claims exactly match retained consulted URLs; Stop It visibly retains actual DIRTYBIRD/Bandcamp URL and exact-edit uncertainty |
| Deep smoke | Five CPU sources executed on generated 24-second audio, 1,833.34 seconds with cached native features; no musical-accuracy conclusion |
| Live writes/uploads | No live rekordbox writes or automatic audio uploads credited |

The 77 research-source count describes the completed request's consulted pages, not 77 corroborating citations per claim. The historical La Danza file has conflicting filename/embedded artist identity; do not claim exact edit identification. Historical imported profiles/cues remain unverified/proposed. Saved plan validation checks timing/constraints, not subjective quality. In the installed 0.2.0 build, set-brief UI fields can reset to defaults on a new session. Current source persists the brief and adds "Use this plan's brief", but this needs a rebuilt installer and visible acceptance.

### Evidence files (local, excluded from Git)

- `.artifacts/quality-report.json`: aggregate engineering record; contains separately dated evidence, not a fresh rerun of every accepted case.
- `.artifacts/package-verification-continuation-20261019.json`: completed approved installer UI and installed/extracted payload comparison.
- `.artifacts/continuation-source-manifest-20261019.json`: exact source hashes.
- `.artifacts/post-installer-workspace-verification-20261019.json`: integrity and exact full rows across all seven tables before/after upgrade.
- `.artifacts/live-adviser-verification-20261019.json`: citation and independently validated set evidence.
- `.artifacts/installed-resolution-smoke-20261019.json`: actual bundled Python silence/no-cue/absent-event smoke and SQLCipher availability.
- `.artifacts/real-transition-preview-verification-20261019.json`: local real-source filter/cut render synchronization and peak evidence.
- `.test-data/post-installer-workspace-20261019.db`: native full-history export after approved install.
- `.test-data/citation-multitrack-live-workspace-20261019.db`: pre-upgrade snapshot; exact table comparison passed.
- `.test-data/real-account-acceptance.json`: scoped live-account acceptance.

These are current-machine paths, not portable release assets. If missing on another machine, say evidence is unavailable; do not recreate successful results. Older release directories are superseded; `release-0.2.0-citations-multitrack` is an interrupted partial output and must not be used. Latest documentation-only changes do not require an installer rebuild. New code changes do.

## Ordered continuation queue

1. **Reconcile the detailed specification before claiming completion.** Use PROJECT_PLAN IDs and PLAN_PROGRESS to audit the exact area being worked on. The current map is phase-level; baseline implementations do not prove every requested nuance. Turn uncovered items into concrete source/test tasks. Preserve all existing work and provenance.
2. **Advance independent engineering gaps in isolated data.** Expand crash/pause/retry coverage at every heavy stage, full-disk/cache/download failures, portable snapshot import/external cache relocation, keyboard/native-dialog/accessibility coverage, and complete missing local learned character estimators or clearly bounded substitutes. Start with the smallest reproducible gap, implement it and verify appropriately. Do not spend hours rerunning unchanged deep models.
3. **Prepare/execute benchmark without inventing ground truth.** Group duplicate songs/versions in the 128-profile historical corpus, prepare the 60/20/20 split, use benchmark annotation/UI helpers, collect actual listening labels for beats/events/key/windows/energy. Existing spectrogram/LLM opinions are provisional. Run baseline/candidate ablations on held-out groups, report missing/invented events and runtime/resources. No +5/+10 improvement or calibrated probability claim without measurements. Human judgments cannot be fabricated by another model. Work on other gaps while labels are unavailable.
4. **Prepare real rekordbox acceptance on a small test collection.** Inspect destination/schema and prepare a consistent test backup with exact approved cue/playlist preview. Verify D, colors, manual/edited-generated preservation, memory/hot loops, comments, limits, short intros/hard endings; rehearse complete restoration with rekordbox closed. Current active library has zero approved cues. Do not approve or write them on the user's behalf. Automated encrypted fixtures passed but cannot close real-app acceptance.
5. **Broader integration/release acceptance.** Restricted/expired/switch/refresh real-account cases, long real tracks, clean-machine installation, uninstall/rollback in an isolated Windows environment, subjective preview/separation/loop/transition quality. CUDA is unverified on this CPU-only machine. Revalidate end-to-end and installer after code changes.
6. **Update evidence and handoff after each meaningful result.** Record test source/build identity, exact completed UI actions, failed/skipped cases and explicit remaining gates. Do not count older successes for changed code without a justified dependency check.

This queue does not add public hosting, commercial distribution, macOS, music acquisition, live deck control or autonomous performance. Those remain out of scope. Some remaining work needs human listening or a suitable isolated real-app environment; do not label those complete solely because independent coding has finished.

## Source and test map

| Concern | Main files | Tests / UI |
|---|---|---|
| Shared boundary and dispatch | `autocue_app/models.py`, `cli.py`, `rpc.py`, `service.py` | `tests/test_store.py`, `test_planning.py`; `desktop/rpc.test.ts` |
| Identity, revisions, edits, jobs | `store.py`, `workspace.py`, `processes.py`, service job methods | `test_store.py`, `test_jobs.py`, `test_workspace.py`, `test_processes.py` |
| Features/rhythm/structure/evidence | `analysis.py`, `deep.py`, `structure.py`, `evidence.py`, `autocue_core.py` | `test_analysis.py`, `test_core.py`; `worker/deep_worker.py` |
| Keys/energy/benchmark | `tonal.py`, service calibration methods, `benchmark.py`, `metrics.py` | `test_analysis.py`, `test_benchmark.py`; `ui/BenchmarkReview.tsx` |
| Deep setup/assets | `worker/requirements-*.lock`, `setup*.sh`, `models/manifest.json` | `ui/ExperimentalModels.tsx`; generated CPU smoke records |
| Transition/set/preview | `catalogue.py`, `transitions.py`, `planning.py`, `preview.py` | `test_planning.py`, `test_preview.py`; `ui/PlanCard.tsx`, `DualAudition.tsx` |
| Rekordbox snapshot/write/restore | `autocue_app/rekordbox.py`; historical wrapper `autocue_rb.py` | `tests/test_rekordbox.py` |
| ChatGPT/citations | `desktop/auth.ts`, `stream.ts`, `citations.ts`, `main.ts`; service `ai_context`, `enrichment`, `import_plan` | desktop account/auth/stream/citations tests; `tests/test_enrichment.py`; `ui/EvidencePanel.tsx` |
| Desktop review/library | `ui/main.tsx`, `CueReview.tsx`, `StructureEditor.tsx`, `library.ts`, `LibraryFilters.tsx`, `WorkspaceRecovery.tsx` | `ui/library.test.ts`; native Computer Use acceptance |
| Packaging | `package.json`, `scripts/bootstrap.ps1`, `scripts/package-runtime.ps1`, `Start-Autocue.cmd` | package/source manifests and installer UI record |

### Recent regressions to preserve

- Grid-only rekordbox seeds no longer suppress historical-analysis import; missing historical key fields preserve metadata.
- Citation extraction includes incremental annotations, completed research items and final completed output; incomplete streams rejected. Original unconsulted citations stay in audit while claims become unverified. Batch validation and save are atomic.
- Multi-track context includes route seeds with carried outgoing rates and all referenced technique options; returned plans use original brief/revisions.
- **Actual outer BEGIN IMMEDIATE** protects per-track savepoints. Releasing an outermost SQLite SAVEPOINT previously committed before the final rekordbox-running check. Do not remove the outer transaction or weaken the fault-injection regression.
- Near-zero absolute signal with a supplied grid produces silence/absent events and no cues, rather than a fabricated bar-zero groove. Resolution version invalidates earlier conclusions while native feature cache remains reusable.
- Preview verifies actual source fingerprints before returning cached renders.
- Planner and alternatives pass the current entry point as `min_out_start` to `options()`. Do not go back to filtering after the top-three limit; that dropped valid routes (`test_late_entry_searches_later_exit_windows`).
- Job checkpoints must not overwrite a pause requested by `job_control`. ENOSPC/EDQUOT pauses the job rather than failing every remaining track.
- Electron package must include `jose` 6.1.0. An earlier manual repack omitted it and failed startup; final tested installer includes it.

## Runtime and reproducible commands

Read DESKTOP.md for prerequisites. Native source environment is managed Python 3.11 via uv; Node 24; FFmpeg on PATH; deep worker is WSL Ubuntu with isolated pinned dependencies. A native `torch:false` diagnostic does not mean WSL models are unavailable. First-use Numba compilation can take minutes; inspect evidence before assuming a hang.

Run from the repository root in PowerShell:

```powershell
& .venv/Scripts/python.exe -X utf8 -m autocue_app --help
& .venv/Scripts/python.exe -X utf8 -m pytest -q -p no:cacheprovider --basetemp .test-data/handoff-NEW-UNIQUE-ID
& 'C:/Program Files/nodejs/node.exe' 'C:/Program Files/nodejs/node_modules/npm/bin/npm-cli.js' test
& 'C:/Program Files/nodejs/node.exe' 'C:/Program Files/nodejs/node_modules/npm/bin/npm-cli.js' run build
```

Replace the pytest suffix with a fresh path. For isolated CLI data put `--home C:/chosen/test-workspace` before the operation. `analyze` waits for completion; desktop jobs provide interactive control. Use `scripts/quality-report.py` for fresh Python results; it preserves prior acceptance fields and reads saved desktop JSON, so **does not rerun desktop tests, models or UI acceptance**. Match test scope to changes.

Codex's default shell failed with OS error 87 after a host restart. If needed, pass explicit `C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe`, `login:false`, and the workspace directory to the shell tool. Git may require per-command `git -c safe.directory=%USERPROFILE%/Music/autocue ...`; avoid changing global settings.

The repository has a baseline commit and extensive modified/untracked implementation. Do not reset, clean, delete or overwrite it. Inspect `git status` and preserve all source, documentation and private evidence. Do not assume a Git clone alone contains the current work. No commit/PR/push was requested in this documentation handoff. Music, credentials, runtime caches, installers and private evidence stay excluded.

## Windows launch context and data preservation

Ordinary active application data is `%USERPROFILE%\AppData\Local\Autocue`. Packaged Codex children can instead see redirected AppData under `%USERPROFILE%\AppData\Local\Packages\OpenAI.Codex_2p2nqsd0c76g0\LocalCache\Local\Autocue`. The same logical path previously led to an apparent empty library. **Launch the installed app through ordinary Explorer/Windows shortcut**, and audit it with diagnostics plus a fresh native full-history export outside AppData. Shell logical-path reads alone cannot establish active DB identity.

The original redirected preparation copy remains a backup. The ordinary empty pre-migration state is preserved as `Autocue-empty-before-migration-20261018`. Matching encrypted Electron profile metadata was migrated with the account file; old metadata is preserved as `Local State.before-migration-20261018`. Do not move credentials or repeat migration just because a different launch context is empty. Do not print token/account files. Copying an encrypted account file without its matching Electron Local State breaks decryption.

Last visible acceptance left the ordinary installed app in Stop It's evidence view. Treat window handles, account/model availability, counts and foreground state as time-sensitive; reobserve before UI action. Three jobs remain paused deliberately; do not resume/cancel them merely for installer testing.

## Computer Use and approvals

For native Windows verification, read the available computer-use skill and bundled guidance/API/confirmation policy before acting. In this environment it is `%USERPROFILE%/.codex/plugins/cache/openai-bundled/computer-use/26.1013.40934/skills/computer-use/SKILL.md`; discover the current path if that version changes. Use the supported sky API in node_repl, fresh returned windows and the observe → inspect → one action → refresh loop. Do not use shell UI automation/CDP to bypass it. Security/authentication dialogs are handled by the user. Current handoff work uses no desktop input. Other chats have shared the desktop; coordinate availability using only authorized channels and do not overlap active native input.

The user approved and completed the exact complete-verification installer above. No reinstall is needed for these docs. For a newly built installer, follow action-time confirmation requirements in the applicable skill. User sign-in stays user-operated. Do not send messages to other chats or people without direct user authorization. Rekordbox writes require approved cues, exact current preview, closed rekordbox and preservation/backup checks. Do not upload audio automatically.

## Definition of handoff versus completion

The project is ready for another agent to continue from this workspace: the requested specification, progress map, source pointers, commands, verified package, evidence and open work are recorded. **The full product plan is not complete or fully verified.** M3 listening evaluation, M4 subjective transition quality and M6 real rekordbox/restoration plus broader failure/clean-machine acceptance are substantive open gates. Continue implementation and verification without manufacturing labels, approvals or success.
