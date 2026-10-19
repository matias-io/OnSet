# Preparing a set in OnSet

OnSet's preparation screens are for analysing tracks, reviewing cues, comparing transitions and preparing DJ sets. The DJ selects and performs the music. The app does not control live decks or perform a set autonomously.

Version 0.2.0 is an engineering candidate. Software checks and installed-app acceptance pass on this computer; musical accuracy and real rekordbox cue/loop/restore acceptance remain open. See [implementation status](IMPLEMENTATION_STATUS.md) for the remaining work.

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

The latest recorded checks pass 93 Python tests, 26 desktop tests and the production build. Computer Use verified the current installer, account/model discovery, retained citations and a validated three-track 9:53 set. A native post-upgrade export passed integrity and exact comparison of every workspace table. These checks do not establish listening quality or real rekordbox write behavior.

| Document | Purpose |
|---|---|
| [Desktop operation](DESKTOP.md) | Daily workflow, accounts, analysis, builds, shared CLI and recovery |
| [Windows runbook](WINDOWS_RUNBOOK.md) | Current installation, preparation and write/restore procedure |
| [Implementation status](IMPLEMENTATION_STATUS.md) | Implemented scope, evidence and incomplete acceptance gates |
| [Desktop verification](history/DESKTOP_VERIFICATION.md) | Dated Computer Use results from the former Electron build |
| [Legacy CLI](history/LEGACY_CLI.md) | Historical scripts and their separate write behavior |
| [Agent handoff](HANDOFF.md) | Current repository, build, source map, commands and ordered remaining work |
| [Complete project plan](PROJECT_PLAN.md) | User-requested requirements, phases, milestones and acceptance criteria |
| [Plan progress map](PLAN_PROGRESS.md) | Implementation/evidence coverage and open work by requirement ID |

The September handoff, session log, cue tables, spectrogram opinions, Gemini attempts and download notes describe one personal music library and stay in the private repository they came from; they are not part of this repository. Automatic music acquisition is outside this release.
