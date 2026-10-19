> Historical energy-detector and opinion-merging method. This describes the retained legacy scripts, not the full desktop analysis/resolution pipeline. See [current analysis status](IMPLEMENTATION_STATUS.md) and [desktop operation](DESKTOP.md). Old agreement/confidence labels are provisional; no held-out listening accuracy is established.

# Method: how the cues are detected and merged

All detection lives in `autocue_core.py`; both front-ends call it.

## 1. Beat grid

- **Linux / XML route (`autocue.py`):** madmom `RNNDownBeatProcessor` +
  `DBNDownBeatTrackingProcessor(beats_per_bar=[4], 80–180 BPM)`. Output: beat times
  with beat-in-bar; downbeats = beat 1. BPM = 60 / median beat interval.
  `tempo_cv` = std/mean of beat intervals; > 3 % is flagged "tempo unstable".
  Known failure: double/half-time on some tracks (Moonshine → 171 instead of ~86).
- **Windows / DB route (`autocue_rb.py`):** rekordbox's own grid from the track's
  ANLZ `.DAT` (`PQTZ` tag) via `pyrekordbox`; phrases from `.EXT` (`PSSI`, XOR-masked,
  pyrekordbox un-masks). Requires the track to be analysed in rekordbox first.

All positions are **bar indices** into the downbeat array; cue times are the
downbeat times. This is why cues land exactly on the grid.

## 2. Energy curves (`bar_energy`)

Audio → mono float32 (soundfile; ffmpeg fallback for m4a). Per bar:

- `full` = RMS dB of the whole signal
- `low`  = RMS dB after a 4th-order Butterworth band-pass 20–150 Hz
- `hi`   = RMS dB after band-pass 500–8000 Hz
- `E = 0.65·low + 0.35·full` (the "combined" curve; `BASS_WEIGHT`)
- A trailing partial bar (< half a bar long) is dropped.

Both `E` and `hi` are stored in `analysis.json` (`energy`, `energy_hi`) so detection
can be re-run offline in milliseconds.

## 3. Section detection (`find_sections`)

Windows use **medians**, which ignore 1–2-bar fills/drop-outs.
`hi = 90th percentile of E` ("high energy" reference).

**Drop (D).** Candidates are bars `i` (from 4 to 70 % of the track) with
`median(E[i:i+8]) − median(E[i−4:i]) ≥ 4 dB` and both `median(E[i:i+8])` and
`median(E[i:i+4]) ≥ hi − 4`. Among candidates, the earliest whose jump is
≥ `0.3 × max jump` wins (`DROP_REL_TO_MAX`; was 0.6, which skipped smaller first
drops). The position is refined to the sharpest single-bar rise within −2..+5 bars,
then snapped to the 4-bar grid from bar 0 if within 1 bar (otherwise flagged "not on
4-bar phrase"). If no candidate and the first 8 bars are already high → D = bar 0
("starts at full energy"). Jump < 8 dB → "weak drop" flag.

**Breakdown (E).** First `j ≥ D+8` with `median(E[j−4:j]) − median(E[j:j+8]) ≥ 4 dB`
and both `median(E[j:j+8])` and `median(E[j:j+4])` below `median(E[D:D+8]) − 4 dB`
(i.e. judged against the drop's own level). Refined to the sharpest fall, snapped to
the 4-bar grid anchored at D (tolerance 2).

**Outro (F).** Scanning backwards, the last `k` with a sustained ≥ 4 dB fall and no
recovery (75th percentile of the remainder < hi − 4). Tried on `E` first; if none, on
the mid/high curve with a 5 dB threshold ("layers leave, kick stays"). Snapped to
D's grid. If F ≤ E, the breakdown *is* the outro (flag) and F is dropped.

**Second drop (G).** First sustained ≥ 4 dB rise after `E + 4`, snapped to D's grid.

Flags produced: `weak drop`, `no drop found`, `starts at full energy`, `drop snapped
a->b`, `drop not on 4-bar phrase`, `short intro (n bars)`, `drop is late in track`,
`no breakdown found`, `no outro found (hard ending?)`, `breakdown runs to end`,
`outro from mid/high band`.

## 4. Cue derivation (`derive_cues`)

- A = bar 0; D = drop; B = D−32 and C = D−16 **only if > 0**; E, F as found;
  G (opt-in `--second-drop`); L (opt-in `--end-loop N`): an N-bar memory loop on the
  last N-bar phrase aligned to D's 4-bar grid, end = next downbeat or track end.
- Colours (rekordbox memory-cue index): A green(4) B yellow(3) C orange(2) D red(1)
  E blue(6) F purple(7) G pink(0) L aqua(5).

## 5. Second opinions and merging

An opinion file is `{path: {drop, breakdown, second_drop, outro: "m:ss"|null, …}}`.
Sources used: Gemini (`gemini_check.py`), Claude spectrogram reads
(`claude_spec.json`), rekordbox phrases (DB route only: first "Chorus" = drop, next
Down/Verse/Bridge = breakdown, "Outro" = outro).

- **One opinion (`merge_llm`)**: each LLM time is snapped to the nearest downbeat,
  then refined to the sharpest energy edge within ±2 bars. Missing sections are
  filled from the LLM. Disagreements > 2 bars are flagged (`LLM puts drop at …`);
  with `--prefer-gemini` the LLM position is used. BPM double/half mismatch is
  flagged. LLM extras (energy, genre, vocals, notes) go to `extra` → comments.
- **Two or more opinions (`vote_sections`)**: energy + each LLM votes per section;
  the value with most agreeing sources (within 2 bars) wins, ties → energy, then
  first LLM. Flags: `drop: 2/3 agree (energy used; llm2=…)` or `drop: no agreement`.

## 6. Review status (`review_status`)

`CHECK` if any flag contains: `LLM puts drop`, `drop: no agreement`, `drop: 2/3`,
`no drop`, `no LLM`, `BPM`, `confidence low`, `tempo unstable`, `late in track`.
Everything else (E/F/G nudges, weak drop, snapping) is OK but visible in `flags`.
Delivered set: 42 CHECK / 57 OK.

## 7. Tuning (`tune.py`)

Sweeps `drop_min_jump ∈ {4,5,6,8}`, `drop_rel ∈ {0.3,0.4,0.5,0.6,0.8}`,
`break_min ∈ {4,5,6,8}`, `high_pctl ∈ {80,85,90}` over the cached curves, scoring
agreement (≤ 2 bars) with a labels file; score = drops + 0.5·breakdowns + 0.5·outros.
Against `claude_spec.json`: defaults (5, 0.6, 5, 85) → drop 74/96; best (4, 0.3, 4,
90) → drop 79/98, breakdown 46/86, outro 14/32. Those are the current defaults.

## 8. Spectrogram reading (Claude as second opinion)

`spectro.py` renders a 24×6 in, 100 dpi mel spectrogram (22.05 kHz, n_fft 2048, hop
512, 96 mels, 0–8 kHz, −70..0 dB, magma) with white bar lines every 4 bars (thicker
every 16) and ticks every 8 bars labelled `bar\nm:ss`. A reader identifies: the bar
where the low band (< 150 Hz) becomes solid/continuous (drop), where it goes dotted
or empty for ≥ 8 bars (breakdown), returns (second drop), and where upper layers
thin for the final section (outro). `spec_record.py` converts `name | D | E | G | F |
conf | note` lines into the opinion-file format. Verified on FISHER, Samsara, Abyss
before use. Limits: vocals not judged; pop structures and continuous-sub tracks are
ambiguous.
