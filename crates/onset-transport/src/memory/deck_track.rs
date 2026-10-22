//! What the live link knows about the track on one deck beyond its playhead: the content id
//! last read for it, the sample rate its position counts in, and the library's values the
//! engine sent for it. Kept per deck, so one deck's track never lends its sample rate or
//! BPM to another, and dropped when the deck takes a new track.
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Sample rates audio files use; a measured rate snaps to the nearest.
const SAMPLE_RATES: [f64; 8] = [
    22_050.0, 32_000.0, 44_100.0, 48_000.0, 88_200.0, 96_000.0, 176_400.0, 192_000.0,
];
/// How far a measurement may sit from a standard rate and still be it, as a fraction:
/// 44.1 and 48 kHz are 8.8 % apart, and a 2 s window read at 120 Hz is good to about 1 %.
const SNAP_TOLERANCE: f64 = 0.03;

/// The standard sample rate `hz` measures, when it is close to one.
pub fn snap_sample_rate(hz: f64) -> Option<f64> {
    SAMPLE_RATES
        .iter()
        .copied()
        .find(|r| (hz - r).abs() <= r * SNAP_TOLERANCE)
}

/// Measures the rate a deck's position counts at, from how far it moves while playing and
/// the deck's pitch: units per second divided by (1 + pitch) is the file's sample rate.
/// Two windows in a row must agree, so a cue jump, a loop or a nudge never sets it.
#[derive(Debug, Clone, Copy, Default)]
pub struct RateProbe {
    anchor: Option<(f64, Instant)>,
    candidate: Option<f64>,
    measured: Option<f64>,
}

impl RateProbe {
    /// Each measurement spans this long, so position updates in audio-buffer steps average out.
    pub const WINDOW: Duration = Duration::from_secs(2);

    /// Takes one read of the raw position. `pitch` is the deck's pitch fraction.
    pub fn update(&mut self, raw: f64, pitch: Option<f32>, playing: bool, now: Instant) {
        let Some(pitch) = pitch.filter(|_| playing) else {
            self.anchor = None;
            self.candidate = None;
            return;
        };
        let Some((raw0, t0)) = self.anchor else {
            self.anchor = Some((raw, now));
            return;
        };
        let dt = now.duration_since(t0);
        if dt < Self::WINDOW {
            return;
        }
        let units = (raw - raw0) / dt.as_secs_f64() / (1.0 + f64::from(pitch));
        let snapped = snap_sample_rate(units);
        if snapped.is_some() && snapped == self.candidate {
            self.measured = snapped;
        }
        self.candidate = snapped;
        self.anchor = Some((raw, now));
    }

    /// The sample rate measured, once two windows agreed.
    pub fn measured(&self) -> Option<f64> {
        self.measured
    }
}

/// Polls a deck's track BPM must read differently before it counts as a new track: tempo
/// and pitch are two fields, and a read between their writes gives one odd value.
const BPM_CHANGE_POLLS: u32 = 30;

/// How far a deck's track BPM may move and still be the same track, as a fraction.
const BPM_TOLERANCE: f32 = 0.005;

/// Which track a deck holds, as far as can be told. rekordbox's content id is only readable
/// while the deck plays (it builds its player objects then), so the last id read is kept
/// while the deck shows no sign of another track: the same track BPM and the same open file.
#[derive(Debug, Clone, Default)]
pub struct TrackIdentity {
    seen: bool,
    id: Option<u32>,
    bpm: Option<f32>,
    file: Option<PathBuf>,
    bpm_differs: u32,
}

impl TrackIdentity {
    /// The content id the deck's track is known by, if one has been read for it.
    pub fn id(&self) -> Option<u32> {
        self.id
    }

    /// Takes this poll's evidence: the content id read (if any), the deck's track BPM and
    /// the open file paired with the deck. Returns the content id to report and whether the
    /// deck now holds a different track than before.
    pub fn update(
        &mut self,
        read: Option<u32>,
        bpm: Option<f32>,
        file: Option<&Path>,
    ) -> (Option<u32>, bool) {
        if !self.seen {
            *self = Self {
                seen: true,
                id: read,
                bpm,
                file: file.map(Path::to_path_buf),
                bpm_differs: 0,
            };
            return (read, false);
        }
        let new_id = read.is_some_and(|id| self.id.is_some_and(|old| old != id));
        let new_file = matches!((&self.file, file), (Some(old), Some(f)) if old != f);
        let bpm_moved = match (self.bpm, bpm) {
            (Some(old), Some(b)) => (old - b).abs() > old * BPM_TOLERANCE,
            (old, b) => old.is_some() != b.is_some(),
        };
        self.bpm_differs = if bpm_moved { self.bpm_differs + 1 } else { 0 };
        let changed = new_id || new_file || self.bpm_differs >= BPM_CHANGE_POLLS;
        if changed {
            *self = Self {
                seen: true,
                id: read,
                bpm,
                file: file.map(Path::to_path_buf),
                bpm_differs: 0,
            };
            return (read, true);
        }
        if read.is_some() {
            self.id = read;
        }
        if self.file.is_none() {
            self.file = file.map(Path::to_path_buf);
        }
        (self.id, false)
    }
}

/// Everything kept for one deck.
#[derive(Debug, Clone, Default)]
pub struct DeckTrack {
    pub identity: TrackIdentity,
    pub probe: RateProbe,
    /// The library's sample rate for the track, as the engine sent it.
    pub sample_rate: Option<f64>,
    /// The library's BPM for the track, as the engine sent it.
    pub bpm: Option<f32>,
}

impl DeckTrack {
    /// Units per second of the deck's position: the library's rate, else the measured one.
    pub fn rate_hz(&self) -> Option<f64> {
        self.sample_rate.or(self.probe.measured())
    }

    /// Forgets the track after the deck took a new one, keeping the identity just set.
    pub fn new_track(&mut self) {
        self.probe = RateProbe::default();
        self.sample_rate = None;
        self.bpm = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measured_rates_snap_to_standard_ones() {
        assert_eq!(snap_sample_rate(44_300.0), Some(44_100.0));
        assert_eq!(snap_sample_rate(47_600.0), Some(48_000.0));
        assert_eq!(snap_sample_rate(46_000.0), None, "between two rates");
        assert_eq!(snap_sample_rate(0.0), None);
    }

    fn after(t0: Instant, s: f64) -> Instant {
        t0 + Duration::from_secs_f64(s)
    }

    #[test]
    fn rate_probe_needs_two_agreeing_windows_while_playing() {
        let t0 = Instant::now();
        let mut probe = RateProbe::default();
        // 48 kHz at +4 % pitch: 49 920 units per second.
        let pos = |s: f64| s * 49_920.0;
        probe.update(pos(0.0), Some(0.04), true, t0);
        probe.update(pos(2.5), Some(0.04), true, after(t0, 2.5));
        assert_eq!(probe.measured(), None, "one window is a candidate");
        probe.update(pos(5.0), Some(0.04), true, after(t0, 5.0));
        assert_eq!(probe.measured(), Some(48_000.0));

        // A cue jump in one window does not unset it, and no pitch resets the window.
        probe.update(pos(60.0), Some(0.04), true, after(t0, 7.5));
        assert_eq!(probe.measured(), Some(48_000.0));
        probe.update(pos(60.0), None, false, after(t0, 8.0));
        assert_eq!(probe.measured(), Some(48_000.0));
    }

    #[test]
    fn rate_probe_ignores_a_paused_deck() {
        let t0 = Instant::now();
        let mut probe = RateProbe::default();
        probe.update(0.0, Some(0.0), false, t0);
        probe.update(0.0, Some(0.0), false, after(t0, 5.0));
        assert_eq!(probe.measured(), None);
    }

    #[test]
    fn identity_keeps_the_id_while_the_deck_pauses() {
        let mut id = TrackIdentity::default();
        assert_eq!(id.update(None, Some(126.0), None), (None, false));
        assert_eq!(id.update(Some(7), Some(126.0), None), (Some(7), false));
        // Paused: no player, so no id to read, but nothing says another track.
        assert_eq!(id.update(None, Some(126.0), None), (Some(7), false));
        assert_eq!(id.id(), Some(7));
    }

    #[test]
    fn identity_changes_with_a_new_id_file_or_settled_bpm() {
        let mut id = TrackIdentity::default();
        id.update(Some(7), Some(126.0), Some(Path::new("a.mp3")));
        assert_eq!(id.update(Some(8), Some(126.0), None), (Some(8), true));

        let mut id = TrackIdentity::default();
        id.update(Some(7), Some(126.0), Some(Path::new("a.mp3")));
        assert_eq!(
            id.update(None, Some(126.0), Some(Path::new("b.mp3"))),
            (None, true)
        );

        let mut id = TrackIdentity::default();
        id.update(Some(7), Some(126.0), None);
        for _ in 0..BPM_CHANGE_POLLS - 1 {
            assert_eq!(id.update(None, Some(140.0), None), (Some(7), false));
        }
        // One odd read in between starts the count over.
        assert_eq!(id.update(None, Some(126.0), None), (Some(7), false));
        for _ in 0..BPM_CHANGE_POLLS - 1 {
            assert_eq!(id.update(None, Some(140.0), None), (Some(7), false));
        }
        assert_eq!(id.update(None, Some(140.0), None), (None, true));
    }

    #[test]
    fn deck_track_prefers_the_library_rate_and_forgets_it_with_the_track() {
        let mut track = DeckTrack::default();
        assert_eq!(track.rate_hz(), None);
        track.sample_rate = Some(48_000.0);
        track.bpm = Some(126.0);
        assert_eq!(track.rate_hz(), Some(48_000.0));
        track.new_track();
        assert_eq!((track.rate_hz(), track.bpm), (None, None));
    }
}
