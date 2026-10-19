# Desktop verification — 18–19 October 2026

## Current acceptance summary (2026-10-19)

The `release-0.2.0-complete-verification/Autocue Setup 0.2.0.exe` upgrade completed through Computer Use. Its installed payload matches the tested source. The ordinary Windows app retained account/model discovery, 160 tracks, five imported revisions, three paused jobs, citation audits and the saved validated three-track 9:53 set. A native post-upgrade export passed integrity and exact row comparison across every workspace table. Current recorded suites pass 93 Python and 26 desktop tests plus the production build.

Live citation retention and multi-track recommendations are verified engineering workflows. Musical accuracy, real rekordbox write/restore behavior, clean-machine installation, uninstall/rollback and broader recovery/accessibility coverage remain open. No live rekordbox writes are credited.

The sections below are a chronological record. Earlier test counts, failed attempts, pending installers and unresolved launch observations describe their date and build; later resolution sections supersede them. The current package identity and completed acceptance appear in the final section.

## Initial isolated QA pass (2026-10-18)

Testing used the Computer Use plugin against a packaged `AutocueQA.exe`, whose data is isolated beside the executable in `qa-data`. The production workspace and real rekordbox database were not modified. Three generated 96-second audio files and explicit grids served as known fixtures. These are software acceptance checks, not listening-based musical validation.

## Initial completed visible workflows

- Launch packaged desktop and import a historical analysis JSON through pasted paths.
- Recognize identical audio as aliases; reimport changed aliases as separate tracks after fixing the shared-identity regression. Three unique tracks are visible; original approval is retained.
- Open waveform review, start playback, observe elapsed time advancing to 23.6 seconds, and pause.
- Select all cues, approve the group, verify all seven approval states, undo once and verify all seven return to proposed.
- Type cue A at 0.5 seconds, save it, approve it and verify its position/approval survives a clean restart.
- Verify a newly typed unsaved position disables individual and bulk approval and displays the save warning. Save the edit before quitting.
- Change a saved cue, restart, verify dependent sets show Needs revalidation, and replan successfully.
- Switch work areas and verify the new area returns to the top of the page.
- Generate up to three technique-specific options for each other fixture track.
- Render a 40-second EQ/bass-swap rehearsal, play it locally, observe its clock advancing past 11 seconds, and pause. Chromium accessibility incorrectly labels its native audio group disabled; the visible player and advancing playback establish success.
- Submit a four-minute set brief with one-minute tolerance. Obtain three validated 3:44 alternatives containing all three tracks.
- Restart and verify saved plans reappear. Inspect full-width transition instructions after repairing the narrow-column layout.
- Export the selected set through the native Windows Save As dialog, then read back three track IDs and a 224-second duration from the resulting JSON.
- Generate three fallback routes whose whole-set timing is validated. A discovered persistence issue was corrected: discovery now leaves alternatives unsaved until explicitly selected. Regression coverage verifies discovery non-mutation and idempotent selection. The final packaged UI kept exactly three displayed options during discovery, then saved an explicitly selected echo-out fallback while retaining valid 3:44 timing.

## Fixes found during this acceptance pass

The pass found a scroll-effect cleanup crash, hidden playback controls caused by retained scroll position, misleading import counts for aliases, changed-alias identity replacement, unsaved-position approval risk, narrow transition instructions, missing saved-set loading, and premature fallback persistence. Source fixes and regression checks are included in 0.2.0. A recovery screen now handles renderer failures.

## Remaining acceptance scope (updated 2026-10-19)

- Real-account sign-in, model discovery, sourced-claim retention and multi-track selection passed in the continuations below. Restricted-account policy and expired-token acceptance remain outstanding; synthetic protocol coverage is separate.
- Real rekordbox cue/loop/comment/playlist inspection and restoration on a small test library remain outstanding. No live-library write is credited.
- Remaining native import-dialog cases, every keyboard/accessibility path, all manual performance gestures, all display sizes, clean-machine installation/uninstall/rollback and live account policy errors remain outside the completed UI matrix. The current installer upgrade passed; all eleven renderable families have generated-audio engineering coverage, not listening acceptance.
- Musical key/event/energy accuracy, separation artifacts and transition quality require verified listening labels and rehearsal.

See [implementation status](IMPLEMENTATION_STATUS.md) for the complete remaining scope. The private machine-readable engineering report is `.artifacts/quality-report.json`.

## Installed-app continuation

The user approved installation. The Windows installer completed and launched Autocue. On resuming Computer Use, the installed interface loaded and matched the release executable, UI archive, service and native runtime hashes. Read rekordbox completed with 160 real tracks. No cue/playlist writes were requested.

This revealed that pre-analysis durations were zero. The correction reads audio metadata before analysis and fills existing rekordbox imports from rekordbox duration metadata. Import progress and durable missing/failed-track reports were added. Regression coverage includes tagless WAV durations and missing rekordbox audio. A separate live-library import using the corrected backend returned 160 unique tracks, 160 durations, zero missing/failed tracks and 49.63 seconds. This backend check is separate from installed-UI acceptance of the patch.

The user handled the Windows Security prompt. Computer Use then verified installed diagnostics (native packages, FFmpeg, WSL Ubuntu and rekordbox discovery), visible import progress and a completed 160-record import with zero skipped/failed records. Fragments displayed a 3:30 duration; local playback advanced past 11 seconds and paused at 18.7 seconds. A subsequent clean exit/relaunch retained all 160 tracks.

An earlier restart showed an empty library and no paused job despite a contemporaneous external database reader reporting 160 tracks and one job; after exit the file read empty. At that point the cause was unresolved; the AppData redirection resolution is recorded below. A fresh installed-UI import followed by exit/relaunch, without concurrent database inspection, passed. Do not credit the earlier paused job as recovered or treat one successful repetition as closing the broader persistence investigation.

Filtering to one visible track exposed an analysis-selection issue: the default still targeted all 160 records. The correction scopes the default to visible results, retains explicitly checked tracks and shows hidden selections. Final installed acceptance of that correction is recorded in the machine-readable UI report.

The filtered-analysis correction passed the installed interface: Fragments alone selects one visible track, and an unmatched query disables both analysis actions. A single-track deep job started and paused. Exit/relaunch retained the job but exposed a separate shutdown-state bug (paused work became cancelled). The correction distinguishes shutdown pause from explicit cancellation; both paths pass new regression tests. Final installed restart acceptance follows separately.

Final installed acceptance of the shutdown correction passed: the saved job resumed through the UI, paused, and survived Quit Autocue and checkpoint jobs followed by reopening. The interface showed paused and “Stopped at application shutdown; resume to continue”; all 160 tracks remained visible. The app is left open with that single-track job paused. A closed-app backup contains all 160 tracks and that job; SQLite integrity_check returned ok. The earlier separate empty-library observation was still open at this stage; it is resolved for ordinary Windows launch in the later migration section.

The final source passes 46 Python tests (134.23 seconds) and 16 desktop tests, plus the production build. The rebuilt local installer is at `release-0.2.0/Autocue Setup 0.2.0.exe`. It uses an uncompressed payload and has no differential package; its larger size is intentional for this local acceptance build. The latest installer was not rerun. Installed executable, UI archive, service and store hashes match the packaged files after the approved app was patched with the app closed.


## Real-account and workspace recovery continuation (2026-10-18)

The user completed ChatGPT authentication. Computer Use verified eligible plan usage, model discovery, metadata-only enrichment, exact-release web research and a validated 210-second single-track set through the installed interface. No audio was uploaded. Source verification found an optional Apple Music URL slug mismatch; the resolver now retains the actual consulted URL only when the storefront, resource type and stable song ID match. Other URL differences remain unverified. Live post-fix research is recorded separately when complete.

Recovery now includes consistent SQLite snapshots, hash/integrity verification, record-count and file-identity health guards, full history export and content-bound restoration previews. Generated fixtures test restoration, stale approval rejection and rollback snapshots. Computer Use verified the installed health panel, creation of a manual verified backup, and the exact current/after counts for tracks, aliases, revisions, edits, jobs, artifacts and settings. Apply remained disabled without approval; Keep current workspace dismissed the preview. No real workspace restoration was performed.

After the updated source and UI archive were completely copied, a fresh installed launch retained 160 tracks, three paused jobs, the signed-in account and the previously saved 3:30 plan. Model discovery succeeded again. Unicode labels, the energy comparison controls and the set planner fit the observed 1426 by 943 window. This does not establish every display size or keyboard path. A premature launch during an earlier asynchronous package copy was discarded as invalid acceptance evidence.

The current engineering checks are 60 passing Python tests plus 12 passing targeted job/workspace checks after the final quit-checkpoint change, 22 passing desktop tests and a successful production build. Native analysis of a real 210-second track completed in isolation; its musical accuracy is not established. The latest native-stage/deep-model resumption run and final installer execution are tracked separately and must not inherit older success results.

A further explicit File > Exit showed the checkpointing title and removed the desktop window. A new process reopened the 160-track workspace with three paused jobs. Some old Windows process records remained visible during teardown, so complete prompt process-tree termination is not credited. Launching an already open installed app retained one visible window. The native full-history Save As dialog opened; changing the filename was interrupted by focus/input changes, and no completed UI export is credited.

The post-update live research request completed and saved provenance, but its release claims still lacked a verified consulted citation and were correctly stored as unverified. The optional-slug resolver has unit coverage; this live run does not establish successful sourced-claim retention. Further request-level citation diagnostics are needed before closing that acceptance item.

The current native-analysis-3 resumption smoke subsequently completed all five model sources: Beat This!, Demucs, Essentia, EDMFormer and all-in-one. It reused the native feature checkpoint before entering the isolated workers. This is generated-audio execution/recovery evidence, not a listening benchmark or a real-library accuracy result.

The final recovery installer completed packaging successfully in `release-0.2.0-final-recovery/Autocue Setup 0.2.0.exe`. Its payload UI archive and recovery/service sources match the current files. An initial archive-read/cleanup lock conflict was corrected with a sequential build retry. The latest installer has not yet been executed; the installed copy was updated while closed. Installer execution remains a separate action-time approval and acceptance gate.


## Rebuilt installer upgrade acceptance (2026-10-18)

The user explicitly approved this rebuilt installer. Computer Use ran the 804,210,656-byte artifact whose SHA-256 is `ec93cb6582042f7a43dd9efd0e57ffd2f366a24a7222d7e3d89efbc987d35aca`. The installer recognized the existing per-user installation and completed its upgrade. The resulting executable, UI archive, managed Python executable, service and workspace modules match the verified package. A consistent pre-upgrade snapshot retains 160 tracks, three jobs and three artifacts and passes SQLite integrity validation.

The installer-launched child exposed an empty workspace and no account. Its diagnostics named the usual LocalAppData path, but reported a different file identity and workspace ID from the intact original database at that path. Refreshing confirmed the discrepancy. Closing that child and launching the installed executable directly through Computer Use reopened all 160 tracks, three paused jobs and the signed-in account. The direct launch passed workspace health, database integrity, account-specific model discovery and manual verified-backup creation. The same saved 3:30 set remains present. No restoration or reimport was needed, and no live rekordbox writes occurred.

This establishes successful file installation and direct-launch persistence. The installer-run child's filesystem/launch context remains unexplained, so automatic post-install launch is not fully accepted. Do not describe the empty child view as loss of the original library. Native full-history export completion remains uncredited; only opening the dialog is established.


## Resolved: normal Windows launch and migrated preparation data (2026-10-18)

The launch discrepancy above was traced to Windows package virtualization inherited from Codex. `fsutil file queryfilenamebyid` resolved the preparation database identity to `AppData/Local/Packages/OpenAI.Codex_2p2nqsd0c76g0/LocalCache/Local/Autocue/autocue.db`, whereas an Explorer/installer-launched app opened ordinary `AppData/Local/Autocue/autocue.db`. This matches [Microsoft's documented AppData redirection behavior](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-behind-the-scenes). The earlier logical-path checks alone could not distinguish these files.

With both app instances closed, Computer Use preserved the ordinary empty folder as `Autocue-empty-before-migration-20261018` and copied the prepared Autocue folder into ordinary LocalAppData through native File Explorer. The source remains intact. The copied encrypted account file also required its matching Electron `Local State` metadata from the redirected Roaming profile. The old ordinary `Local State` was preserved as `Local State.before-migration-20261018` before copying the matching file; no credential plaintext was exposed or security settings changed.

A subsequent launch by double-clicking the installed executable in Windows Explorer passed: 160 tracks, three paused jobs, the original workspace ID, healthy storage and the retained signed-in account. NTFS file identity now resolves the running service's database to ordinary LocalAppData. Model discovery returned GPT-6.1-Sol without another sign-in.

The native full-history Save As workflow then saved `.test-data/normal-windows-verified-workspace.db` outside AppData. Independent SQLite readback passed integrity validation and exact full-row comparison against the preparation source for all seven tables: tracks, aliases, revisions, edits, jobs, artifacts and settings. Counts were 160 tracks, 160 aliases, three jobs and three artifacts, with zero revisions/edits/settings. This closes the previously uncredited native full-history export acceptance case.

Explicit Quit removed all Autocue processes. A second ordinary Explorer launch retained all 160 tracks, the account and the saved validated 3:30 plan. Autocue is left open in that ordinary Windows context. The upgrade, normal launch, account migration and persistence gate is now passed on this machine. Cross-machine/clean-machine installation, uninstall and rollback remain separate acceptance work. No live rekordbox writes or audio uploads occurred.

For future acceptance runs, launch the installed application through ordinary Windows Explorer or its Windows shortcut. A process spawned directly by packaged Codex can inherit its redirected AppData view. Do not treat the shell's logical AppData path as proof that it is auditing the active ordinary-Windows database; compare physical file identities or use a fresh native workspace export. The preserved redirected copy is a backup, not a second active workspace to edit.


## Citation retention and multi-track acceptance (2026-10-19)

The installed application was updated with a backed-up acceptance patch and launched through ordinary Explorer. A missing `jose` dependency in the first repack caused a startup error; the dependency was restored before subsequent tests. No credentials or library records were changed by that packaging failure.

The historical importer initially matched 128 files without attaching analysis because grid-only rekordbox metadata suppressed it. The repaired importer passed regression coverage and imported a five-track subset with `legacy-import`, `imported`, proposed cues and unverified confidence retained. Missing historical key fields preserve imported key metadata. These observations are software-test inputs, not listening ground truth.

Using the signed-in account and discovered GPT-6.1-Sol model, Suggest a set returned Losing It -> Stop It -> La Danza. The interface reported ChatGPT proposal validated and 9:53, with filter/cut instructions, key lock and pitch constraints. It explicitly identified unknown energy and arrangement uncertainty. Following online research, the Stop It evidence panel showed 77 consulted research sources retained, its actual DIRTYBIRD/Bandcamp URL, Citation: retained, retrieval timestamp and an Open cited source button. The model separately warned that the local file's exact edit was unverified.

The native full-history export saved `.test-data/citation-multitrack-live-workspace-20261019.db`. Independent read-only SQLite validation passed integrity with 160 tracks, five analysis revisions, three jobs and zero approved cues. Eight web claims across the stored enrichment records exactly match retained consulted URLs. The saved three-track plan independently validated at 593.4328450249433 seconds, including playback-rate continuity. Evidence is in `.artifacts/live-adviser-verification-20261019.json`.

After explicit Quit and another ordinary Explorer launch, the signed-in account and saved 9:53 three-track plan remained visible. No audio was uploaded and no live rekordbox writes occurred. Subsequent Computer Use was paused to honor another user-authorized task's exclusive desktop testing interval.


The saved real-track filter and cut options were rendered locally through the shared preview core using the independently exported profiles. Source fingerprint checks passed. Independent waveform readback found finite samples, the expected 27.36-second and 12-second durations, correct action offsets and peaks at or below 0.951. Evidence is `.artifacts/real-transition-preview-verification-20261019.json`; renders remain under `.test-data/live-transition-previews-20261019`. This verifies engineering synchronization and clipping limits, not listening quality or exact mixer behavior.


The final installed source includes `structure-resolution-2` and launches normally after a clean restart with all 160 tracks. The actual bundled Python runtime also completed the isolated silent-track smoke test with no mix cues, absent musical events and SQLCipher available. The final full suite passes 93 Python tests and 26 desktop tests. The rebuilt installer completed successfully and its extracted UI archive and all 25 Python source files match the tested source exactly; the `jose` 6.1.0 dependency is present in the extracted archive. The user-approved new installer UI acceptance passed on 2026-10-19; details follow.



## Complete-verification installer acceptance (2026-10-19)

Computer Use opened the exact approved installer through ordinary Windows Explorer, selected the current-user installation and existing `%USERPROFILE%\AppData\Local\Programs\Autocue` destination, and observed the successful completion screen. Finish with Run Autocue selected launched the installed application. No security prompt was automated. After installation, every one of the 25 Python source hashes and the UI archive hash matched the tested manifest.

The application retained the signed-in account and successfully discovered GPT-6.1-Sol. Workspace health reported 160 tracks, five revisions and three paused jobs. A new native Save As export created `.test-data/post-installer-workspace-20261019.db`. Independent read-only validation passed SQLite integrity and exact full-row equality against the pre-upgrade export for all seven tables: tracks, aliases, revisions, edits, jobs, artifacts and settings. This includes all 105 artifacts and the stored citation audits and plans.

Visible acceptance also confirmed the saved Option 2, 9:53 Losing It -> Stop It -> La Danza plan, validated timing and filter/cut handovers, pitch/key-lock settings and unverified-musical-evidence warnings. Stop It's expanded evidence panel retained the actual DIRTYBIRD/Bandcamp URL, 77 consulted research sources, the Citation: retained audit and separate exact-edit uncertainty. No new online enrichment, audio upload, cue approval or live rekordbox write was performed. Evidence is `.artifacts/package-verification-continuation-20261019.json` and `.artifacts/post-installer-workspace-verification-20261019.json`.

## Audit-gaps installer acceptance (2026-10-19)

Installer: `release-0.2.0-audit-gaps/Autocue Setup 0.2.0.exe`, 212,965,009 bytes, SHA-256 `70c20400fb5a29d0292eb94a0adc29b9c6cd39530f9cfc00481586b3116f209e`. The hash was rechecked before running it. The user approved the install. Evidence: `.artifacts/package-verification-audit-gaps-20261019.json`.

**Install.** Computer Use ran the installer from ordinary File Explorer and chose "Only for me (water)" with the existing destination `%USERPROFILE%\AppData\Local\Programs\Autocue`. The installer finished and launched Autocue itself; no shell started the app. `scripts/verify-package.py` then matched all 51 installed Python, worker and app-archive files to the tested source; installed app.asar SHA-256 is `69fef91c9c9c16ec0abcda9aa16a5209b1e49b6215beae65a88d579f8a9ec67a`. The app opened the ordinary workspace (160 tracks; Data & backups: Healthy, 5 revisions, 45 snapshots).

**Passed, visibly in the installed app:**

- **Review queue.** The Library view put the five imported tracks first (Stop It, World Hold On, La Danza, Losing It, FISHER - STAY), each with a reason such as "Unapproved hot cue D or mix-in/out cues · +5 more". Pending tracks followed with "Key is weak or unknown".
- **Brief fields.** Smooth blends, Contrast and rekordbox playlists rendered with their hints. Their values were not changed.
- **Set search as a job.** "Build 3 options" over all 160 visible tracks created a "Set search · Running" job and the planner showed "Cancel search". Cancelling left the job Cancelled after roughly 20–25 s.
- **Completed set search.** With the five imported tracks checked and a temporary 12-minute / 8-minute brief, a Set search completed with validated 12:00 four-track options.
- **Key shift.** Setting Stop It to +1 st showed "Key shift saved; rebuild or replan to apply it". It was reset to ±0 st.
- **Replan.** With no key shift, "Keep tracks 1–2 and replan the rest" returned options explained as "Keeps the first 2 tracks and replans the rest", all starting Losing It → Stop It. The last row's replan button is disabled.
- **Benchmark form.** The structured form rendered on Fragments (development split): four events, keys, true BPM, notes and the Advanced JSON section. "Use playhead (bar 0)" set Later drop to Present, bar 0. Nothing was saved.
- **Data & backups.** The Import from another computer and Move analysis cache cards rendered, with their action buttons disabled until filled in. Neither was run against the real workspace.

**Findings:**

1. **Key shift blocks replan.** A key shift on a track inside the kept part of a set makes "Keep tracks 1–N" fail with "Transition was calculated for a different transpose setting". The kept transition was calculated before the shift. The refusal is safe, but the replan cannot proceed until that transition is recalculated.
2. **Slow cancel on large searches.** Cancellation is only checked between search depths, so on 160 tracks the first full pairwise pass delays it by about 20 s.
3. **Seeking (outside these changes).** Clicking the waveform on Fragments and on Stop It left the playhead at 0:00.0, so "Use playhead" was only exercised at bar 0. Not investigated; seeking was never covered by earlier acceptance either.
4. **Cosmetic.** While a set search runs, the sidebar reads "Analysing · 0%" and the job's progress stays at 0% until it completes.

**Follow-up (source only, not packaged or rechecked in the installed app).** Findings 1, 2 and 4 are fixed in source:

- Replan now recalculates kept transitions that do not match the current key shifts or timing, keeping the same technique where possible, and says so in the plan explanation.
- The set search checks for cancellation for every candidate pair, not only between depths.
- The set search reports an estimated progress fraction, saved at most four times a second. The sidebar names the running job, for example "Set search · 40%".

Regressions: `test_replan_recalculates_kept_transitions_after_a_key_shift` and `test_set_search_reports_progress_and_cancels_within_the_first_pass`. 120 Python tests, 33 desktop tests and the build pass. Finding 3 (waveform seeking) is fixed in source too. The `autocue-audio` handler forwarded requests with `net.fetch(file://)`, which did not answer range requests with 206 Partial Content, so Chromium treated audio as unseekable and reset the playhead. `desktop/media.ts` now serves byte ranges itself (`desktop/media.test.ts` covers 200, 206 and 416 responses). A development Electron build was run with Computer Use on an isolated workspace with one generated track. Clicking the waveform held the playhead at 0:44.5, and playback continued to 0:47.7. Not yet checked in a rebuilt installer.

**Side effects:** two Set search jobs (one complete, one cancelled) and their set and transition artifacts were saved to the workspace. The brief's length and tolerance were set to 12 and 8 minutes for one search, then restored to 60 and 2. Stop It's key shift was set and reset. No cues were approved, nothing was written to rekordbox, no audio was uploaded, and the three paused deep-analysis jobs were not touched.
