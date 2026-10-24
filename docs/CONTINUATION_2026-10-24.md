# OnSet continuation, 2026-10-24

The Claude session logs were read to recover the interrupted work. Their last outstanding machine-work sequence was: package the local `merge-autocue` tip, install it, then deep-analyse La Danza, Ray Of Solar, Lock In and Carl Cox & Perry Farrell - Joya (Radio Edit). Keep the branch local until the user decides whether to push. Listening, cue approvals, real-deck checks and the particle branch remain separate decisions.

## Source and checks

Installer source: `abec26f`, including the Demucs stereo-input fix (`feec729`), EDMFormer chunking (`cd20a63`), deck numbering (`1f1d9d3`), two-deck simulator and engine tidy. The existing clean detached packaging checkout at `C:/Users/water/Projects/onset-pkg` was advanced to that commit; the main checkout's source was preserved.

Checks run during this continuation:

- UI: TypeScript/Vite production build passed; 36 tests passed across 8 files.
- Worker regressions: `test_deep_worker_input.py` and `test_deep_worker_edmformer.py`: 4 passed. These test stereo preparation and the chunking rule, not real model execution.
- Existing packaged Python runtime: numpy, scipy, soundfile, librosa and pyrekordbox imports passed. `service/pyproject.toml` and `service/uv.lock` have no changes between the previous packaging checkout (`aa791f3`) and this source.
- Claude's saved simulator command output was checked: build, simulator and summary exited successfully; 5,196 of 5,197 frames were grid-locked over 45.4 seconds, lag +35 ms over 237 kicks, final kick/snare confidence 0.99/1.00. This is the earlier simulator measurement, not a new real-deck or listening check. Its recording remains private on D:.
- A read-only SQLite backup of the logical workspace path passed `integrity_check`; all seven tables have row counts and full-row hashes recorded in a private pre-upgrade manifest under `D:/Offload/scratch/onset-upgrade-20261024-abec26f`. It contains 160 tracks, 160 aliases, 181 revisions, 9 edits and 16 jobs.
- Before the upgrade, the existing installed app was opened by its registered app ID. After service startup it showed 160 tracks, one reviewed track and six approved cues. No job was resumed. The subsequent native full-history exports and their comparison are recorded below; the initial logical-path backup alone was not treated as proof of active-workspace identity under Windows AppData virtualization.

## Installer result

The installer build passed with `scripts/package-desktop.ps1 -SkipRuntime` from the clean packaging checkout and `CARGO_TARGET_DIR=D:/Offload/cargo-target`. Rust release compilation took 10m 58s; NSIS packaging also completed successfully. The build reported a future Rust compatibility warning in upstream `binrw 0.14.2`, without failing compilation.

| Item | Verified result |
|---|---|
| Installer | `D:/Offload/cargo-target/release/bundle/nsis/OnSet_0.0.1_x64-setup.exe` |
| Bytes | 104,522,737 |
| SHA-256 | `ccb810c31b845a7740fb66743c670b8148940f79d4b2ddd7bc7d8cc4c9f34f88` |
| Raw desktop executable SHA-256 | `2f66658c0176155e6888298d8f0bb25c45e49fdbcbd186a8b2ba7c9214ef547d` |
| Installed executable SHA-256 | `657e4587d10a8a36058b8543f9a81d6fd0f90f7456f48dc207e071c6263331c9` |
| Bundled Python runtime | 10,305 files, approximately 450 MiB; reused with unchanged dependency definitions and a fresh import check |
| Source resource manifest | `service/.artifacts/installer-source-abec26f-20261024.json`: 48 service, worker, model-manifest, offset and notice files |

Tracked repository changes from this continuation are documentation only; the packaging checkout remains clean at `abec26f`. Nothing was pushed.

## Installed acceptance

The user approved installation and the four-track re-run. The existing app exported its full history, then exited through **Checkpoint jobs and quit**. NSIS **Add/Reinstall components** completed successfully in the existing `%LOCALAPPDATA%/OnSet` folder, and its **Run OnSet** option launched the new app with the installed `runtime/python.exe` and `service/` against the existing workspace.

All 48 installed source resources match the candidate manifest. The installed executable matches the raw build exactly after the NSIS bundler's expected `__TAURI_BUNDLE_TYPE_VAR_UNK` to `__TAURI_BUNDLE_TYPE_VAR_NSS` patch (three bytes at offset 27,125,152). Installed numpy/scipy/soundfile/librosa/pyrekordbox imports passed. Evidence: `service/.artifacts/installed-payload-abec26f-20261024.json`.

Native **Export full history** snapshots before and after the upgrade both passed SQLite integrity checks. Every full row across all seven tables is unchanged: 160 tracks, 160 aliases, 181 revisions, 9 edits, 16 jobs, 61,179 artifacts and 2 settings. On screen the new app showed the same 160 tracks, one reviewed track and six approved cues. Private exports and comparisons are under `D:/Offload/scratch/onset-upgrade-20261024-abec26f/`, including `post-upgrade-verification.json`. This is installed-payload and workspace-preservation acceptance, not a repeat of account, listening, projector or real-deck acceptance.

## Four-track deep re-run

Job `d835f53ccae0447891349b8209ad5321` ran in the installed app from 2026-10-24 03:15 to 04:41 EDT (86 minutes 9 seconds). Its saved parameters matched the exact four approved track IDs, with deep analysis and the existing `allin1`/`edmformer` settings enabled. It finished **complete**, with four completed tracks and no track-level failures. The read-only watcher ended normally, no low-disk pause was needed, and the temporary system sleep-prevention request was released. C: had 13.3 GiB free at final verification.

Every track has complete results for Beat This!, Demucs, Essentia, genre, all-in-one and EDMFormer. This resolves all six previously failing Demucs/EDMFormer stages in the approved group. Beat This!, which failed in all four baselines, also executed successfully in this run.

| Track | Models complete | EDMFormer chunk joins | EDMFormer segments |
|---|---|---|---|
| La Danza | 6/6 | 121 s | 15 |
| Ray Of Solar | 6/6 | 121 s | 16 |
| Lock In | 6/6 | None; whole 215.172 s track | 13 |
| Carl Cox & Perry Farrell - Joya (Radio Edit) | 6/6 | 122 s | 11 |

Independent final verification fully decoded all 16 stems: stereo, 16-bit FLAC, finite samples, expected frame counts and durations matching each track within 0.1 s. Every EDMFormer segment has finite start/end times within its track, and every `data.chunk_joins` value matches the configured rule. All four results retain an `invalid value encountered in divide` warning; the three chunked results also retain the warning that labels just after the join are less reliable. These checks establish execution and data validity, not listening ground truth.

After analysis, SQLite integrity is **ok**. All 160 track IDs and all 156 non-target profile rows are unchanged; aliases, nine edits and settings are unchanged; all 181 prior revision rows and all 16 prior job records remain intact. The four target revisions changed, with manual overrides preserved. There are now 185 revisions and 17 jobs. The app was brought forward and showed 160 tracks, seven analysed, one reviewed and six approved cues; Joya displayed **Complete** with a stale review, as expected after reanalysis. The installed executable and all 48 source resources were rechecked after the run and still match the candidate.

Private evidence remains excluded from Git: `service/.artifacts/four-track-baseline-20261024.json`, `four-track-watch-20261024.jsonl`, `four-track-results-20261024.json` and `installed-payload-abec26f-20261024.json`. Existing stems and deliberately paused setup jobs were kept.

La Danza's current D proposal is still 3.946 s, marked **proposed/unverified**, with stale review/write state. It needs listening and explicit cue approval; the completed model run does not approve that interpretation or write it to rekordbox. The other tracks also need human review of their new revisions.

The full project plan remains open as described in [PLAN_PROGRESS.md](PLAN_PROGRESS.md).

## Listening-review interruption: cue audition offset

The DJ reported that clicking a coloured cue-list button jumped earlier than the displayed cue. Source inspection confirmed that every button sought to `max(0, cue.time - 4)`: an implicit four-second lead-in, not a change to the saved cue. The source fix makes the coloured button seek exactly to the cue and provides a separate labelled **−4s** lead-in button. Waveform interaction is unchanged; a separate observed risk is that clicking a waveform marker is treated as a move even without a deliberate drag and deserves follow-up.

Engineering verification: 39 UI tests across nine files pass, including exact cue seeking at 3.946/49.562/224.1 seconds, separate clamped lead-in playback, action ordering and accessible labels. TypeScript and the production UI build pass. This source fix still needs packaging, installation and visible acceptance. No listening labels, cue approvals or rekordbox writes were made during this review; La Danza's interpretation remains open.
