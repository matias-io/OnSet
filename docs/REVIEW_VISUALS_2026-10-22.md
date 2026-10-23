# Review: beat-locked hits, structure in every scene, replay and particles

What to run and what to look for, in order. Parts 1–3 are on `merge-autocue` (commits
`f39a790`, `c56801d`). Parts 4–5 are on their own branches and are tested in their own
worktrees, so nothing is merged until you decide. Every automated block says what "pass"
looks like; every manual check says what you should see and what would mean a problem.

Build settings throughout: `CARGO_TARGET_DIR=D:/Offload/cargo-target` and `-j 4` (C: is
nearly full). First builds take several minutes.

## 1. Automated checks on `merge-autocue`

Unit and integration tests for the crates that changed.

```bash
cd /c/Users/water/Projects/OnSet && CARGO_TARGET_DIR=D:/Offload/cargo-target cargo test -j 4 -p onset-core -p onset-engine -p onset-render -p onset-settings 2>&1 | grep -E "^test result|FAILED|panicked"
```

Pass: every line is `test result: ok`, none `FAILED`. Expected counts: onset-core 72
(eight are `hits::tests::*`), onset-render 28 unit + 25 across the test binaries,
including `scenes::a_scene_on_disk_compiles_against_the_common_on_disk` and
`hud::tests::scope_shows_the_hits_and_the_lag`.

Lints, as the workspace denies `clippy::all`:

```bash
cd /c/Users/water/Projects/OnSet && CARGO_TARGET_DIR=D:/Offload/cargo-target cargo clippy -j 4 -p onset-core -p onset-engine -p onset-render --all-targets 2>&1 | grep -E "^(warning|error)" | grep -v "binrw|future version"; echo "clippy done"
```

Pass: only `clippy done`.

The Show page's new HUD switch type-checks:

```bash
cd /c/Users/water/Projects/OnSet/ui && npx tsc --noEmit -p . && echo "tsc ok"
```

## 2. The beat-lock, simulated

The eight `hits` tests simulate 120 BPM with the audio detector reporting kicks 50–80 ms
late. Run them with output to see the numbers:

```bash
cd /c/Users/water/Projects/OnSet && CARGO_TARGET_DIR=D:/Offload/cargo-target cargo test -j 4 -p onset-core --lib hits -- --nocapture
```

What each proves ([crates/onset-core/src/hits.rs](../crates/onset-core/src/hits.rs)):

| Test | Claim |
|---|---|
| `grid_kicks_land_on_the_beat_not_on_the_late_audio` | after a few beats the shown kick peaks at beat phase 0 and has decayed by the time the audio hears it |
| `measures_the_audio_lag` | a 60 ms audio lag is measured within 15 ms after 32 beats |
| `confidence_rises_with_agreement_and_falls_without_it` | confirmed kicks push confidence past 0.9; twelve beats of silence from the detector take it below 0.1 and the grid kick fades out |
| `rekordbox_quiet_lows_gate_the_kick` | rekordbox's low band under 0.15 means no grid kick (a breakdown) |
| `off_grid_audio_hits_pass_through` | a kick halfway through a beat (a fill) still shows at full strength |
| `without_a_grid_or_while_paused_the_audio_is_passed_through` | no grid, or paused: the audio's own hits, unchanged |
| `silence_shows_nothing` | silent capture: nothing pulses, whatever the grid says |
| `a_new_track_starts_the_evidence_over_but_keeps_the_lag` | confidence resets per track; the lag estimate (a property of the audio path) does not |

## 3. The gallery: every scene in four states, headless

Renders idle, build-up, drop and chorus for every scene to PNG sheets. Takes about two
minutes.

```bash
cd /c/Users/water/Projects/OnSet && mkdir -p D:/Offload/scratch/review-gallery && ONSET_DUMP_DIR=D:/Offload/scratch/review-gallery CARGO_TARGET_DIR=D:/Offload/cargo-target cargo test -j 4 -p onset-render --test gallery every_scene 2>&1 | grep -E "test result|panicked" && explorer "D:\Offload\scratch\review-gallery"
```

Open the `*_sheet.png` files (top left idle, top right build, bottom left drop, bottom
right chorus). Look for:

- **Idle vs chorus**: idle is dimmer and a little greyer in every scene; chorus is brighter and
  fuller. That is the post pass's exposure (0.82 → 1.12) and saturation following the
  director's intensity. Judge whether the swing is the right size; the constants are in
  [renderer.rs](../crates/onset-render/src/renderer.rs) (`tone`) and
  [post.wgsl](../assets/shaders/post.wgsl) (`intensity`).
- **Lasers**: the middle lamps (bass) wear accent 1, the outer lamps (treble) accent 3, in
  between accent 2. **Crystal** and the **Neon Tunnel** struts follow the same rule. Before,
  a per-track seed scrambled which colour meant bass.
- Nothing black, nothing white, no scene missing.

## 4. On the projector (the checks that matter)

Rebuild first: any binary built before today has none of this, and the loader fix means
an older binary silently falls back to its embedded shaders.

```bash
cd /c/Users/water/Projects/OnSet && CARGO_TARGET_DIR=D:/Offload/cargo-target cargo build -j 4 --release -p onset-visuals && echo built
```

Switch on the scope: Show page → Stats (HUD) → tick **Signal scope**, or set `scope =
true` under `[hud]` in the config file (`onset.exe --config-path` prints where it is).
Press `H` on the projector to show the HUD.

### 4a. The beat-lock on a real deck

Play a four-to-the-floor track in rekordbox, any scene (Neon Tunnel or Laser Show make
the kick obvious).

- The `scope` line: `kick [||||||......]` should peak the moment the beat counter ticks and
  decay over the beat. `heard kick` peaks a little later. That gap is the whole point.
- The next line: `audio lag +NN ms over N kicks`. After ~30 kicks the number should settle.
  **Write it down.** Expected: a few tens of ms, positive. Near zero or negative means the
  audio path is faster than assumed and the lock buys little; over ~150 ms means something
  else is in the path.
- `locked to the grid · confidence kick 0.9x snare 0.x`: kick confidence should sit above
  0.8 while the kick drum plays. Snare confidence depends on the track; a track with
  snares on 2 and 4 should read above 0.6.
- Pull the low EQ fully down for eight bars: kick confidence should fall and the visual
  kick with it (within ~4 beats). Push it back: it recovers as fast.
- Pause the deck: the line changes to `hits as heard (no grid, paused or silent)` and the
  visuals follow the audio only.
- Watch the kick with your eyes against the music: it should land *on* the beat, not just
  after it. If it lands early, the lag number is the amount to compensate elsewhere.

### 4b. Structure in every scene

Play a track with a clear breakdown and drop, in Auto or any scene.

- Breakdown: the picture dims and softens. Drop: it steps up. This happens in every scene,
  including the classics (Warp, Cells, Pulse, Ring), which never handled intensity
  themselves.
- Judge the size of the step. Too subtle or too much → the two constants named in part 3.

### 4c. Nothing regressed

- Change scenes with the keys: every scene loads, no HUD error line (red). A red
  `shader ... failed to compile` line means a disk shader does not compile against the
  disk `common.wgsl`; that would be a real bug now, not the old binary mismatch.
- Edit `assets/shaders/lasers.wgsl` while the show runs (change a number): hot reload still
  applies it.

## 5. The replay branch (`refs/heads/claude/interesting-dijkstra-cb6df3`, commit `90f5829`)

Its session's worktree is still checked out at
`.claude/worktrees/interesting-dijkstra-cb6df3`, so it runs from there and nothing is
merged here. Its tests, including the checked-in synthetic recording replaying through a scene:

```bash
cd /c/Users/water/Projects/OnSet/.claude/worktrees/interesting-dijkstra-cb6df3 && CARGO_TARGET_DIR=D:/Offload/cargo-target cargo test -j 4 -p onset-core -p onset-engine -p onset-render 2>&1 | grep -E "^test result|FAILED|panicked|replay"
```

Pass: all `ok`; `tests/replay.rs` runs and the fixture test passes.

Then the thing it is for. Record a real set (rekordbox playing, the show window open):

```bash
cd /c/Users/water/Projects/OnSet/.claude/worktrees/interesting-dijkstra-cb6df3 && CARGO_TARGET_DIR=D:/Offload/cargo-target cargo run --release -p onset-visuals -- --record D:/Offload/scratch/review-set.onsetrec --exit-after 180
```

Replay it through two scenes and dump frames (replace the path if you recorded elsewhere):

```bash
cd /c/Users/water/Projects/OnSet/.claude/worktrees/interesting-dijkstra-cb6df3 && ONSET_RECORDING=D:/Offload/scratch/review-set.onsetrec ONSET_REPLAY_SCENES=synthwave,lasers ONSET_DUMP_DIR=D:/Offload/scratch/review-replay-frames CARGO_TARGET_DIR=D:/Offload/cargo-target cargo test --release -j 4 -p onset-render --test replay replay_recording -- --ignored --nocapture
```

Look for: frames in `D:\Offload\scratch\review-replay-frames` that follow the set (the
drop you heard shows as a bright frame at the right second), and a per-scene timing
summary printed. A recording is engine state only (no audio, no file paths), but keep real
set recordings out of the repository all the same.

## 6. The particle branch (`refs/heads/claude/kind-shamir-4b8d38`, commit `0c464ca`)

Likewise checked out at `.claude/worktrees/kind-shamir-4b8d38`. Its headless test proves a kick adds light and a drop adds more, both fading:

```bash
cd /c/Users/water/Projects/OnSet/.claude/worktrees/kind-shamir-4b8d38 && CARGO_TARGET_DIR=D:/Offload/cargo-target cargo test -j 4 -p onset-render --test particles -- --nocapture 2>&1 | grep -E "^test |test result|light"
```

On the projector, from that worktree (settings: Show page → Particles, on by default,
**Sparks** slider):

```bash
cd /c/Users/water/Projects/OnSet/.claude/worktrees/kind-shamir-4b8d38 && CARGO_TARGET_DIR=D:/Offload/cargo-target cargo run --release -p onset-visuals
```

- Kicks throw sparks from the centre, snares a wider spray, a drop a full-screen throw, a
  passed cue a puff in the cue's colour. Colours come from the track's accents.
- Frame rate: with the HUD on, compare `frame` ms with Particles on and off at your usual
  quality. On the Iris Xe at Medium the baseline was ~52 fps; more than a few ms lost
  means the pool or the update cost needs trimming at that quality.
- Low quality should visibly use fewer particles.

The worktrees belong to their sessions; archiving a session in the app removes its
worktree once its branch is merged or discarded.

## 7. Decisions this review feeds

1. The measured audio lag (4a): record it in PLAN_PROGRESS with the date and machine.
2. Keep, tune or revert the exposure/saturation swing (3, 4b).
3. Merge the replay branch, then record one full set as the fixture everything else is
   tuned against.
4. Merge the particle branch, with the Sparks default adjusted to taste, or hold it until
   the frame-rate cost is acceptable.
