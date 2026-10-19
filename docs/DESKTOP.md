# OnSet desktop operation

OnSet's desktop app (`crates/onset-desktop`, Tauri) holds the preparation screens and the live view in one window. It starts the Python service in `service/` as a child process and serves the React screens built from `ui/`. Automated engineering checks pass; musical accuracy and real rekordbox acceptance are separate release gates. See [implementation status](IMPLEMENTATION_STATUS.md).

Last reviewed 2026-10-19. The dated installer checks in [desktop verification](history/DESKTOP_VERIFICATION.md) were made on the former Electron build of the preparation screens, before they moved into OnSet. They do not carry over to the Tauri app, which has no installer yet and needs its own installed acceptance.

Development continuation: [agent handoff](HANDOFF.md), [complete plan](PROJECT_PLAN.md), [progress map](PLAN_PROGRESS.md).

## Start

On this computer, the **OnSet** Start menu and desktop shortcuts run `bin\OnSet.exe` in the OnSet folder: a release build of `onset-desktop` copied there. It uses the folder's `service\` source and `service\.venv` Python, so keep the checkout in place. FFmpeg must be available on PATH; this computer already has it installed.

Workspace data defaults to `%LOCALAPPDATA%\Onset\Onset\data\workspace` (database `workspace.db`). On first start the app moves a workspace left in `%LOCALAPPDATA%\Autocue` by the earlier preparation app into that folder and renames `autocue.db` to `workspace.db`. If those files are in use, it leaves them and tries again next start. `ONSET_WORKSPACE` selects an isolated workspace for testing. Audio remains in its existing locations. Historical JSON inputs are preserved.

1. Import music or `analysis_rb.json`, or use **Read rekordbox** to read a consistent snapshot. Historical profiles are marked imported, not verified.
2. In **Jobs & settings**, check the environment and set up the WSL worker. Setup downloads locked dependencies locally; initial model retrieval happens during deep analysis.
3. Analyse a small selection first. Light analysis explicitly skips deep models. Deep analysis runs Beat This!, Demucs and Essentia; missing or failed models produce a partial result.
4. Review the waveform, key alternatives, independent evidence and energy curves. Drag cue letters or section edges, edit section times and labels, and approve individual cues. Corrections and undo history are stored separately from raw observations.
5. Use **Transition lab** to compare entry/exit windows, pitch changes and instructions. Render supported local rehearsal previews on request. Manual two-track audition is also available; audible gestures still require rehearsal.
6. In **Set planner**, choose candidate tracks, duration, energy arc, equipment and required selections. Generate local alternatives or ask an AI provider for editorial recommendations. Plans are validated before saving or exporting.
7. Preview exact cue or playlist changes, close rekordbox, and apply the reviewed operation. A consistent backup and write manifest are retained. Test the result in rekordbox on a small collection before writing the full library.

Use **Import by pasted file or folder paths** when you already have an Explorer path. Identical audio is retained as location aliases. Detailed library filters accept standard or Camelot keys, tempo/energy ranges and vocal evidence.

Closing the window checkpoints the service's jobs before the app exits. Interrupted jobs return paused at restart and can resume from completed tracks and cached stages.

## Accounts and uploads

**Continue with ChatGPT** opens the documented OAuth/PKCE flow in the default browser. Account tokens are encrypted for the Windows user with DPAPI in the workspace folder and stay in the Rust process; the screens only see account status. Load the available models and select one; the choice persists per account. Requests use `store:false` and streaming. Incomplete streams are rejected. The installed Claude, Codex and Gemini command-line tools can also serve as AI providers.

ChatGPT receives the selected derived profiles, immutable analysis revisions, sourced context and validated candidate transition IDs. Audio is not included. Web research is optional and depends on account/model support. Completed responses retain consulted HTTPS URLs and per-claim citation audits. A retained citation means the URL matches consulted research evidence; it does not independently verify the claim or exact recording. Unconsulted claims are downgraded to model/unverified, with the original proposed citation retained for audit. Research-source counts describe consulted pages, not the number of corroborating sources for each claim. Unsupported tools or usage restrictions return an error without changing local work.

The existing Gemini command-line opinion provider remains optional. Invoking an audio opinion command uploads the named files to that provider; it is a separate action from ChatGPT sign-in and local analysis. The desktop does not automatically upload audio.

Official protocol references: [sign-in](https://developers.openai.com/siwc/token-sharing-open-source/sign-in), [models and inference](https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference), [limitations](https://developers.openai.com/siwc/token-sharing-open-source/preview-limitations).

## Review and preservation

Cue D retains the first sustained full-groove meaning. A/B/C can occupy the same bar, including bar zero. Loop endpoints use actual beat positions. Detector labels, confidence and local key estimates remain provisional until checked.

Reanalysis invalidates dependent transitions, previews, plans and approvals. Manual cue positions and metadata corrections survive. Edits to the analysis grid never rewrite the rekordbox grid. Cue export against an edited grid requires explicit acceptance after checking the rekordbox grid.

The normal write path preserves manual cues, occupied hot slots and generated cues that were changed externally or lack a matching write manifest. It reports conflicts rather than adopting old generated cues silently. Use **Adopt cues from earlier versions in rekordbox** to preview and explicitly adopt unchanged generated cues from before the manifest existed. Adoption does not write the rekordbox database. Generated cues keep the same UUID prefix as before, and the track comment block is now `[OnSet]…[/OnSet]`; an older `[Autocue]…[/Autocue]` block is replaced, not duplicated. XML export remains available through the legacy tools, with rekordbox's existing import limitations.

## Energy and benchmarks

Within-track curves show drive, density, intensity, tension and contrast. An overall 1–10 rating is absent until collection calibration. Personal absolute ratings and pairwise comparisons can inform calibration; the result is relative to that collection, not an objective cross-genre scale.

Export the benchmark annotation template before tuning detectors. Check song groups before accepting development/validation/test assignments; artist/title grouping is provisional. Verify events, keys, grids, mixing windows and perceived energy by listening. Existing spectrogram opinions are historical evidence, not gold labels.

## Development and builds

Use Rust (see `rust-toolchain.toml`), Node 24 and `uv` with managed Python 3.11. Bootstrap requires `uv` already installed. Use a new pytest temporary directory for repeat runs if an old directory has incompatible permissions.

```powershell
# Python service, from service/
powershell -ExecutionPolicy Bypass -File scripts/bootstrap.ps1
& .venv/Scripts/python.exe -m pytest -q -p no:cacheprovider --basetemp .test-data/pytest

# Screens, from ui/
npm ci
npm test
npm run build

# Desktop app, from the repository root (ui/dist must be built first)
cargo build --locked --release -p onset-desktop
```

`onset-desktop` finds the service through `ONSET_SERVICE_ROOT` (default: this checkout's `service/`) and its interpreter through `ONSET_PYTHON` (default: `service/.venv`, then `service/.runtime`). There is no installer for the desktop app yet; `service/scripts/package-runtime.ps1` prepares a self-contained Python runtime for that work.

**Interface preview without the desktop app.** Run `npm run dev` in `ui/` (or the `ui-preview` entry in `.claude/launch.json`) and open `http://127.0.0.1:5173`. Without the desktop bridge, `ui/src/devMock.ts` supplies a sample library so you can review layout and styling. It never touches real data, and Vite removes it from production builds. The dev server strips the page Content-Security-Policy only while serving, because React refresh needs an inline script. `ui/dist/index.html` keeps the strict policy.

**Interface layout.** The interface has five pages in the sidebar:

- **Library:** search, a view switch, a filter panel and an analysis selection bar.
- **Track review:** waveform and transport, plus Cues / Profile / Evidence / Stems / Corrections tabs. Writing to rekordbox lives under Cues.
- **Transitions.**
- **Set planner:** brief, AI adviser and plan options.
- **Settings:** Jobs, ChatGPT, Analysis engine, Energy calibration, and Data & backups (workspace snapshots, rekordbox write history and exports).

Colours, spacing and radii come from tokens at the top of `ui/src/style.css` and `ui/src/brand.css`; the Inter font files are shared from `assets/fonts`. Shared controls live in `ui/src/ui.tsx`.

To regenerate the local Python engineering report, run `& .venv/Scripts/python.exe scripts/quality-report.py` from `service/`. That script runs Python tests and retains existing acceptance records; it reads a saved desktop test JSON report rather than running Vitest or Computer Use. Run UI tests/build separately and keep their report provenance current.

Native dependencies are pinned in `service/uv.lock`; screen dependencies in `ui/package-lock.json`; WSL CPU dependencies in `service/worker/requirements-cpu.lock`. Experimental EDMFormer and all-in-one have separate locked WSL environments (under `~/.local/share/onset/experimental` in WSL) and setup controls. Both passed generated-audio CPU inference; they remain opt-in evidence pending listening benchmarks. The model manifest records versions and attribution; worker asset manifests record downloaded model hashes.

## Service CLI

From `service/`:

```powershell
& .venv/Scripts/python.exe -m onset_service doctor
& .venv/Scripts/python.exe -m onset_service import analysis_rb.json
& .venv/Scripts/python.exe -m onset_service analyze --light
& .venv/Scripts/python.exe -m onset_service analyze TRACK_ID --experimental edmformer
& .venv/Scripts/python.exe -m onset_service plan --params brief-request.json
& .venv/Scripts/python.exe -m onset_service review-export --params export-request.json
```

Other verbs: `transitions`, `validate-plan`, `preview`, `rekordbox-preview`, `rekordbox-apply`, `restore`, `benchmark`, `import-plan`, `rekordbox-import`, `workspace-health`, `workspace-backups`, `workspace-backup`, `workspace-restore-preview`, `workspace-restore`. JSON argument files use the same versioned requests as desktop RPC. Place global `--home C:/chosen/isolated-workspace` before the operation to isolate CLI tests from the active desktop workspace. `analyze` waits for the submitted job to finish; use desktop jobs for interactive pause/resume. `plan` accepts either a SetBrief or `{"brief":{...}}`; export accepts `{"kind":"profiles","path":"C:/chosen/output.json"}`. Writes require a saved preview ID. The one-off scripts from early development are in `service/tools/` (run as `python -m tools.<name>`), with explicit legacy write options; see [legacy CLI](history/LEGACY_CLI.md).

`cargo run -p onset-cli -- prepared "<title>"` shows the prepared structure and approved cues the show takes for a track, beside rekordbox's phrases.

## Recovery

In **Jobs & settings → Workspace health & recovery**, choose **Create verified workspace backup** for a consistent local snapshot. Nonempty workspaces also receive startup, quit-checkpoint and shutdown snapshots. Expand **Workspace snapshots**, choose **Preview workspace restore**, and inspect current and replacement counts. **Keep current workspace** dismisses the preview. Restoring requires explicit approval, a still-current preview and no active workers; a rollback snapshot is created first. These operations affect OnSet's workspace data. Unexpected database replacement or record loss pauses ordinary operations for review.

**Export full workspace history** saves a verified SQLite copy containing profiles, revisions, human corrections, undo history, jobs, settings and saved artifacts. Music, large feature/stem caches, model assets, credentials and the rekordbox database are separate. Keep those resources backed up as appropriate. To import a JSON or SQLite workspace export from another machine, use **Import from another computer**; files are remapped by folder and matched by checksum. After moving the feature/stem cache folder, use **Move analysis cache**. Local snapshots are not automatically pruned.

**Write history & recovery** separately restores a recorded rekordbox database backup with rekordbox closed. It checks the backup checksum and retains the current database before replacement. No automatic full-library write is part of setup.

## Review shortcuts and portable workspaces

Focus a cue row and use arrows for grid steps, Shift+arrows for 0.1-second adjustments, A to approve and R to reject. Space plays or pauses outside inputs. Save typed positions before approving. Select groups for one bulk decision; Undo reverses the whole group. Removal decisions still require a write preview.

The catalogue contains 18 technique families. Eleven support rendered previews: long/quick blend, EQ swap, filter, cut, drop swap, breakdown blend, echo out, reverb out, loop blend and fade/end-to-start. Double drops, loop rolls, stem blends, teases, backspin/brake, tempo bridges and wordplay remain manual auditions/instructions.

Deep analysis exposes local stem players for inspection. Transition previews use the actual proposed timing, beat-derived clicks and supported effects. Unsupported gestures remain manual rehearsal instructions.

Export complete workspace (JSON) preserves current profiles, sets, transitions and benchmark labels supplied by the user. It does not independently verify those labels. Full revision, edit and job history requires the separate SQLite full-history export. On import, missing audio can be located by its exact hash; approvals require fresh review, and invalid dependencies are marked stale. Cache arrays and stem files are not embedded in JSON.

### Launching from Codex on Windows

Use the ordinary Windows shortcut or double-click the executable in File Explorer. Packaged-host AppData virtualization can give a directly spawned child a separate profile at the same logical path. For audits, use the active app's workspace diagnostics and a fresh native full-history export, rather than assuming a shell's AppData view is the same database.
