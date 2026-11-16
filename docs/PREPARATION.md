# Preparing a set in OnSet

OnSet's preparation screens are for analysing tracks, reviewing cues, comparing transitions and preparing DJ sets. The DJ selects and performs the music. The app does not control live decks or perform a set autonomously.

Version 0.2.0 is an engineering candidate. Software checks and installed-app acceptance passed on the development computer for the specific builds named in the dated records below; they have not all been repeated on this branch's final tip (see [verification scope](#verification-scope)). Musical accuracy and real rekordbox cue/loop/restore acceptance remain open. See [implementation status](IMPLEMENTATION_STATUS.md) for the remaining work.

For another agent continuing development, start with the [current handoff](HANDOFF.md), [complete requested plan](PROJECT_PLAN.md) and [plan progress map](PLAN_PROGRESS.md).

## Start

Open **OnSet** from the Start menu or desktop shortcut. Workspace data lives in `%LOCALAPPDATA%\Onset\Onset\data\workspace`; music stays in its existing locations. FFmpeg must be on PATH; local deep analysis also requires the WSL2 worker setup. To build from source, follow [setup and build instructions](DESKTOP.md).

## Prepare a set

1. Import music, historical analysis JSON, or read a consistent rekordbox snapshot.
2. Analyse a small selection, inspect source evidence and uncertainty, then correct and approve cues. Cue D means the first sustained full groove; estimates are provisional.
3. Compare transition windows and instructions in Transition lab. Render supported rehearsal previews locally on request.
4. Set the duration, candidate tracks, energy arc and equipment constraints in Set planner. Generate local alternatives or request ChatGPT advice. All returned timing and IDs are validated.
5. Inspect the exact cue/playlist write preview. Close rekordbox before applying a reviewed write, and inspect a small test collection before considering a full-library write.

ChatGPT receives selected profiles and derived context for user-initiated requests. Audio remains local unless deliberately selected for a separate audio-provider upload. Online research retains consulted source URLs and claim-level citation audits; unsupported claims remain visibly unverified.

## Verification and documentation

### Verification scope

The documents below are dated engineering records, written between 2026-10-18 and 2026-10-24 while the branch was still changing. Each result belongs to the build it names. A feature that appears in this branch has not necessarily been verified on the final branch tip, and a fix that was packaged has not necessarily been installed or accepted.

| Date | Build | What was checked there |
|---|---|---|
| 2026-10-18 to 19 | Former standalone Electron app (`Autocue Setup 0.2.0.exe` builds) | 93 Python tests, 26 desktop tests and the production build; Computer Use checks of those installers, account/model discovery, retained citations and a validated three-track 9:53 set; post-upgrade workspace export comparison. See [desktop verification](history/DESKTOP_VERIFICATION.md). |
| 2026-10-19 to 22 | Installed Tauri builds `04fe8d2`, `93aa400` and `3a739ac` | First OnSet installers, the rekordbox 7.2.19 calibration, the [demo](DEMO.md) walkthrough (2026-10-20) and live-show performance measurements. See the [handoff](HANDOFF.md) and [implementation status](IMPLEMENTATION_STATUS.md). |
| 2026-10-23 | Installed `aa791f3` | The demo's visuals steps, now inside the app, and the calibration panel. |
| 2026-10-24 | Installed `abec26f` | Installed payload, exact workspace preservation across the upgrade and deep analysis of four real tracks. See the [continuation record](CONTINUATION_2026-10-24.md). |
| 2026-10-24 | `3630ea4` (packaged, **not installed**) | Cue-audition seek fix: 39 UI tests and the production UI build passed and the installer was built. Installation and visible acceptance are still pending; the installed `abec26f` does not include this fix. |
| Final branch tip | This pull request | Only the automated CI checks: UI install, type-check and build; `cargo fmt`, `cargo clippy -D warnings`, `cargo test` and the release build on Windows. No installed-app, listening or real rekordbox checks have been run on the tip. |

Since `3630ea4` the branch has changed only documentation, formatting, one test assertion and the CI workflow. None of these records establishes listening quality or real rekordbox write and restore behavior.

| Document | Purpose |
|---|---|
| [Demo](DEMO.md) | Ten-minute walkthrough of preparation, the live link and the visuals |
| [Desktop operation](DESKTOP.md) | Daily workflow, accounts, analysis, builds, shared CLI and recovery |
| [Windows runbook](WINDOWS_RUNBOOK.md) | Current installation, preparation and write/restore procedure |
| [Implementation status](IMPLEMENTATION_STATUS.md) | Implemented scope, evidence and incomplete acceptance gates |
| [Desktop verification](history/DESKTOP_VERIFICATION.md) | Dated Computer Use results from the former Electron build |
| [Legacy CLI](history/LEGACY_CLI.md) | Historical scripts and their separate write behavior |
| [Agent handoff](HANDOFF.md) | Current repository, build, source map, commands and ordered remaining work |
| [Complete project plan](PROJECT_PLAN.md) | User-requested requirements, phases, milestones and acceptance criteria |
| [Plan progress map](PLAN_PROGRESS.md) | Implementation/evidence coverage and open work by requirement ID |

The September handoff, session log, cue tables, spectrogram opinions, Gemini attempts and download notes describe one personal music library and stay in the private repository they came from; they are not part of this repository. Automatic music acquisition is outside this release.
