# Autocue: user-requested project plan

This is the repository's durable specification for the detailed plan supplied by the user in this conversation. It is a consolidated restatement of that request, preserving its scope, defaults, phases and acceptance criteria. It is a specification, **not a completion claim**. The user's original instructions take precedence if a discrepancy is discovered. Later requests add citation retention, multi-track recommendations, installed-app Computer Use testing, accurate documentation and a continuation handoff; those are included below.

Read [the current agent handoff](../HANDOFF.md) for where to continue and [implementation status](IMPLEMENTATION_STATUS.md) for measured results. Requirement IDs below support traceability. The distinction between requested capability, implemented baseline, engineering verification and musical/real-app acceptance must remain explicit.

## Goal and fixed architecture — A

**A1. Product:** a Windows DJ preparation desktop app with rich, evidence-backed track context, cue review, set-list recommendations and executable transition plans. The DJ chooses and performs. Provide alternatives, reasons, local rehearsal and exports; do not build an autonomous or live-performance DJ. First release targets this Windows computer and rekordbox. Accuracy-first deep analysis may take longer. Retain existing CLI/export workflows.

**A2. Components:** Electron/React/TypeScript desktop; managed Python 3.11 native service for files, DSP, persistence and rekordbox; isolated Python/PyTorch WSL2 deep worker; versioned JSON-RPC over stdin/stdout, including progress and cancellation. Share core analysis, recommendation and export implementations between CLI and desktop. Use owned SQLite for metadata, observations, approvals, jobs, transitions and sets; compressed arrays for large features/model outputs. Use local playback/waveforms and FFmpeg decoding/rendering.

**A3. Data/security:** default app data `%LOCALAPPDATA%\Autocue`; leave music where it is. Separate configurable model/temp-render caches. Continue with ChatGPT uses eligible plan usage. Main process stores credentials using Windows-backed protection; renderer and workers never receive tokens. Selected profiles and derived features may be sent for user-initiated online requests; no automatic background enrichment. Audio requires a deliberate provider/file-specific upload action. Only approved cues enter the normal write set after an exact change preview.

## Merge with OnSet and live mode — O

Later request (2026-10-19). Autocue merges into OnSet (`matias-io/OnSet`, real-time rekordbox visuals in Rust/wgpu) as one Windows program with one installer. A1–A3 and every phase below still apply except where this section changes them. Merge proposal and decisions: the shared "OnSet + Autocue merge proposal" doc.

**O1. Product:** preparation stays as specified. Add a read-only **live mode** during a show: progress through the planned set, upcoming mix windows, a check that the loaded track matches the plan, key/tempo checks on the loaded track and fallback tracks when the DJ goes off plan. The DJ still chooses and performs; no deck control, no autonomous performance, nothing plays on its own.

**O2. Architecture direction (supersedes A2 step by step, not at once):** Rust core built on OnSet's crates; Tauri replaces Electron and carries Autocue's React screens; OnSet's launcher moves to React; the show visuals run in their own process; Python remains as a sidecar until native analysis and the workspace move to Rust, then only for deep models (ONNX Runtime/DirectML where models convert). SQLite remains the single local database, with sections, cues, beats and set plans in real tables and large arrays as files.

**O3. Live data:** OnSet's rekordbox memory reader supplies master deck, playhead and loaded tracks; Autocue's resolved structure, approved cues, energy and set plan feed OnSet's show director. Untracked tracks fall back to rekordbox grid/phrases. Per-version memory calibration is normal upkeep.

**O4. Show mode:** no analysis, deep models, Python worker or rekordbox writes while a show runs; queued jobs pause at their last checkpoint and resume afterwards. Live advice reads data prepared before the show.

**O5. Order:** (1) combine repositories keeping both histories, private library data and personal identifiers removed from the public copy; (2) one app with live mode using the existing Python service as sidecar; (3) Tauri replaces Electron; (4) workspace and native analysis to Rust, verified against existing Python tests and recorded outputs; (5) deep models on Windows GPUs. Each step keeps explicit approvals, exact write previews, backups/restore and human corrections.

## Phase 0: reliable baseline — P0

**P0.1 Foundation:** reproducible dependency definitions/lockfiles for desktop, native service and WSL worker; replace broken-environment reliance. Doctor covers runtimes, FFmpeg, rekordbox discovery/schema support, WSL2, GPU, RAM, disk and assets. Establish local version control while excluding music, credentials, DBs, caches and private reports. Preserve historical analysis/opinion/log/report inputs and known provenance/versions. Replace synthetic DB demo with generated-audio/grid assertions, removing madmom dependency, Linux paths and invalid result unpacking. Gate: reproducible CLI, automated baseline report and passing regressions before adding detectors.

**P0.2 Cue/export repairs:** allow B/C at bar zero and explicit co-located roles; regenerate optional cues when requested opinions are missing/fail; cached XML export must not import madmom. Commit valid deletions/comment changes even if no additions; include skipped/failed tracks in reports. Preserve grid source/phrase anchor on refresh and threshold evaluation. Validate loops and derive endpoints from actual beats.

**P0.3 Identity/provider/planning repairs:** stable identity instead of basename-only matching; atomic/recoverable opinions/caches; consistent serialization/quota behavior for standalone/hybrid Gemini jobs. Fix BPM-only planning without key; normalize standard/enharmonic/Camelot notation; pitch means adjustment applied to incoming deck.

## Phase 1: versioned profiles, persistence and jobs — P1

**P1.1 Shared objects:**

| Object | Required content |
|---|---|
| TrackProfile | Persistent internal ID, file identity, exact edit/remix, metadata, duration, analysis revision |
| GridObservation | Beat/downbeat arrays, tempo hypotheses, source, alignment/stability |
| FeatureBundle | Time-indexed energy, rhythm, timbre, harmony, vocals/stems |
| SectionObservation | Region/event type, original timing, source, uncertainty and evidence |
| ResolvedStructure | Ordered sections, distinct events, chosen interpretation and alternatives |
| KeyEstimate | Global/local keys, notation, tuning, alternatives, strength and source |
| EnergyProfile | Intensity dimensions, time curve, summary and calibration reference |
| CueProposal | Role, position, loop endpoints, dependencies, review/write state |
| TransitionOption | Track/window IDs, technique, playback settings, timing/actions/suitability |
| SetBrief / SetPlan | Constraints, subset/order, transitions, duration and explanations |
| JobRecord | Stage/state/progress, checkpoints, attempts, failures and resources |

**P1.2 Identity/compatibility:** separate internal/file/rekordbox identity. Moves update locations; changed audio invalidates dependencies. Identical audio aliases may share identity; distinct edits remain separate. Import legacy JSON/opinions without altering originals. Match unique full paths or verified cross-machine identity; ambiguous basename matches need resolution. Version JSON plus CSV/XML interchange. Human corrections stay separate from observations and survive recomputation.

**P1.3 Durable jobs/cache:** decode → grid → features → stems/models → structure → tonality/energy → enrichment → transitions → export. Cache by audio, grid revision, model/algorithm version and parameters. Persist completed stages atomically. Support pause/cancel/retry/restart recovery. Default one heavy and two light jobs with settings limits. Validated CUDA when available, otherwise explicit CPU status. Close hides to tray; Quit checkpoints/stops workers. Missing deep models cannot silently produce a completed deep result.

## Phase 2: rhythm, beat and phrase evidence — P2

**P2.1 Sources:** preserve rekordbox beat/downbeat/phrase evidence and its grid as the write destination. Independently run Beat This! beat/downbeat inference and DSP onset envelopes, autocorrelation/tempograms and dynamic-programming tracking. Benchmark against rekordbox, including CPU operation; do not assume a detector always wins.

**P2.2 Hypotheses:** assess tempo and half/double ambiguity, offset/drift, downbeat phase, missing/duplicated beats, phrase offset, local tempo changes and weak ambient/breakdown evidence separately. Retain competing tempo/phase hypotheses and multiple rhythmic signals; do not assume four kicks per bar. Prefer 4/8/16/32-bar phrases softly, allowing pickups, irregular sections and local resets.

**P2.3 Review/write:** user may correct the analysis grid; never automatically rewrite rekordbox's grid. Resolve or explicitly accept a conflicting destination grid before exporting cues.

## Phase 3: instrumentation and full structure — P3

**P3.1 Representation:** decode once into canonical PCM with original timestamp mapping and resampled model copies. Retain frame/beat/bar low/mid/high energy, bass transients versus sustained bass, percussion density/regularity, harmonic/percussive components, flux/brightness/flatness/timbre, chroma, vocal and stem-relative activity, silence/impacts/fills/sustained changes. Deep mode separates drums/bass/vocals/other locally; retain original mix and separation uncertainty.

**P3.2 Boundaries/repetition:** generate changes from rhythm/timbre/harmony/vocals/energy over beat, bar and multi-bar windows; preserve fine timing. Separate fills/pauses from sustained changes. Separate rhythm/timbre/harmonic self-similarity and recurrence identify returns, related drops/hooks and stripped versions.

**P3.3 Sequence inference:** combine boundary/repetition evidence, segment durations and plausible transitions in a duration-aware sequence model. Labels: intro, groove, vocal/verse, breakdown, build, drop/core, stripped groove, outro, silence, unknown. Allow repetition and no-drop/no-outro tracks. Distinguish first sustained full groove, first major payoff, main hook, later drops, breakdowns and useful mix windows. Cue D remains first sustained full groove unless preset deliberately changes.

**P3.4 Experimental sources:** isolated EDMFormer source; all-in-one boundary activations/embeddings evaluated separately from functional labels. Retain energy baseline/fallback and individual model results. Experimental output gains influence only after library evaluation.

**P3.5 Evidence resolution:** original timestamps, alignment and suggested refinement are separate; do not move all sources to energy edges before agreement scoring. Learn source reliability by event type. Distinguish absent/unknown/failed, timing uncertainty and semantic uncertainty. Conservative confidence until verified calibration supports probabilities.

## Phase 4: key, energy, genre and character — P4

**P4.1 Keys:** preserve rekordbox estimate; independent native chroma/template baseline; tuning-aware Essentia HPCP/key in WSL comparing general/EDM profiles. Global, section and actual mixing-window estimates, strongest alternatives, tuning offset, strength and weak-tonality state. Normalize enharmonics/Camelot. Compute sounding-key shift from speed with key lock off; harmonic suitability uses actual overlap and tonal density. Detector strength/separation are evidence, not probabilities.

**P4.2 Energy:** separate drive/density/intensity/tension/contrast curves; keep mood/danceability distinct. Combine interpretable and local learned estimates. Calibrate displayed 1–10 summary using user's ratings, collection/version and uncertainty. Collect pairwise and absolute judgments. Do not imply objective cross-genre rankings.

**P4.3 Enrichment:** multiple genres/subgenres, mood, instrumentation/arrangement from embedded metadata, local classifiers, sourced online findings and ChatGPT interpretation. Resolve exact edits before attaching facts; store citations, retrieval time, match evidence and per-claim confidence; unresolved matches visible. Preserve source-quality provenance; FLAC/bitrate cannot establish original quality. Support manually supplied URLs/metadata when research unavailable.

## Phase 5: desktop review/correction — P5

**P5.1 Work areas:** Library search/filter by state, genre, BPM, key, energy, vocals and review needs; Track review waveform/spectrogram/grid/sections/cues/stems/disagreement; Transition lab comparison/audition; Set planner brief/alternatives/reorder/energy arc; Jobs/settings progress, hardware, models, accounts, caches, diagnostics/backups.

**P5.2 Editing:** cue-centered audition, click track, before/after; drag cue/boundary with beat/bar snap or deliberate free placement. Edit labels/key/energy/genre/notes. Approve/reject events/cues individually or by selected groups. Undo and corrections survive reanalysis. Audio/grid/cue changes stale dependent approval. Show important event provenance/confidence. Prioritize disagreement/uncertainty/downstream impact. Separate analysis/review/write states instead of drop-centric OK/CHECK. Spectrogram frequency labels use actual coordinates.

## Phase 6: rekordbox hardening — P6

**P6.1 Preview/preservation:** separate analysis, preparation and application. Exact additions/replacements/deletions/skipped slots/comments/playlists; normal set only approved cues. Preserve manual cues, occupied slots/comments and manually altered generated cues by comparing last-written state. A–F and optional G/I/L; explicit cue limits/omissions. Delimited Autocue comment contribution. Destructive replacement only explicit selected operation. Validate memory loops, hot D, hot loops, colors and counts in real rekordbox.

**P6.2 Consistency/recovery:** SQLCipher-driver consistent snapshot instead of live DB/WAL sequential copying; validate snapshots/backups and fail clearly. Do not clean another job's snapshot. Rekordbox closed for writes, immediately rechecked before transaction/commit. Per-track savepoints under an actual outer transaction; rollback failed track. Verify before commit and read back afterward. Manifest plus backup ID. Restore complete consistent backup with rekordbox closed. XML remains supported with limitations; never prescribe deleting collection entries as an automatic workaround.

## Phase 7: technique-specific transitions — P7

**P7.1 Catalogue:** eighteen families, each with prerequisites, timing construction, actions, equipment/skill and clash checks.

| Families | Behaviors |
|---|---|
| Long blend; quick blend | Gradual/short beatmatched overlaps |
| EQ/bass swap; filter | Frequency-aware handovers |
| Cut/dropmix; drop swap; double drop | Abrupt changes/aligned drops |
| Breakdown blend | Atmospheric/quiet overlaps |
| Echo-out; reverb-out | Tail-assisted exits |
| Loop blend; loop roll | Extended/shortening loops |
| Acapella/stem blend | Selected elements |
| Tease/hot-cue exchange | Recognizable fragments |
| Backspin/brake | Audible reset |
| Tempo bridge | Ramps and half/double connections |
| Fade/end-to-start | Natural reset |
| Wordplay/thematic | Verified lyrics/hooks/themes |

**P7.2 Candidates:** multiple windows, not only F/E → B/C/A. Per-technique rhythm, local harmony, vocals/bass, arrangement density, energy movement and usable duration. A bad long blend may still make a good cut. Natural versus loop-extended windows; rhythmic/harmonic/seam continuity; fake-out checks before drop handover. Arrangement fingerprints for structural matching. Up to three distinct pair options with reasons, uncertainty, alternatives and rehearsal. Missing data lowers certainty, never becomes invented evidence.

**P7.3 Timing:** actual beats and explicit rate mapping. Separate source time, beats/bars and elapsed performance time. Incoming defaults to outgoing played tempo; initial ±6% applies to incoming deck. Carry key lock/transpose, validate available audio/phrase ending/vocal pickup/tails/loop exit; flag unreliable variable tempo.

## Phase 8: local rehearsal — P8

**P8.1 Render on demand:** initial long/quick blend, EQ swaps, filters, cuts, drop swaps, breakdown blends, loop blends, echo/reverb out. Proposed timing/rate/gain/EQ/effects, optional click/action markers. Synchronized manual audition for every technique. Label unsupported gestures; do not fake them. Show separation artifacts/uncertainty. Cache by source and transition revision. Never upload rehearsal automatically. Acceptance is useful audition, not exact mixer/DJ equivalence.

## Phase 9: ChatGPT and structured planning — P9

**P9.1 Sign-in:** documented local OAuth/PKCE, persisted installation host and account/workspace registration, identity/permissions validation, account selection/refresh/disconnect/reauth. Discover visible models; first visible default then retain choice. Stream store:false; require completed event. Recover limits, policy errors, unavailable plan usage and interruption without losing local data. Public documented endpoint; no browser-session extraction.

**P9.2 Boundaries:** selected profiles/candidate IDs/sourced context/optional derived images only. Sign-in route does not support audio input in the requested design; audio opinions require separate configured provider and deliberate upload. Research depends on capability; supplied sources/unenriched profiles when unavailable. Gemini remains optional with resumable validated responses, collision-resistant proxies and original time mapping. Specific clip/full-track provider/file choices do not authorize future blanket uploads.

**P9.3 Responsibilities:** suggest tags, reconcile sourced descriptions, select/order tracks, choose validated transitions, explain flow/alternatives. Never silently change measurements, approve cues/write rekordbox or invent timing. Manual JSON/CSV context export and returned-plan import use the same validation. Later requirement: retain consulted citations/audits through persistence and upgrade, and support validated multi-track recommendations, not just a single-track result.

## Phase 10: set selection and ordering — P10

**P10.1 Brief:** library/playlist candidates; target/tolerance; energy arc/mood; genres/variety; required/excluded; opening/closing; max pitch/key lock; techniques/equipment/skill; smoothness/contrast/familiarity/exploration.

**P10.2 Planner:** deterministic transition graph plus ChatGPT editorial choices. Beam width 128; three alternatives; no repeats, required selections, equipment/timing constraints; stable IDs break ties. Penalize unintended energy jumps, repeated technique and unsupported clashes. Performed duration uses entries/exits/rates/overlaps/loops/tails. Explain unsatisfiable conflicts and explicit changes; reject unknown AI IDs and offer valid replacements. Show subset/order, energy timeline, instructions, inclusion/omission reasons and fallback routes. Pin tracks/transitions, manually reorder and partially replan. JSON/CSV/printable HTML/PDF and reviewed rekordbox playlist exports.

## Interfaces and invalidation — I

**I1. CLI/RPC:** shared autocue operations doctor, import, analyze, review-export, transitions, plan, validate-plan, preview, rekordbox-preview, rekordbox-apply, restore. Compatibility scripts remain; legacy destructive operations explicit, standard approved writes. Desktop exposes same plus job control, edits, playback, account/model discovery.

**I2. Schema/provenance:** version profiles/catalogues/briefs/AI requests-results/manifests, reject unsupported future schema. AI requests/exports refer to immutable analysis revisions. Audio/grid/cue/key changes invalidate dependent transitions/previews and stale plans. Independently recover annotations, raw observations, resolved results and written state.

## Verification and acceptance — V

**V1. Regression/integration:** bar-zero B/C, missing opinions, empty selections, malformed grids, short/partial-bar tracks; exact constant/variable loops; key/Camelot/enharmonic and pitch direction; duplicate names/moves/changed audio/stale cache; BPM-only/technique compatibility; dry-run nonmutation/backups/rollback/idempotence/limits/manual and edited-generated cues/comments-only; active-rekordbox refusal/consistent snapshots; crash/quota/cancel/restart; OAuth cancellation/refresh/switching/unavailable models/incomplete streams/malformed AI/unknown IDs; desktop keyboard/undo/playback/approval/preview/restart; preview synchronization/gain/clipping.

**V2. Benchmark:** starting 128-track historical corpus, group duplicate songs/versions before approximately 60/20/20 development/validation/held-out split. Manually verify beats/downbeats, event presence/absence, key alternatives, mix windows and energy. Spectrogram opinions provisional. Measure beat timing/downbeat/tempo ambiguity; exact and ±1/±2-bar events; missing/invented events; boundary precision/recall and labels; key including ambiguous/weak; energy ranking; transition acceptability/rehearsal corrections/review time; runtime/memory/disk/cache reuse. Ablate features/models. Promote only held-out event improvement without more invented events. Targets +5 points exact-bar first groove and +10 breakdown/outro F1 over repaired baseline are targets, not promises.

**V3. Real rekordbox:** representative small test collection with manual cues, hot D, memory/hot loops, comments, hard endings, short intros; inspect in app and rehearse consistent backup restore on test copy before full-library writes. Fixtures do not close this gate.

**V4. End to end:** import → deep analyse → inspect → correct/approve → transitions → edit set → audition → validate → write preview → export/apply → restart with work intact. No live-performance control. Later request: use Computer Use for actual installed-app usability and verification; record exact tested build and separate unfinished tests.

## Delivery order, defaults and exclusions — M

| Milestone | Gate |
|---|---|
| M1 Foundation | Repaired CLI/reproducible runtimes/durable jobs/storage/database tests |
| M2 Review desktop | Library/waveform/corrections/approval/safe preview |
| M3 Rich analysis | Rhythm/instrumentation/structure/key/energy/benchmark |
| M4 Transition lab | Catalogue/timed options/loops/audition/previews |
| M5 ChatGPT planning | Sign-in/sourced enrichment/validated subset/order/alternatives/exports |
| M6 Personal production | Installer/recovery/real rekordbox/restore rehearsal/docs |

Build benchmark/review early. Local deep analysis on selected tracks; user-initiated online enrichment/recommendations; deliberate audio uploads; approval-based writes. Maintain model manifest with versions/hashes/locations/attribution, isolated experimental dependencies, local quality/performance report per analysis release and previous working runtime/model configuration for rollback.

Excluded from this release: public hosting, commercial distribution, macOS packaging, automatic music acquisition, live deck control and autonomous performance. Read-only live advice (O1) is no longer excluded; licensing and distribution of the merged program are decided later. Preserve future extensibility without making those prerequisites.

## User-supplied reference URLs

These are references supplied with the plan, not a claim that their external documentation was reverified during this handoff. Check current primary documentation when changing an integration.

- [Beat This! implementation](https://github.com/CPJKU/beat_this)
- [EDM-98 / EDMFormer package](https://pypi.org/project/edm98/)
- [Essentia Key](https://essentia.upf.edu/reference/std_Key.html)
- [ChatGPT sign-in](https://developers.openai.com/siwc/token-sharing-open-source/sign-in)
- [Models and inference](https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference)
- [Sign-in limitations](https://developers.openai.com/siwc/token-sharing-open-source/preview-limitations)
