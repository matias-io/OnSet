//! Beat-locked drum hits. The audio path hears a kick 60-100 ms after it happened (the
//! loopback buffer, the analysis window, the envelope follower, the render queue), so a
//! visual that follows the audio alone is always a little late. The beat grid and
//! rekordbox's own band analysis at the playhead carry no such delay, so while a track with
//! a grid is playing the kick and snare are *predicted* from the grid, gated by what
//! rekordbox says is there, and the audio detector only confirms them: a beat whose kick the
//! audio keeps failing to hear (the DJ cut the lows, a breakdown without drums) fades out.
//! Audio hits away from the grid (fills, breaks) still pass through.
//!
//! The same comparison measures the audio path's lag against the grid, for the HUD scope.
use crate::audio_features::AudioFeatures;
use crate::bands::BandsAt;
use crate::structure::StructureState;

/// Visual decay of a grid kick, in beats: 0.3 of a beat is 140 ms at 128 BPM, like the
/// audio detector's, and stays a fixed share of the beat at any tempo.
pub const KICK_DECAY_BEATS: f32 = 0.3;
/// Visual decay of a grid snare, in beats.
pub const SNARE_DECAY_BEATS: f32 = 0.25;
/// An audio hit within this many beats of a grid beat (after the measured lag) confirms it;
/// further away it is an off-grid hit of its own.
pub const AGREE_BEATS: f32 = 0.2;
/// How fast confidence follows the audio: 0.4 reaches 90 % agreement in about four beats.
const CONFIDENCE_RATE: f32 = 0.4;
/// Confidence when a track starts, before the audio has agreed or disagreed.
const CONFIDENCE_START: f32 = 0.5;
/// rekordbox's low band at the playhead, scaled to 0..1 against the track's peak, below
/// which no kick is expected and above which it is expected in full.
const LOW_GATE: (f32, f32) = (0.15, 0.5);
/// The same for the mid band and the snare; the mid band is rarely empty, so the audio
/// does most of the gating there.
const MID_GATE: (f32, f32) = (0.1, 0.4);
/// Weight of an audio snare off beats 2 and 4: vocals and stabs trigger the snare band too,
/// so an unconfirmed snare shows at half strength.
const OFF_GRID_SNARE: f32 = 0.5;
/// Smoothing of the lag estimate, per audio kick.
const LAG_RATE: f32 = 0.1;

/// The hits the visuals react to this frame, and the evidence behind them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BeatHits {
    /// The kick to show, 1 at the hit decaying to 0.
    pub kick: f32,
    /// The snare to show.
    pub snare: f32,
    /// What the audio detector alone says.
    pub audio_kick: f32,
    pub audio_snare: f32,
    /// How often the audio has confirmed the grid's kicks and snares lately, 0..1.
    pub kick_confidence: f32,
    pub snare_confidence: f32,
    /// Seconds the audio's kicks arrive after the grid's beats (negative = early); only
    /// meaningful once `lag_samples` is a few dozen.
    pub lag_s: f32,
    pub lag_samples: u32,
    /// True while the hits come from the grid; false when they are the audio's as heard.
    pub locked: bool,
}

impl Default for BeatHits {
    fn default() -> Self {
        Self {
            kick: 0.0,
            snare: 0.0,
            audio_kick: 0.0,
            audio_snare: 0.0,
            kick_confidence: CONFIDENCE_START,
            snare_confidence: CONFIDENCE_START,
            lag_s: 0.0,
            lag_samples: 0,
            locked: false,
        }
    }
}

fn smoothstep(edge: (f32, f32), x: f32) -> f32 {
    let t = ((x - edge.0) / (edge.1 - edge.0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Fuses the grid, rekordbox's bands and the audio detector into [`BeatHits`], one call per
/// engine tick.
#[derive(Debug, Clone)]
pub struct HitLock {
    last_beat: Option<u32>,
    /// Strength latched at the start of the current beat; the envelope decays from it.
    kick_strength: f32,
    snare_strength: f32,
    kick_confidence: f32,
    snare_confidence: f32,
    /// An audio hit agreed with the current beat.
    kick_seen: bool,
    snare_seen: bool,
    /// Audio hits away from the grid, decaying.
    off_kick: f32,
    off_snare: f32,
    prev_audio_kick: f32,
    prev_audio_snare: f32,
    lag_beats: f32,
    lag_s: f32,
    lag_samples: u32,
}

impl Default for HitLock {
    fn default() -> Self {
        Self::new()
    }
}

impl HitLock {
    pub fn new() -> Self {
        Self {
            last_beat: None,
            kick_strength: 0.0,
            snare_strength: 0.0,
            kick_confidence: CONFIDENCE_START,
            snare_confidence: CONFIDENCE_START,
            kick_seen: false,
            snare_seen: false,
            off_kick: 0.0,
            off_snare: 0.0,
            prev_audio_kick: 0.0,
            prev_audio_snare: 0.0,
            lag_beats: 0.0,
            lag_s: 0.0,
            lag_samples: 0,
        }
    }

    /// A new track: the grid's evidence starts over; the audio path's lag is the same.
    fn restart(&mut self) {
        self.kick_confidence = CONFIDENCE_START;
        self.snare_confidence = CONFIDENCE_START;
        self.kick_seen = false;
        self.snare_seen = false;
        self.kick_strength = 0.0;
        self.snare_strength = 0.0;
    }

    /// `bpm` is the tempo being played; `bands` is rekordbox's analysis at the playhead
    /// when the track has one; `dt` the seconds since the last call.
    pub fn update(
        &mut self,
        st: &StructureState,
        playing: bool,
        bpm: f32,
        bands: Option<BandsAt>,
        audio: &AudioFeatures,
        dt: f32,
    ) -> BeatHits {
        let new_kick = audio.kick > self.prev_audio_kick + 1e-3;
        let new_snare = audio.snare > self.prev_audio_snare + 1e-3;
        self.prev_audio_kick = audio.kick;
        self.prev_audio_snare = audio.snare;

        let locked = playing && bpm > 1.0 && st.beat_index.is_some() && !audio.silent;
        let Some(beat) = st.beat_index.filter(|_| locked) else {
            self.last_beat = None;
            self.kick_strength = 0.0;
            self.snare_strength = 0.0;
            return BeatHits {
                kick: audio.kick,
                snare: audio.snare,
                audio_kick: audio.kick,
                audio_snare: audio.snare,
                kick_confidence: self.kick_confidence,
                snare_confidence: self.snare_confidence,
                lag_s: self.lag_s,
                lag_samples: self.lag_samples,
                locked: false,
            };
        };
        let beat_s = 60.0 / bpm;
        // Beats 2 and 4 of the bar carry the snare in four-to-the-floor music.
        let backbeat = matches!((st.bar_phase * 4.0).floor() as u32, 1 | 3);

        // Audio hits: confirm the beat they fall on, or stand on their own.
        let phase = st.beat_phase;
        if new_kick {
            // Signed distance to the nearest beat, in beats: late is positive.
            let off = if phase < 0.5 { phase } else { phase - 1.0 };
            let k = if self.lag_samples == 0 { 1.0 } else { LAG_RATE };
            self.lag_beats += k * (off - self.lag_beats);
            self.lag_s = self.lag_beats * beat_s;
            self.lag_samples = self.lag_samples.saturating_add(1);
            if (off - self.lag_beats).abs() <= AGREE_BEATS {
                self.kick_seen = true;
            } else {
                self.off_kick = self.off_kick.max(audio.kick);
            }
        }
        if new_snare {
            let off = if phase < 0.5 { phase } else { phase - 1.0 };
            if backbeat && (off - self.lag_beats).abs() <= AGREE_BEATS {
                self.snare_seen = true;
            } else {
                self.off_snare = self.off_snare.max(audio.snare * OFF_GRID_SNARE);
            }
        }

        // A beat crossing: score the beat that ended, latch the strength of the new one.
        if self.last_beat != Some(beat) {
            match self.last_beat {
                Some(prev) if beat > prev && beat - prev <= 8 => {
                    self.kick_confidence +=
                        CONFIDENCE_RATE * (f32::from(u8::from(self.kick_seen)) - self.kick_confidence);
                    if matches!(((st.bar_phase * 4.0).floor() as u32 + 3) % 4, 1 | 3) {
                        // The beat that ended was a backbeat.
                        self.snare_confidence += CONFIDENCE_RATE
                            * (f32::from(u8::from(self.snare_seen)) - self.snare_confidence);
                    }
                }
                _ => self.restart(),
            }
            self.kick_seen = false;
            self.snare_seen = false;
            let low_gate = bands.map_or(1.0, |b| smoothstep(LOW_GATE, b.low));
            let mid_gate = bands.map_or(1.0, |b| smoothstep(MID_GATE, b.mid));
            self.kick_strength = low_gate * self.kick_confidence;
            self.snare_strength = if backbeat {
                mid_gate * self.snare_confidence
            } else {
                0.0
            };
            self.last_beat = Some(beat);
        }

        // Envelopes from the phase, so a late tick still lands on the right shape.
        let kick = self.kick_strength * (-phase / KICK_DECAY_BEATS).exp();
        let snare = self.snare_strength * (-phase / SNARE_DECAY_BEATS).exp();
        self.off_kick *= (-dt / (KICK_DECAY_BEATS * beat_s)).exp();
        self.off_snare *= (-dt / (SNARE_DECAY_BEATS * beat_s)).exp();

        BeatHits {
            kick: kick.max(self.off_kick).clamp(0.0, 1.0),
            snare: snare.max(self.off_snare).clamp(0.0, 1.0),
            audio_kick: audio.kick,
            audio_snare: audio.snare,
            kick_confidence: self.kick_confidence,
            snare_confidence: self.snare_confidence,
            lag_s: self.lag_s,
            lag_samples: self.lag_samples,
            locked: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BPM: f32 = 120.0;
    const TICK: f32 = 1.0 / 120.0;

    fn state(beat: u32, phase: f32) -> StructureState {
        StructureState {
            beat_index: Some(beat),
            beat_phase: phase,
            bar_phase: (beat % 4) as f32 / 4.0 + phase / 4.0,
            bpm: BPM,
            ..Default::default()
        }
    }

    fn heard(kick: f32, snare: f32) -> AudioFeatures {
        AudioFeatures {
            kick,
            snare,
            silent: false,
            ..AudioFeatures::silent()
        }
    }

    /// Plays `beats` beats, with the audio reporting a kick `lag_s` after each grid beat
    /// (and a snare after each backbeat when `snares`), ticking at 120 Hz. Returns the
    /// hits at every tick.
    fn play(
        lock: &mut HitLock,
        length_beats: u32,
        lag_s: f32,
        low: f32,
        snares: bool,
    ) -> Vec<(u32, f32, BeatHits)> {
        let beat_s = 60.0 / BPM;
        let mut out = Vec::new();
        let mut audio_kick = 0.0f32;
        let mut audio_snare = 0.0f32;
        let mut t = 0.0f32;
        let total = length_beats as f32 * beat_s;
        while t < total {
            let beat = (t / beat_s).floor() as u32;
            let phase = t / beat_s - beat as f32;
            audio_kick *= (-TICK / 0.14).exp();
            audio_snare *= (-TICK / 0.11).exp();
            let since = phase * beat_s;
            if since >= lag_s && since < lag_s + TICK {
                audio_kick = 1.0;
                if snares && beat % 2 == 1 {
                    audio_snare = 1.0;
                }
            }
            let bands = Some(BandsAt {
                low,
                mid: 0.6,
                high: 0.3,
                vocal: 0.0,
            });
            let hits = lock.update(
                &state(beat, phase),
                true,
                BPM,
                bands,
                &heard(audio_kick, audio_snare),
                TICK,
            );
            out.push((beat, phase, hits));
            t += TICK;
        }
        out
    }

    #[test]
    fn grid_kicks_land_on_the_beat_not_on_the_late_audio() {
        let mut lock = HitLock::new();
        let run = play(&mut lock, 16, 0.08, 0.8, true);
        // After settling, the kick peaks at the top of the beat and has decayed by the time
        // the audio hears it.
        let late: Vec<_> = run.iter().filter(|(b, _, _)| *b >= 8).collect();
        for (_, phase, h) in &late {
            if *phase < 0.02 {
                assert!(h.kick > 0.85, "kick at beat start {h:?}");
            }
        }
        let at_audio = late
            .iter()
            .find(|(_, _, h)| h.audio_kick > 0.99)
            .unwrap()
            .2;
        assert!(at_audio.kick < at_audio.audio_kick, "{at_audio:?}");
        assert!(at_audio.locked);
    }

    #[test]
    fn measures_the_audio_lag() {
        let mut lock = HitLock::new();
        let run = play(&mut lock, 32, 0.06, 0.8, false);
        let last = run.last().unwrap().2;
        assert!(last.lag_samples >= 30, "{last:?}");
        assert!((last.lag_s - 0.06).abs() < 0.015, "lag {}", last.lag_s);
    }

    #[test]
    fn confidence_rises_with_agreement_and_falls_without_it() {
        let mut lock = HitLock::new();
        let agreed = play(&mut lock, 12, 0.05, 0.8, true).last().unwrap().2;
        assert!(agreed.kick_confidence > 0.9, "{agreed:?}");
        assert!(agreed.snare_confidence > 0.8, "{agreed:?}");

        // The DJ cuts the lows: the audio stops hearing kicks and the grid kick fades.
        let beat_s = 60.0 / BPM;
        let mut last = agreed;
        for i in 0..(12.0 * beat_s / TICK) as u32 {
            let t = i as f32 * TICK;
            let beat = 12 + (t / beat_s).floor() as u32;
            let phase = t / beat_s - (t / beat_s).floor();
            last = lock.update(
                &state(beat, phase),
                true,
                BPM,
                Some(BandsAt {
                    low: 0.8,
                    mid: 0.6,
                    high: 0.3,
                    vocal: 0.0,
                }),
                &heard(0.0, 0.0),
                TICK,
            );
        }
        assert!(last.kick_confidence < 0.1, "{last:?}");
        assert!(last.kick < 0.1, "{last:?}");
    }

    #[test]
    fn rekordbox_quiet_lows_gate_the_kick() {
        let mut lock = HitLock::new();
        let run = play(&mut lock, 8, 0.05, 0.05, false);
        let peak = run
            .iter()
            .filter(|(b, _, _)| *b >= 2)
            .map(|(_, _, h)| h.kick)
            .fold(0.0, f32::max);
        assert!(peak < 0.05, "no kick expected in a breakdown: {peak}");
    }

    #[test]
    fn off_grid_audio_hits_pass_through() {
        let mut lock = HitLock::new();
        play(&mut lock, 8, 0.0, 0.8, false);
        // A kick halfway through the beat (a fill).
        let h = lock.update(
            &state(8, 0.5),
            true,
            BPM,
            None,
            &heard(1.0, 0.0),
            TICK,
        );
        assert!(h.kick > 0.9, "{h:?}");
    }

    #[test]
    fn without_a_grid_or_while_paused_the_audio_is_passed_through() {
        let mut lock = HitLock::new();
        let no_grid = StructureState::default();
        let h = lock.update(&no_grid, true, 0.0, None, &heard(0.7, 0.3), TICK);
        assert!(!h.locked);
        assert!((h.kick - 0.7).abs() < f32::EPSILON);
        assert!((h.snare - 0.3).abs() < f32::EPSILON);
        let h = lock.update(&state(4, 0.1), false, BPM, None, &heard(0.7, 0.3), TICK);
        assert!(!h.locked);
        assert!((h.kick - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn silence_shows_nothing() {
        let mut lock = HitLock::new();
        play(&mut lock, 8, 0.05, 0.8, false);
        let h = lock.update(
            &state(8, 0.0),
            true,
            BPM,
            None,
            &AudioFeatures::silent(),
            TICK,
        );
        assert!(!h.locked);
        assert!(h.kick.abs() < f32::EPSILON);
    }

    #[test]
    fn a_new_track_starts_the_evidence_over_but_keeps_the_lag() {
        let mut lock = HitLock::new();
        let before = play(&mut lock, 16, 0.07, 0.8, false).last().unwrap().2;
        let h = lock.update(&state(0, 0.0), true, BPM, None, &heard(0.0, 0.0), TICK);
        assert!((h.kick_confidence - CONFIDENCE_START).abs() < f32::EPSILON);
        assert!((h.lag_s - before.lag_s).abs() < f32::EPSILON);
    }
}
