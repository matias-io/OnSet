# Windows runbook

Reviewed 2026-10-19 for OnSet's preparation screens in the desktop app. See [desktop operation](DESKTOP.md) for source setup and [implementation status](IMPLEMENTATION_STATUS.md) for incomplete production gates. The September runbook is [archived separately](history/LEGACY_WINDOWS_RUNBOOK.md).

## Start or update

There is no installer for OnSet's desktop app yet. On this computer, the **OnSet** Start menu and desktop shortcuts run `bin\OnSet.exe`, a release build in the OnSet folder that uses the folder's `service\.venv` Python; build and replace it as described in [desktop operation](DESKTOP.md). Launch from the shortcut or Explorer.

FFmpeg must be on PATH. Deep models use the local WSL2 worker configured in Jobs & settings; the Check environment result reports missing components. CPU execution passed on generated audio; CUDA and clean-machine installation remain unverified. Keep a verified workspace backup before an update.

The former standalone Electron build passed an upgrade check on 2026-10-19 that retained account/model access, 160 tracks, five imported revisions, three paused jobs, citations and the saved three-track set. That result belongs to that build, not to the desktop app.

## Import, analyse and review

1. Import local files/folders or choose Read rekordbox. Source files stay in place. Historical JSON import does not make old detector opinions verified.
2. Select a small group first. Without explicit checked selections, analysis targets the visible filtered tracks; empty results disable analysis. Inspect the displayed selection count.
3. Set up the WSL worker in Jobs & settings for deep analysis. Light analysis skips deep models. Experimental structure sources are opt-in; failures remain visible as partial analysis.
4. Inspect rhythm disagreement, keys, sections, energy and cue positions by listening. Save typed positions before approval. Manual corrections survive recomputation; changed dependencies require renewed review.
5. Approve only the cues you want written. A legacy OK/CHECK label or an imported historical proposal is not approval.

## Transitions and set planning

Use Transition lab to compare local entry/exit windows, incoming tempo adjustments, key-lock settings and clash warnings. Supported previews render only on request and stay local. A preview is an audition aid; equipment behavior and listening quality require rehearsal.

In Set planner, choose duration/tolerance, candidates, required/excluded tracks, equipment and energy preferences. Local planning retains up to three alternatives. ChatGPT advice requires sign-in and an available model; requests send selected profiles and validated transition candidates. Research is optional. Inspect retained source citations and exact-edit uncertainty. Energy is unknown until rated or calibrated; a requested energy arc cannot create missing measurements.

## Preview and apply rekordbox changes

1. Review exact additions, replacements, removals, comments and skipped slots in the change preview. Resolve manual/edited-generated conflicts and analysis-grid conflicts.
2. Close rekordbox. Apply the approved preview; the service rechecks process state and preview dependencies, creates a consistent backup and records a write manifest.
3. Open a small test collection in rekordbox and inspect cue D, colours, memory/hot loops, counts, comments and playlists. These real-app checks and restoration rehearsal remain outstanding. No live-library write is credited by the current verification report.

The legacy `onset_service/cuewriter.py` write path requires explicit `--legacy-unreviewed` and does not provide the same manifest-based conflict protection. Use [legacy CLI documentation](history/LEGACY_CLI.md) only when deliberately choosing that workflow. XML import has limitations for existing collection entries; use a test collection and do not delete collection entries as an automatic workaround. Cached `python -m tools.cues_xml --from-json` export does not require madmom; fresh legacy grid inference does.

## Checkpoint, backup and restore

Closing the window checkpoints jobs and exits. Interrupted jobs reopen paused and can resume; not every native model call cancels immediately.

Jobs & settings offers verified workspace snapshots and full-history SQLite export. Export to a separate file outside active application/rekordbox data. Music, credentials, large feature/stem caches, models and rekordbox data are separate resources. For a workspace restore, preview exact replacement counts, explicitly approve and stop active workers; a rollback snapshot is created. Cross-machine portable snapshot import has no dedicated interface yet.

For a manifest-backed rekordbox restore, use Write history & recovery with rekordbox closed and validate the selected backup. Rehearse on a test library first. Replacing only a live master.db file is not a complete WAL-aware recovery procedure.

If an app launched from packaged Codex shows a different workspace, compare diagnostics and a native export before assuming data loss. Codex-spawned children can inherit redirected AppData. The historical redirected preparation copy was preserved as a backup; ordinary Windows launch is the current operating context. See [desktop verification](history/DESKTOP_VERIFICATION.md).
