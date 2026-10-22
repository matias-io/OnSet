//! The guided calibration for a new rekordbox version: find a deck's playback position in
//! memory and describe the object that holds it, so `offsets/<version>.toml` can find every
//! deck at runtime by scanning for that description (see `DeckSignature`).
//!
//! The operator loads a track and leaves it paused, then plays it, pauses it and plays it
//! again. rekordbox keeps hundreds of audio-rate clocks running whatever the decks do, so a
//! first scan with the deck paused records them and they are ignored. A counter that starts
//! advancing when the deck plays, holds still while it is paused and resumes from the same
//! value is the deck's position; everything else that ticks at an audio rate (time-since-play
//! clocks, output counters) fails one of the steps. Around the position sits the player object,
//! which carries pointers into the rekordbox module (its vtables and statics). Those
//! pointers at their distances from the position are the signature. The calibrated deck's
//! tempo and pitch are recorded when they sit beside the position. The track's identity comes
//! from rekordbox's player objects where the offsets file names them (`TrackIdFields`, found
//! by hand so far), else from the files rekordbox keeps open, matched by length and BPM.
//! Several runs can be merged into the anchors they all found (`merge_offsets`).
//!
//! The CLI and the launcher both drive this; only the reporter differs.
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, bail};

use super::REKORDBOX_EXE;
use super::calibrate::{rarest_first, shared_signature};
use super::offsets::{Anchor, DeckSignature, Offsets, PositionFormat, TempoFields};
use super::process::Process;
use super::reader::{Mem, track_bpm_from};
use super::scan::{
    Counter, CounterFormat, confirm_counters, find_bytes, find_sample_counters, find_u64_values,
};

/// Counters that must stop for the pause to count (the position has a few copies).
const MIN_STOPPED: usize = 3;
/// Bytes below and above the position field searched for module pointers.
const ANCHOR_BELOW: u64 = 0x400;
const ANCHOR_ABOVE: u64 = 0x40;
/// A usable signature has at least this many anchors.
const MIN_ANCHORS: usize = 3;
/// Hits counted when ranking anchors by rarity; the rarest is scanned for at runtime.
const RARITY_CAP: usize = 24;
/// Position candidates whose surroundings are examined, best first.
const CANDIDATES: usize = 24;

const POLL: Duration = Duration::from_millis(500);
/// Settling time after a change is seen, so rekordbox finishes its own bookkeeping first.
const SETTLE: Duration = Duration::from_millis(800);

/// The steps the operator performs, in order, as the launcher lists them up front.
pub const STEPS: &[&str] = &[
    "Load a track on any deck and leave it paused, at least a minute before any loop or the end",
    "Press play on that deck when asked",
    "Pause that deck",
    "Press play on it again",
];

/// One calibration run's settings and its way of talking to the operator.
pub struct Session<'a> {
    /// How long each step may take the operator before the calibrator gives up on it.
    pub step_limit: Duration,
    /// Receives every progress line (the CLI prints them, the launcher shows them).
    pub report: &'a mut dyn FnMut(&str),
    /// Set by the caller to abandon the run at the next opportunity.
    pub cancel: &'a AtomicBool,
}

impl Session<'_> {
    fn say(&mut self, text: impl AsRef<str>) {
        (self.report)(text.as_ref());
    }

    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// Sleeps in short slices so a cancel request is honoured within a tenth of a second.
    fn sleep(&self, total: Duration) -> anyhow::Result<()> {
        let deadline = Instant::now() + total;
        while Instant::now() < deadline {
            if self.cancelled() {
                bail!("calibration cancelled");
            }
            std::thread::sleep(Duration::from_millis(100).min(deadline - Instant::now()));
        }
        Ok(())
    }
}

/// Announces a step and returns once `seen` reports the change it describes, or fails
/// once the step limit passes.
fn wait_for(
    s: &mut Session<'_>,
    instruction: &str,
    mut seen: impl FnMut() -> bool,
) -> anyhow::Result<()> {
    s.say(format!(
        "{instruction} (waiting up to {} s)",
        s.step_limit.as_secs()
    ));
    let deadline = Instant::now() + s.step_limit;
    loop {
        if s.cancelled() {
            bail!("calibration cancelled");
        }
        if seen() {
            s.say("  seen");
            s.sleep(SETTLE)?;
            return Ok(());
        }
        if Instant::now() >= deadline {
            bail!("{instruction}: not seen in time");
        }
        s.sleep(POLL)?;
    }
}

fn read_counter(p: &Process, addr: u64, format: CounterFormat) -> Option<f64> {
    match format {
        CounterFormat::I64 => p.read_i64(addr).ok().map(|v| v as f64),
        CounterFormat::I32 => p.read_i32(addr).ok().map(f64::from),
        CounterFormat::F64 => p.read_f64(addr).ok(),
    }
}

/// A rate in units per second that looks like audio playback at up to ±50 % pitch.
fn audio_rate(rate: f64) -> bool {
    (11_000.0..=150_000.0).contains(&rate)
}

/// Reads every counter now.
fn snapshot_counters(p: &Process, counters: &[Counter]) -> Vec<Option<f64>> {
    counters
        .iter()
        .map(|c| read_counter(p, c.addr, c.format))
        .collect()
}

/// True when at least one counter advances at an audio rate over `dt`.
fn any_counter_moving(p: &Process, counters: &[Counter], dt: Duration) -> bool {
    let before = snapshot_counters(p, counters);
    std::thread::sleep(dt);
    let secs = dt.as_secs_f64();
    counters.iter().zip(&before).any(|(c, a)| {
        matches!((a, read_counter(p, c.addr, c.format)), (Some(a), Some(b)) if audio_rate((b - a) / secs))
    })
}

/// How many counters read the same value twice `dt` apart.
fn count_still(p: &Process, counters: &[Counter], dt: Duration) -> usize {
    let before = snapshot_counters(p, counters);
    std::thread::sleep(dt);
    counters
        .iter()
        .zip(&before)
        .filter(|(c, a)| {
            matches!((a, read_counter(p, c.addr, c.format)), (Some(a), Some(b)) if (a - b).abs() < 1e-9)
        })
        .count()
}

/// Counters advancing now that were not already advancing in the paused baseline: the ones
/// playback started. rekordbox's always-running clocks are in both scans and drop out.
fn started_with_play(baseline: &BTreeSet<u64>, confirmed: Vec<Counter>) -> Vec<Counter> {
    confirmed
        .into_iter()
        .filter(|c| !baseline.contains(&c.addr))
        .collect()
}

/// Groups counters by the second they show, largest group first (the position exists in
/// several copies), then puts 32-bit integers first, since a 32-bit field read as 64 bits
/// drags its neighbour along.
fn rank(confirmed: Vec<Counter>) -> Vec<Counter> {
    let mut groups: BTreeMap<i64, Vec<Counter>> = BTreeMap::new();
    for c in confirmed {
        groups
            .entry((c.seconds(c.second) * 2.0).round() as i64)
            .or_default()
            .push(c);
    }
    let mut ordered: Vec<Vec<Counter>> = groups.into_values().collect();
    ordered.sort_by_key(|g| std::cmp::Reverse(g.len()));
    let mut best: Vec<Counter> = ordered.into_iter().flatten().collect();
    best.sort_by_key(|c| match c.format {
        CounterFormat::I32 => 0,
        CounterFormat::I64 => 1,
        CounterFormat::F64 => 2,
    });
    best
}

/// One line per group of counters showing the same second (largest first, at most eight):
/// what the operator can compare with the deck's displayed time when a step is not seen.
fn group_summary(counters: &[Counter]) -> Vec<String> {
    let mut groups: BTreeMap<i64, (usize, f64, BTreeSet<&'static str>)> = BTreeMap::new();
    for c in counters {
        let secs = c.seconds(c.second);
        let entry = groups
            .entry((secs * 2.0).round() as i64)
            .or_insert((0, secs, BTreeSet::new()));
        entry.0 += 1;
        entry.2.insert(match c.format {
            CounterFormat::I32 => "i32",
            CounterFormat::I64 => "i64",
            CounterFormat::F64 => "f64",
        });
    }
    let mut ordered: Vec<_> = groups.into_values().collect();
    ordered.sort_by_key(|g| std::cmp::Reverse(g.0));
    ordered
        .into_iter()
        .take(8)
        .map(|(n, secs, formats)| {
            let formats: Vec<_> = formats.into_iter().collect();
            format!("{n} counter(s) at {secs:.1} s ({})", formats.join(", "))
        })
        .collect()
}

/// Paused scans whose counters make up the baseline. One scan misses many of rekordbox's
/// hundreds of clocks (on 7.2.18 a single pass caught 161 of about 480), and every clock it
/// misses would later pass for the deck starting.
const BASELINE_SCANS: usize = 3;

/// The addresses of every audio-rate counter running while the operator's deck is paused.
fn paused_baseline(s: &mut Session<'_>, p: &Process) -> anyhow::Result<BTreeSet<u64>> {
    s.say("  recording what runs with the deck paused...");
    let mut baseline = BTreeSet::new();
    for _ in 0..BASELINE_SCANS {
        baseline.extend(scan_counters(s, p).iter().map(|c| c.addr));
        if s.cancelled() {
            bail!("calibration cancelled");
        }
    }
    s.say(format!(
        "  {} counter(s) run while paused (rekordbox's own clocks); these are ignored",
        baseline.len()
    ));
    Ok(baseline)
}

/// One pass over rekordbox's memory for values advancing at an audio rate, confirmed twice.
fn scan_counters(s: &mut Session<'_>, p: &Process) -> Vec<Counter> {
    s.say("  scanning for playback positions (two passes over rekordbox's memory)...");
    let started = Instant::now();
    let candidates = find_sample_counters(p, Duration::from_millis(400), 0.2);
    let confirmed = confirm_counters(p, &candidates, Duration::from_millis(400), 0.2);
    let mut confirmed = confirm_counters(p, &confirmed, Duration::from_millis(700), 0.2);
    confirmed.retain(|c| c.nominal_hz >= 22_000.0 && c.seconds(c.second) < 30.0 * 60.0);
    s.say(format!(
        "    done in {:.0} s",
        started.elapsed().as_secs_f64()
    ));
    confirmed
}

/// Seconds between asking for play and scanning, so the press lands before the scan reads
/// the deck. A scan that began first could pass the deck while still paused; covering that
/// took a second scan, which kept the deck playing long enough to reach a short track's loops.
const PLAY_COUNTDOWN_S: u64 = 5;

/// Playback counters that started with play, best first (see `rank`). The scan waits for a
/// countdown after asking for play, and scans again if the press still came late.
fn ranked_counters(
    s: &mut Session<'_>,
    p: &Process,
    baseline: &BTreeSet<u64>,
) -> anyhow::Result<Vec<Counter>> {
    s.say(format!(
        "Step 2: press play on that deck now (scanning in {PLAY_COUNTDOWN_S} s)."
    ));
    s.sleep(Duration::from_secs(PLAY_COUNTDOWN_S))?;
    let deadline = Instant::now() + s.step_limit;
    let mut baseline = baseline.clone();
    let started = loop {
        let started = started_with_play(&baseline, scan_counters(s, p));
        if started.len() >= MIN_STOPPED {
            break started;
        }
        // Too few to be a deck: clocks the paused scans missed. Ignore them from now on.
        baseline.extend(started.iter().map(|c| c.addr));
        if Instant::now() >= deadline {
            bail!(
                "no deck started playing: no new playback counter appeared after the paused scan. \
                 Start again with the deck paused and press play only when asked"
            );
        }
        s.say("  no newly playing deck found yet; scanning again");
        s.sleep(Duration::from_secs(2))?;
    };
    let best = rank(started);
    s.say(format!(
        "  {} counter(s) confirmed; the largest group reads {:.1} s",
        best.len(),
        best[0].seconds(best[0].second)
    ));
    for line in group_summary(&best) {
        s.say(format!("    {line}"));
    }
    Ok(best)
}

/// The deck's absolute position: a counter that moves while the deck plays, holds still
/// while it is paused, and continues from the same value when play resumes.
fn absolute_position_counters(s: &mut Session<'_>, p: &Process) -> anyhow::Result<Vec<Counter>> {
    let baseline = paused_baseline(s, p)?;
    let counters = ranked_counters(s, p, &baseline)?;
    wait_for(
        s,
        "Step 3: pause that deck (press play/pause once).",
        || {
            // Every counter here started with play; a few of them stopping is the pause
            // (time-since-play clocks and any other deck keep running).
            count_still(p, &counters, Duration::from_millis(300)) >= MIN_STOPPED
        },
    )?;
    let held = snapshot_counters(p, &counters);
    s.sleep(Duration::from_millis(500))?;
    let again = snapshot_counters(p, &counters);
    let mut keep = Vec::new();
    for ((c, a), b) in counters.iter().zip(&held).zip(&again) {
        if let (Some(a), Some(b)) = (a, b)
            && (a - b).abs() < 1e-9
        {
            keep.push((*c, *a));
        }
    }
    s.say(format!(
        "  {} counter(s) hold still while paused",
        keep.len()
    ));
    let held_counters: Vec<Counter> = keep.iter().map(|(c, _)| *c).collect();
    wait_for(s, "Step 4: press play on that deck again.", || {
        any_counter_moving(p, &held_counters, Duration::from_millis(300))
    })?;
    let t0 = Instant::now();
    let now: Vec<Option<f64>> = keep
        .iter()
        .map(|(c, _)| read_counter(p, c.addr, c.format))
        .collect();
    s.sleep(Duration::from_millis(500))?;
    let dt = t0.elapsed().as_secs_f64();
    let absolute: Vec<Counter> = keep
        .iter()
        .zip(&now)
        .filter_map(|((c, at_pause), v0)| {
            let v0 = (*v0)?;
            let v1 = read_counter(p, c.addr, c.format)?;
            let moving = audio_rate((v1 - v0) / dt);
            // Continued from where it paused: never below it, and not restarted from zero.
            let continued = v1 >= *at_pause && *at_pause > 0.0;
            (moving && continued).then_some(*c)
        })
        .collect();
    s.say(format!(
        "  {} counter(s) resumed from the paused value: absolute positions",
        absolute.len()
    ));
    if absolute.is_empty() {
        bail!("no counter behaved like an absolute position across pause and play");
    }
    Ok(absolute)
}

/// Module pointers around a position field: (offset from the field, module-relative
/// value), 8-byte aligned, within the window.
fn anchors_around(p: &Process, pos: u64) -> Vec<Anchor> {
    let start = pos.saturating_sub(ANCHOR_BELOW);
    let len = usize::try_from(ANCHOR_BELOW + ANCHOR_ABOVE).unwrap_or(0x440);
    let mut buf = vec![0u8; len];
    // The window may straddle an unreadable page; shrink from the bottom until it reads.
    let mut from = start;
    while from < pos {
        let want = usize::try_from(pos + ANCHOR_ABOVE - from).unwrap_or(0);
        if p.read_exact(from, &mut buf[..want]).is_ok() {
            buf.truncate(want);
            break;
        }
        from += 0x100;
    }
    if from >= pos {
        return Vec::new();
    }
    let mut out = Vec::new();
    let first_aligned = (8 - (from % 8)) % 8;
    let mut i = usize::try_from(first_aligned).unwrap_or(0);
    while i + 8 <= buf.len() {
        let v = u64::from_le_bytes(buf[i..i + 8].try_into().unwrap_or([0; 8]));
        if p.is_in_module(v) {
            let addr = from + i as u64;
            out.push(Anchor {
                offset: addr.cast_signed() - pos.cast_signed(),
                module_offset: v - p.module_base,
            });
        }
        i += 8;
    }
    out
}

/// Among the position candidates, the anchor set shared by the most of them (the deck
/// objects all have the same layout, so the real position's anchors repeat per loaded deck
/// while stray copies of the position have none or different ones).
fn best_signature(
    s: &mut Session<'_>,
    p: &Process,
    counters: &[Counter],
) -> anyhow::Result<(DeckSignature, Counter)> {
    let mut by_set: BTreeMap<Vec<(i64, u64)>, Vec<Counter>> = BTreeMap::new();
    for c in counters.iter().take(CANDIDATES) {
        let anchors = anchors_around(p, c.addr);
        if anchors.len() < MIN_ANCHORS {
            continue;
        }
        let key: Vec<(i64, u64)> = anchors
            .iter()
            .map(|a| (a.offset, a.module_offset))
            .collect();
        by_set.entry(key).or_default().push(*c);
    }
    let (key, members) = pick_layout(s, p, by_set)
        .context("no position candidate sits in an object with module pointers")?;
    let mut anchors: Vec<Anchor> = key
        .iter()
        .map(|(offset, module_offset)| Anchor {
            offset: *offset,
            module_offset: *module_offset,
        })
        .collect();
    s.say(format!(
        "  {} anchor(s) around {} position field(s) of the same layout",
        anchors.len(),
        members.len()
    ));
    // The runtime scans for the first anchor's bytes: make that the rarest one. One pass
    // over the heap counts every anchor (a pass per anchor took minutes).
    let candidates: Vec<Anchor> = anchors.iter().take(6).cloned().collect();
    let values: Vec<u64> = candidates
        .iter()
        .map(|a| p.module_base + a.module_offset)
        .collect();
    let counts = find_u64_values(p, &values, RARITY_CAP);
    let mut rarity: Vec<(usize, Anchor)> = candidates
        .iter()
        .zip(&counts)
        .map(|(a, found)| {
            let hits = found.len();
            let sign = if a.offset < 0 { "-" } else { "+" };
            s.say(format!(
                "    module+{:#x} at {sign}{:#x}: {hits} occurrence(s)",
                a.module_offset,
                a.offset.unsigned_abs()
            ));
            (hits, a.clone())
        })
        .collect();
    rarity.sort_by_key(|(hits, a)| (*hits, a.offset));
    let first = rarity[0].1.clone();
    anchors.retain(|a| *a != first);
    anchors.insert(0, first);
    let counter = members
        .iter()
        .copied()
        .min_by_key(|c| match c.format {
            CounterFormat::I32 => 0,
            CounterFormat::I64 => 1,
            CounterFormat::F64 => 2,
        })
        .unwrap_or(counters[0]);
    Ok((
        DeckSignature {
            anchors,
            max_decks: 4,
            master_flag: None,
            tempo: None,
            track_id: None,
        },
        counter,
    ))
}

/// rekordbox 7.2.19 keeps the deck's tempo (f32 BPM) just below the position and its pitch
/// (f32 fraction) just above it. Kept when the calibrated deck reads a plausible pair there.
fn tempo_fields(p: &Process, pos: u64) -> Option<TempoFields> {
    let fields = TempoFields {
        bpm_offset: -4,
        pitch_offset: 4,
    };
    let bpm = p
        .read_f32(pos.checked_add_signed(fields.bpm_offset)?)
        .ok()?;
    let pitch = p
        .read_f32(pos.checked_add_signed(fields.pitch_offset)?)
        .ok()?;
    track_bpm_from(bpm, pitch).map(|_| fields)
}

/// Deck objects the runtime keeps. The reader takes them in address order, which is not deck
/// order, and rekordbox 7.2.19 has five objects with the deck layout (four decks and a
/// player), so a cap of four can drop the deck that is playing.
fn deck_limit(found: usize) -> usize {
    found.clamp(4, 8)
}

/// Among the layouts the position candidates sit in, the deck layout. Each layout is cut down
/// to the pointers its objects share (as `refine` does) and ranked by `layout_rank`: not the
/// most objects, which on 7.2.19 chose a generic structure found 15 times with three shared
/// pointers and no tempo beside the position, over the deck layout found 5 times with eight.
/// rekordbox also keeps the playhead in its player objects, twice in each; a layout holding
/// two candidates in one object is the player, ranked last.
fn pick_layout(
    s: &mut Session<'_>,
    p: &Process,
    by_set: BTreeMap<LayoutKey, Vec<Counter>>,
) -> Option<(LayoutKey, Vec<Counter>)> {
    if by_set.len() < 2 {
        return by_set.into_iter().next();
    }
    let values: Vec<u64> = by_set
        .keys()
        .flatten()
        .map(|(_, module_offset)| p.module_base + module_offset)
        .collect::<BTreeSet<u64>>()
        .into_iter()
        .collect();
    let hits = find_u64_values(p, &values, SIBLING_HITS);
    let objects = |key: &[(i64, u64)], primary: u64| -> (usize, usize) {
        let anchors: Vec<Anchor> = key
            .iter()
            .map(|(offset, module_offset)| Anchor {
                offset: *offset,
                module_offset: *module_offset,
            })
            .collect();
        let found: Vec<Vec<u64>> = key
            .iter()
            .map(|(_, module_offset)| {
                values
                    .iter()
                    .position(|v| *v == p.module_base + module_offset)
                    .map(|i| hits[i].clone())
                    .unwrap_or_default()
            })
            .collect();
        let shared = shared_signature(p, primary, &anchors, &found);
        if shared.anchors.len() < MIN_ANCHORS {
            (0, 0)
        } else {
            (shared.decks.len(), shared.anchors.len())
        }
    };
    let scored: Vec<(LayoutRank, LayoutKey, Vec<Counter>)> = by_set
        .into_iter()
        .map(|(k, v)| {
            let (found, shared) = objects(&k, v[0].addr);
            let rank = layout_rank(
                !holds_twice(&v),
                found,
                shared,
                tempo_fields(p, v[0].addr).is_some(),
                v.len(),
            );
            (rank, k, v)
        })
        .collect();
    for (rank, k, v) in &scored {
        s.say(format!(
            "  layout with {} anchor(s) around {} candidate(s){}: finds {} object(s) sharing {}, tempo beside the position: {}",
            k.len(),
            v.len(),
            if rank.single { "" } else { " in one object" },
            rank.objects,
            rank.shared,
            if rank.tempo { "yes" } else { "no" }
        ));
    }
    scored
        .into_iter()
        .max_by_key(|(rank, _, _)| rank.key())
        .map(|(_, k, v)| (k, v))
}

/// Deck objects a layout can plausibly be found as: rekordbox keeps one object per deck and a
/// few more of the same class, never dozens.
const PLAUSIBLE_OBJECTS: std::ops::RangeInclusive<usize> = 2..=8;

/// What decides between layouts, best first: one candidate per object (not the player), a
/// plausible object count, tempo fields beside the position, more shared pointers, then more
/// objects and more candidates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LayoutRank {
    single: bool,
    objects: usize,
    shared: usize,
    tempo: bool,
    candidates: usize,
}

impl LayoutRank {
    fn key(&self) -> (bool, bool, bool, usize, usize, usize) {
        (
            self.single,
            PLAUSIBLE_OBJECTS.contains(&self.objects),
            self.tempo,
            self.shared,
            self.objects,
            self.candidates,
        )
    }
}

fn layout_rank(
    single: bool,
    objects: usize,
    shared: usize,
    tempo: bool,
    candidates: usize,
) -> LayoutRank {
    LayoutRank {
        single,
        objects,
        shared,
        tempo,
        candidates,
    }
}

/// A layout: the (offset, module offset) of each module pointer around a position field.
type LayoutKey = Vec<(i64, u64)>;

/// True when two of a layout's candidates sit in one object (closer than its anchor window).
fn holds_twice(candidates: &[Counter]) -> bool {
    let mut addrs: Vec<u64> = candidates.iter().map(|c| c.addr).collect();
    addrs.sort_unstable();
    addrs.windows(2).any(|w| w[1] - w[0] < ANCHOR_BELOW)
}

/// Hits kept per anchor value when looking for sibling decks.
const SIBLING_HITS: usize = 256;

/// Cuts `anchors` (seen around the deck at `primary`) down to those every deck object of
/// the same class holds, so the runtime scan finds all decks, not only the calibrated one.
fn refine(
    s: &mut Session<'_>,
    p: &Process,
    primary: u64,
    anchors: &[Anchor],
) -> anyhow::Result<(Vec<Anchor>, Vec<u64>)> {
    let values: Vec<u64> = anchors
        .iter()
        .map(|a| p.module_base + a.module_offset)
        .collect();
    let hits = find_u64_values(p, &values, SIBLING_HITS);
    let shared = shared_signature(p, primary, anchors, &hits);
    let counts: Vec<usize> = shared
        .anchors
        .iter()
        .map(|a| {
            anchors
                .iter()
                .position(|b| b == a)
                .map_or(0, |i| hits[i].len())
        })
        .collect();
    let mut kept = shared.anchors;
    rarest_first(&mut kept, &counts, shared.decks.len());
    s.say(format!(
        "  {} deck object(s) share {} of {} anchor(s)",
        shared.decks.len(),
        kept.len(),
        anchors.len()
    ));
    if kept.len() < MIN_ANCHORS {
        bail!("the deck objects share too few pointers to recognise them");
    }
    if shared.decks.len() < 2 {
        s.say(
            "  only one deck object found; decks that have never loaded a track may not exist yet",
        );
    }
    Ok((kept, shared.decks))
}

/// Re-derives the shared anchors of an existing offsets file against the running rekordbox,
/// without any deck interaction. Fixes files that only find the deck they were made from.
pub fn refine_existing(dir: &Path, s: &mut Session<'_>) -> anyhow::Result<PathBuf> {
    let p = Process::open(REKORDBOX_EXE)?;
    let version = p.file_version().context("rekordbox version unknown")?;
    let path = dir.join(format!("{version}.toml"));
    let mut offsets = Offsets::load(&path)
        .with_context(|| format!("no offsets for rekordbox {version} in {}", dir.display()))?;
    let sig = offsets
        .signature
        .clone()
        .context("this offsets file has no signature to refine")?;
    let needle = (p.module_base + sig.anchors[0].module_offset).to_le_bytes();
    let found: Vec<u64> = find_bytes(&p, &needle, SIBLING_HITS)
        .iter()
        .filter_map(|h| h.checked_add_signed(-sig.anchors[0].offset))
        .filter(|pos| super::reader::signature_matches(&p, &sig, *pos))
        .collect();
    let primary = *found
        .first()
        .context("the current signature finds no deck; recalibrate")?;
    s.say(format!(
        "rekordbox {version}: the current signature finds {} deck(s)",
        found.len()
    ));
    let (anchors, decks) = refine(s, &p, primary, &sig.anchors)?;
    let max_decks = deck_limit(decks.len()).max(sig.max_decks);
    offsets.signature = Some(DeckSignature {
        anchors,
        max_decks,
        ..sig
    });
    offsets.save(&path)?;
    s.say(format!("wrote {}", path.display()));
    Ok(path)
}

/// Combines several calibrations of one rekordbox version into the one they agree on: the
/// anchors every run found (in the first run's order, so its rarest anchor stays first), the
/// largest deck limit, and the optional fields from the first run that has them. A pointer
/// one run saw by chance drops out; one every run saw is part of the deck's layout.
pub fn merge_offsets(runs: &[Offsets]) -> anyhow::Result<Offsets> {
    let first = runs.first().context("no calibrations to merge")?;
    for run in runs {
        if run.rekordbox_version != first.rekordbox_version {
            bail!(
                "calibrations of different versions: {} and {}",
                first.rekordbox_version,
                run.rekordbox_version
            );
        }
        if run.position_format != first.position_format {
            bail!("the calibrations disagree on the position format");
        }
    }
    let sigs: Vec<&DeckSignature> = runs
        .iter()
        .map(|r| {
            r.signature
                .as_ref()
                .context("a calibration has no signature")
        })
        .collect::<anyhow::Result<_>>()?;
    let anchors: Vec<Anchor> = sigs[0]
        .anchors
        .iter()
        .filter(|a| sigs.iter().all(|s| s.anchors.contains(a)))
        .cloned()
        .collect();
    if anchors.len() < MIN_ANCHORS {
        bail!(
            "the calibrations share only {} anchor(s); at least {MIN_ANCHORS} are needed",
            anchors.len()
        );
    }
    let mut merged = first.clone();
    merged.signature = Some(DeckSignature {
        anchors,
        max_decks: sigs.iter().map(|s| s.max_decks).max().unwrap_or(4),
        master_flag: sigs.iter().find_map(|s| s.master_flag.clone()),
        tempo: sigs.iter().find_map(|s| s.tempo),
        track_id: sigs.iter().find_map(|s| s.track_id.clone()),
    });
    merged.provenance = Some(format!("merged from {} calibrations", runs.len()));
    Ok(merged)
}

/// Runs the guided session against the running rekordbox and writes
/// `<out_dir>/<version>.toml`, returning its path.
pub fn calibrate(out_dir: &Path, s: &mut Session<'_>) -> anyhow::Result<PathBuf> {
    let p = Process::open(REKORDBOX_EXE)?;
    let version = p.file_version().context("rekordbox version unknown")?;
    s.say(format!(
        "rekordbox {version} (pid {}), module at {:#x}",
        p.pid, p.module_base
    ));
    s.say(
        "This takes about three minutes: with a track paused, play it, pause it, play it again.          The deck plays for under a minute of that.",
    );
    s.say(
        "Step 1: load a track on any deck and leave it paused, at least a minute before any \
         loop or the end of the track (anywhere else is fine; the start is not needed).",
    );
    let counters = absolute_position_counters(s, &p)?;
    let (mut signature, counter) = best_signature(s, &p, &counters)?;
    let (anchors, _) = refine(s, &p, counter.addr, &signature.anchors)?;
    signature.anchors = anchors;

    // The signature must find the deck it came from when scanned for at runtime.
    let needle = (p.module_base + signature.anchors[0].module_offset).to_le_bytes();
    let hits = find_bytes(&p, &needle, RARITY_CAP);
    let found: BTreeSet<u64> = hits
        .iter()
        .filter_map(|h| h.checked_add_signed(-signature.anchors[0].offset))
        .filter(|pos| super::reader::signature_matches(&p, &signature, *pos))
        .collect();
    s.say(format!(
        "  a runtime scan finds {} deck object(s): {}",
        found.len(),
        found
            .iter()
            .map(|a| format!("{a:#x}"))
            .collect::<Vec<_>>()
            .join(" ")
    ));
    if !found.contains(&counter.addr) {
        bail!("the signature does not find the position it was derived from");
    }
    signature.max_decks = deck_limit(found.len());
    signature.tempo = tempo_fields(&p, counter.addr);
    match signature.tempo {
        Some(_) => s.say("  tempo fields found beside the position (tells loaded tracks apart)"),
        None => s.say("  no tempo fields beside the position; tracks are told apart by length"),
    }

    let offsets = Offsets {
        rekordbox_version: version.clone(),
        platform: "windows".into(),
        position_format: match counter.format {
            CounterFormat::I64 => PositionFormat::I64,
            CounterFormat::I32 => PositionFormat::I32,
            CounterFormat::F64 => PositionFormat::F64,
        },
        // Positions count samples of the loaded file (44.1 or 48 kHz files differ).
        position_rate_hz: 0.0,
        master_deck: None,
        decks: Vec::new(),
        signature: Some(signature),
        provenance: Some(format!(
            "onset calibrate at unix {}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs())
        )),
    };
    let path = out_dir.join(format!("{version}.toml"));
    offsets.save(&path)?;
    s.say(format!("wrote {}", path.display()));
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(anchors: &[(i64, u64)], max_decks: usize) -> Offsets {
        Offsets {
            rekordbox_version: "7.2.19".into(),
            platform: "windows".into(),
            position_format: PositionFormat::I32,
            position_rate_hz: 0.0,
            master_deck: None,
            decks: Vec::new(),
            signature: Some(DeckSignature {
                anchors: anchors
                    .iter()
                    .map(|(offset, module_offset)| Anchor {
                        offset: *offset,
                        module_offset: *module_offset,
                    })
                    .collect(),
                max_decks,
                master_flag: None,
                tempo: None,
                track_id: None,
            }),
            provenance: None,
        }
    }

    #[test]
    fn a_layout_holding_the_playhead_twice_in_one_object_is_the_player() {
        // rekordbox 7.2.19's player object keeps the playhead at two places 0x220 apart;
        // a deck object keeps it once.
        let player = [
            counter(0x26f_291b_04e8, CounterFormat::I32, 38.4),
            counter(0x26f_291b_02c8, CounterFormat::I32, 38.4),
        ];
        assert!(holds_twice(&player));
        let decks = [
            counter(0x26f_6374_de14, CounterFormat::I32, 38.4),
            counter(0x26f_6374_f104, CounterFormat::I32, 38.4),
        ];
        assert!(!holds_twice(&decks));
    }

    #[test]
    fn the_deck_layout_outranks_a_generic_structure_found_more_often() {
        // 7.2.19, calibrated from the panel: the deck layout was found 5 times sharing 8
        // pointers with tempo beside the position; a generic structure 15 times sharing 3,
        // with none; the player object 8 times, holding the playhead twice.
        let deck = layout_rank(true, 5, 8, true, 1);
        let generic = layout_rank(true, 15, 3, false, 1);
        let player = layout_rank(false, 8, 4, false, 2);
        assert!(deck.key() > generic.key());
        assert!(deck.key() > player.key());
        assert!(generic.key() > player.key());
        // Without tempo fields, a plausible count with more shared pointers still wins.
        assert!(layout_rank(true, 5, 8, false, 1).key() > generic.key());
    }

    #[test]
    fn merging_keeps_the_anchors_every_run_found() {
        let a = run(&[(-420, 1), (-412, 2), (-364, 3), (-36, 4), (-8, 99)], 4);
        let mut b = run(&[(-412, 2), (-420, 1), (-36, 4), (-364, 3)], 8);
        b.signature.as_mut().unwrap().tempo = Some(TempoFields {
            bpm_offset: -4,
            pitch_offset: 4,
        });
        let merged = merge_offsets(&[a, b]).unwrap();
        let sig = merged.signature.unwrap();
        let kept: Vec<(i64, u64)> = sig
            .anchors
            .iter()
            .map(|a| (a.offset, a.module_offset))
            .collect();
        assert_eq!(kept, vec![(-420, 1), (-412, 2), (-364, 3), (-36, 4)]);
        assert_eq!(sig.max_decks, 8);
        assert!(sig.tempo.is_some());
    }

    #[test]
    fn merging_refuses_runs_that_share_too_little() {
        let a = run(&[(-420, 1), (-412, 2), (-364, 3)], 4);
        let b = run(&[(-420, 1), (-8, 7), (-16, 8)], 4);
        assert!(merge_offsets(&[a, b]).is_err());
        let mut other = run(&[(-420, 1), (-412, 2), (-364, 3)], 4);
        other.rekordbox_version = "7.2.18".into();
        assert!(merge_offsets(&[run(&[(-420, 1), (-412, 2), (-364, 3)], 4), other]).is_err());
    }

    fn counter(addr: u64, format: CounterFormat, seconds: f64) -> Counter {
        Counter {
            addr,
            format,
            first: seconds * 44_100.0 - 4_410.0,
            second: seconds * 44_100.0,
            rate_hz: 44_100.0,
            nominal_hz: 44_100.0,
            playback_rate: 1.0,
        }
    }

    #[test]
    fn clocks_running_while_paused_are_not_position_candidates() {
        // rekordbox 7.2.18 runs hundreds of audio-rate clocks with every deck paused. A scan
        // that started before play saw only those; they never stop, so the pause was never seen.
        let clocks: Vec<Counter> = (0..600)
            .map(|i| counter(0x1000 + i * 8, CounterFormat::I32, 4.6))
            .collect();
        let baseline: BTreeSet<u64> = clocks.iter().map(|c| c.addr).collect();
        let positions = [0x1d7_16f0_cca4, 0x1d7_16fe_ecc0, 0x1d7_1700_c7d8];
        let mut after_play = clocks.clone();
        after_play.extend(
            positions
                .iter()
                .map(|a| counter(*a, CounterFormat::I32, 117.8)),
        );
        let started = started_with_play(&baseline, after_play);
        assert_eq!(
            started.iter().map(|c| c.addr).collect::<Vec<_>>(),
            positions
        );
    }

    #[test]
    fn a_scan_that_only_saw_clocks_has_nothing_new() {
        let clocks: Vec<Counter> = (0..10)
            .map(|i| counter(0x2000 + i * 8, CounterFormat::I32, 9.2))
            .collect();
        let baseline: BTreeSet<u64> = clocks.iter().map(|c| c.addr).collect();
        assert!(started_with_play(&baseline, clocks).len() < MIN_STOPPED);
    }

    #[test]
    fn ranking_puts_the_largest_group_and_narrow_integers_first() {
        let ranked = rank(vec![
            counter(1, CounterFormat::F64, 30.0),
            counter(2, CounterFormat::I64, 117.8),
            counter(3, CounterFormat::I32, 117.8),
            counter(4, CounterFormat::I32, 117.9),
            counter(5, CounterFormat::I32, 30.0),
        ]);
        assert_eq!(ranked[0].format, CounterFormat::I32);
        assert_eq!(ranked.last().map(|c| c.format), Some(CounterFormat::F64));
        let i32s: Vec<u64> = ranked
            .iter()
            .filter(|c| c.format == CounterFormat::I32)
            .map(|c| c.addr)
            .collect();
        assert_eq!(
            i32s,
            vec![3, 4, 5],
            "the 117.8 s group comes before the smaller 30 s group"
        );
    }

    #[test]
    fn the_steps_start_with_the_deck_paused() {
        assert!(STEPS[0].contains("paused"));
        assert_eq!(STEPS.len(), 4);
    }
}
