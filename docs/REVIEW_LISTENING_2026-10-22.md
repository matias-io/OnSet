# Review: listening — La Danza's drop, analysis labels and transitions

Listening checks that no code can close: where La Danza's drop really is (its cues stay
unapproved until then), verified analysis labels for the M3/V2 benchmark, and whether the
planned transitions work (M4). Part 4 combines a live run of the test set with the visual
review in [REVIEW_VISUALS_2026-10-22.md](REVIEW_VISUALS_2026-10-22.md), parts 4a, 4b and 5,
so one session at the decks serves both.

**Before starting:** the installed OnSet must include `d62880b` (the verdict buttons on the
Transitions page) and `df069c7` (the analysis fixes). The session that builds installers is
installing one from `origin/merge-autocue`, then re-analysing La Danza and six other songs.
Wait for that; with an older build the verdict buttons are missing.

Use headphones or the monitors you mix on. Parts 1–3 happen in OnSet with rekordbox closed
or idle. Part 4 needs rekordbox, the controller and the projector.

## 1. La Danza's drop (about 15 minutes)

Track review → **La Danza**. The analyses disagree on where the drop is:

| Source | Bar | Time |
|---|---|---|
| Energy baseline (proposed today's cues) | 2 | 0:04 |
| rekordbox's phrase analysis | 24 | 0:46 |
| September's cue, still in rekordbox | 32 | 1:01 |
| Structure model ("drop" label) | ~66 | 2:06 |

1. Play from 0:00 and listen through to about 2:30. The drop is the first point where the
   full kick and bass arrive together after the intro.
2. Corrections → More tools → **Benchmark labels (listening-verified)**. Pause at the drop,
   set **First full groove** to Present and press **Use playhead**. Do the same for **First
   breakdown**, **Later drop** and **Outro** (set Absent if the track has none). Enter the
   keys you hear as acceptable (rekordbox lists 12A) and the BPM (126.27 from rekordbox,
   unless you hear otherwise). Tick **I verified these labels by listening** and **Save**.
3. Cues tab: drag **D** to the drop. If the drop is at bar 16 or later, drag **C** to 16 bars
   before it; at bar 32 or later, **B** to 32 bars before it. Then approve the cues you agree
   with. Nothing is written to rekordbox until you preview and apply.

What would mean a problem: no single clear drop (a track that builds without one). Say so in
the label notes and leave **First full groove** at the bar where the groove is complete.

## 2. Analysis labels for the benchmark (about 10 minutes per track)

The same **Benchmark labels** panel, for these tracks. They are today's test tracks, so their
cues and grids were already looked at:

| Track | Split |
|---|---|
| Losing It | development |
| FISHER – STAY | development |
| World Hold On (Fisher Rework) | development |
| Lock In | development |
| Joya (Radio Edit) | development |
| Safe And Sound | development |
| Beautiful Places | development |
| Ray Of Solar | development |
| Stop It | validation |
| Midnight (The Hanging Tree) | validation |
| Follow | **test (held out)** |

For each track: the four events with **Use playhead**, acceptable keys and BPM, the verified
tick and **Save**. Only labels you verified by ear count. *Follow* is in the held-out split:
label it like the rest, but its result is only looked at when comparing releases, never for
tuning.

## 3. The set's transitions (about 5 minutes each)

The test set "OnSet test 2026-10-22" has four planned transitions:

| From → to | Technique | Outgoing from | Incoming from | Length | Pitch |
|---|---|---|---|---|---|
| Stop It → FISHER – STAY | Filter transition | 2:29 | 0:15 | 15 s | −3.12% |
| FISHER – STAY → La Danza | EQ / bass swap | 2:00 | 0:00 | 31 s | −1.80% |
| La Danza → World Hold On | EQ / bass swap | 3:06 | 0:24 | 31 s | −2.36% |
| World Hold On → Losing It | Filter transition | 2:04 | 1:53 | 15 s | −0.80% |

For each pair: Transitions page → **Outgoing** and **Incoming** → **Find options**. Find the
card with that technique and times; re-analysis may shift a time by a beat or two. Then:

1. **Render preview** and listen. Then **Audition both decks** to hear the two sources.
2. Choose **Works**, **Needs changes** or **Doesn't work**, keep "Rendered preview", note what
   you would change (for example "bass clash at +16 s" or "start 8 bars later") and **Save
   verdict**.

Also listen to one or two other options per pair: a verdict on an option the planner ranked
lower is just as useful.

## 4. Live: the set at the decks, with the visual review

One run of the four transitions on the controller serves both reviews. Set up as in
[REVIEW_VISUALS_2026-10-22.md](REVIEW_VISUALS_2026-10-22.md) part 4: projector running, HUD
on (`H`), **Signal scope** ticked.

One projector build does everything: `merge-autocue` has the beat lock (`9e4d9be`), the
structure lift (`527523b`) and the set recorder (`41dba6c`). Build it as in the visual
review's part 4 and run it with `--record`, so this set becomes the recording the visuals
are tuned against:

```bash
cd /c/Users/water/Projects/OnSet && CARGO_TARGET_DIR=D:/Offload/cargo-target cargo run -j 4 --release -p onset-visuals -- --record D:/Offload/scratch/review-set.onsetrec
```

In rekordbox, load the playlist **OnSet test 2026-10-22** and mix it using the table in part
3: start each incoming track at its time, with the technique shown, pitch per the column.

During the set:

- **Transitions:** after each one, note your verdict, then in OnSet save it again with
  **Played in a set**. A transition can work in the render and not live, or the other way
  round; both verdicts are kept.
- **Beat lock (visual 4a):** the two EQ / bass swaps pull the outgoing low EQ down for most
  of 31 s, the case the visual review describes. The beat lock follows the master deck's
  grid, and in a beat-matched mix the incoming kick confirms that grid too. So expect kick
  confidence to dip while the outgoing lows are down and recover as the incoming bass lands,
  not a clean fall to zero. The visual kick should stay on the beat throughout.
- **Structure (visual 4b):** each track's breakdown and drop should dim and lift the
  picture. La Danza is where it matters: if the picture lifts at 0:04 and not at the drop you
  labelled in part 1, that is the analysis's error showing.
- Write down the HUD's `audio lag +NN ms` once it settles; the visual review asks for it.

## 5. After the review

Tell me when parts 1–3 are saved; part 4 notes can be spoken or typed. Then I:

- run the benchmark against your verified labels and report beat, event and key accuracy,
  separately for development, validation and the held-out track;
- correct La Danza's analysis to your drop if the cue move in part 1 needs follow-up;
- summarise the transition verdicts by technique, render against live;
- record the results and dates in PLAN_PROGRESS (M3, M4, V2).
