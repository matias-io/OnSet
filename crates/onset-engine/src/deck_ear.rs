//! Follows the deck the room hears. The rekordbox link cannot see the mixer, so during a
//! blend both decks "play" and only the MASTER flag says which one the show should follow;
//! when that flag is unreadable or wrong, the show sticks to the outgoing track.
//!
//! This listens instead. Every playing deck has a known track, a playhead and rekordbox's
//! 3-band waveform for that track (`TrackBands`), so what the room should sound like if
//! only that deck were up is known at every moment. The captured output is fitted as a
//! non-negative mix of the playing decks' expected envelopes, band by band, over the last
//! few seconds: the fit's weights are the mix, so a blend reads as 0.7/0.3 rather than as
//! a coin toss. The show follows the deck with the larger share, with MASTER as a prior and
//! enough hysteresis that a beat-matched blend of two similar tracks keeps the current deck
//! until the other one clearly dominates.
//!
//! Beat-level envelopes of two beat-matched tracks correlate strongly, so the discrimination
//! comes from section energy (a drop on one deck, a breakdown on the other) and from the
//! band whose EQ the DJ moved: a bass swap shows in the low band within a few seconds.
use std::collections::VecDeque;
use std::sync::Arc;

use onset_core::bands::TrackBands;

/// Seconds of listening the fit covers: long enough to span a bass swap, short enough to
/// follow one.
const WINDOW_S: f64 = 6.0;
/// Least listening before a verdict.
const MIN_S: f64 = 2.5;
/// Samples kept per second.
const RATE_HZ: f64 = 25.0;
/// A deck must hold at least this share of what is heard, for [`HOLD_S`], before the show
/// switches to it.
const SWITCH_SHARE: f32 = 0.6;
const HOLD_S: f64 = 4.0;
/// Added to the MASTER deck's share: a prior, not a decision.
const MASTER_PRIOR: f32 = 0.1;
/// The fit must explain this much of the heard variance (r²) to count.
const MIN_FIT: f32 = 0.3;
/// Coordinate-descent passes for the non-negative fit; three unknowns converge in far fewer.
const NNLS_PASSES: usize = 40;

/// A deck the ear can listen for: its index and its track's waveform.
pub struct EarDeck {
    pub deck: u8,
    pub bands: Arc<TrackBands>,
}

/// One moment of listening: the live low, mid and high levels and where each playing deck
/// was.
#[derive(Debug, Clone)]
struct Heard {
    now_s: f64,
    bands: [f32; 3],
    /// `(deck, playhead_s)` for every deck playing at that moment.
    positions: Vec<(u8, f64)>,
}

/// What the ear concluded from the last window.
#[derive(Debug, Clone, PartialEq)]
pub struct Verdict {
    /// Each playing deck's share of what is heard, summing to 1.
    pub shares: Vec<(u8, f32)>,
    /// How much of the heard variance the mix explains, 0..1.
    pub fit: f32,
}

pub struct DeckEar {
    decks: Vec<EarDeck>,
    heard: VecDeque<Heard>,
    last_sample_s: f64,
    /// Seconds the audio reaches Onset after the playhead passes (measured by the hit lock,
    /// or a default).
    lag_s: f64,
    current: Option<u8>,
    /// The deck that has dominated, and since when.
    challenger: Option<(u8, f64)>,
}

impl DeckEar {
    pub fn new(decks: Vec<EarDeck>, lag_s: f64) -> Self {
        Self {
            decks,
            heard: VecDeque::new(),
            last_sample_s: f64::NEG_INFINITY,
            lag_s,
            current: None,
            challenger: None,
        }
    }

    /// The decks it can listen for.
    pub fn decks(&self) -> &[EarDeck] {
        &self.decks
    }

    /// Updates the lag the expected envelopes are read at (seconds, positive = audio late).
    pub fn set_lag(&mut self, lag_s: f64) {
        self.lag_s = lag_s.clamp(0.0, 0.5);
    }

    /// Tells the ear which deck the show follows now (after a choice made elsewhere).
    pub fn set_current(&mut self, deck: Option<u8>) {
        if self.current != deck {
            self.challenger = None;
        }
        self.current = deck;
    }

    pub fn current(&self) -> Option<u8> {
        self.current
    }

    /// Forgets the listening (a deck's track changed, or the playhead jumped).
    pub fn reset(&mut self) {
        self.heard.clear();
        self.challenger = None;
    }

    /// One moment of listening at `now_s`: the live bands and each playing deck's playhead.
    /// Samples closer together than [`RATE_HZ`] allows are dropped.
    pub fn listen(&mut self, now_s: f64, bands: [f32; 3], positions: &[(u8, f64)]) {
        if now_s - self.last_sample_s < 1.0 / RATE_HZ {
            return;
        }
        self.last_sample_s = now_s;
        self.heard.push_back(Heard {
            now_s,
            bands,
            positions: positions.to_vec(),
        });
        while self
            .heard
            .front()
            .is_some_and(|h| now_s - h.now_s > WINDOW_S)
        {
            self.heard.pop_front();
        }
    }

    fn expected(&self, deck: u8, playhead_s: f64) -> Option<[f32; 3]> {
        let d = self.decks.iter().find(|d| d.deck == deck)?;
        let at = d.bands.at(playhead_s + self.lag_s);
        Some([at.low, at.mid, at.high])
    }

    /// The mix the window explains, or `None` with too little listening, fewer than two
    /// decks heard throughout, or a fit too poor to trust.
    pub fn verdict(&self) -> Option<Verdict> {
        let (Some(first), Some(last)) = (self.heard.front(), self.heard.back()) else {
            return None;
        };
        if last.now_s - first.now_s < MIN_S {
            return None;
        }
        // The decks playing through the whole window, in a stable order.
        let mut decks: Vec<u8> = first.positions.iter().map(|p| p.0).collect();
        decks.retain(|d| {
            self.heard
                .iter()
                .all(|h| h.positions.iter().any(|p| p.0 == *d))
                && self.decks.iter().any(|e| e.deck == *d)
        });
        if decks.is_empty() {
            return None;
        }
        let n = self.heard.len();
        let mut contribution = vec![0.0f32; decks.len()];
        let mut explained = 0.0f32;
        let mut total = 0.0f32;
        for band in 0..3 {
            let x: Vec<f32> = self.heard.iter().map(|h| h.bands[band]).collect();
            let columns: Vec<Vec<f32>> = decks
                .iter()
                .map(|d| {
                    self.heard
                        .iter()
                        .map(|h| {
                            let pos = h.positions.iter().find(|p| p.0 == *d).map_or(0.0, |p| p.1);
                            self.expected(*d, pos).map_or(0.0, |e| e[band])
                        })
                        .collect()
                })
                .collect();
            let xc = centred(&x);
            let cols: Vec<Vec<f32>> = columns.iter().map(|c| centred(c)).collect();
            let weights = nnls(&xc, &cols);
            let var_x: f32 = xc.iter().map(|v| v * v).sum::<f32>() / n as f32;
            let mut resid = xc.clone();
            for (w, c) in weights.iter().zip(&cols) {
                for (r, v) in resid.iter_mut().zip(c) {
                    *r -= w * v;
                }
            }
            let var_r: f32 = resid.iter().map(|v| v * v).sum::<f32>() / n as f32;
            total += var_x;
            explained += (var_x - var_r).max(0.0);
            for (k, (w, c)) in weights.iter().zip(&cols).enumerate() {
                let norm = (c.iter().map(|v| v * v).sum::<f32>() / n as f32).sqrt();
                contribution[k] += w * norm;
            }
        }
        if total < 1e-6 {
            return None;
        }
        let fit = (explained / total).clamp(0.0, 1.0);
        if fit < MIN_FIT {
            return None;
        }
        let sum: f32 = contribution.iter().sum();
        if sum < 1e-6 {
            return None;
        }
        Some(Verdict {
            shares: decks
                .iter()
                .zip(&contribution)
                .map(|(d, c)| (*d, c / sum))
                .collect(),
            fit,
        })
    }

    /// The deck the show should follow at `now_s`: the current one until another deck has
    /// held more than [`SWITCH_SHARE`] of what is heard (MASTER counting a little extra)
    /// for [`HOLD_S`]. `None` when the ear has nothing to say, so the caller's own rules
    /// (the MASTER flag, the deck already followed) decide.
    pub fn choose(&mut self, now_s: f64, master: Option<u8>) -> Option<u8> {
        let verdict = self.verdict()?;
        let share = |d: u8| {
            verdict
                .shares
                .iter()
                .find(|s| s.0 == d)
                .map_or(0.0, |s| s.1)
                + if master == Some(d) { MASTER_PRIOR } else { 0.0 }
        };
        let Some(current) = self
            .current
            .filter(|c| verdict.shares.iter().any(|s| s.0 == *c))
        else {
            // Nothing followed yet, or the followed deck stopped: take the biggest share.
            let pick = verdict
                .shares
                .iter()
                .map(|s| s.0)
                .max_by(|a, b| share(*a).total_cmp(&share(*b)))?;
            self.current = Some(pick);
            self.challenger = None;
            return Some(pick);
        };
        let leader = verdict
            .shares
            .iter()
            .map(|s| s.0)
            .filter(|d| *d != current)
            .max_by(|a, b| share(*a).total_cmp(&share(*b)));
        match leader {
            Some(d) if share(d) >= SWITCH_SHARE => {
                let since = match self.challenger {
                    Some((c, t)) if c == d => t,
                    _ => now_s,
                };
                self.challenger = Some((d, since));
                if now_s - since >= HOLD_S {
                    self.current = Some(d);
                    self.challenger = None;
                    return Some(d);
                }
            }
            _ => self.challenger = None,
        }
        Some(current)
    }
}

fn centred(x: &[f32]) -> Vec<f32> {
    let mean = x.iter().sum::<f32>() / x.len().max(1) as f32;
    x.iter().map(|v| v - mean).collect()
}

/// Non-negative least squares of `x ≈ Σ w_j · cols[j]` by coordinate descent: each weight
/// in turn takes its best value against the current residual, clamped at 0. A small ridge
/// term makes the answer unique when two columns coincide (the beat pulse of two
/// beat-matched tracks), splitting their weight evenly instead of handing it all to the
/// first deck.
fn nnls(x: &[f32], cols: &[Vec<f32>]) -> Vec<f32> {
    let dots: Vec<f32> = cols
        .iter()
        .map(|c| c.iter().map(|v| v * v).sum::<f32>())
        .collect();
    let ridge = 0.01 * dots.iter().sum::<f32>() / dots.len().max(1) as f32 + 1e-9;
    // Start from each column's own fit shared out evenly: coordinate descent then keeps
    // identical columns level (from zero it would hand the first one nearly everything
    // and take hundreds of passes to even out), and distinct columns converge as usual.
    let mut w: Vec<f32> = cols
        .iter()
        .zip(&dots)
        .map(|(c, d)| {
            if *d < 1e-9 {
                0.0
            } else {
                (c.iter().zip(x).map(|(a, b)| a * b).sum::<f32>() / d / cols.len() as f32).max(0.0)
            }
        })
        .collect();
    for _ in 0..NNLS_PASSES {
        for j in 0..cols.len() {
            if dots[j] < 1e-9 {
                continue;
            }
            let mut num = 0.0f32;
            for (i, xi) in x.iter().enumerate() {
                let mut others = 0.0f32;
                for (k, c) in cols.iter().enumerate() {
                    if k != j {
                        others += w[k] * c[i];
                    }
                }
                num += cols[j][i] * (xi - others);
            }
            w[j] = (num / (dots[j] + ridge)).max(0.0);
        }
    }
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A track whose sections alternate every `period` seconds between loud and quiet in
    /// the low band, with a mid band that pulses on every beat (shared by every track, as
    /// beat-matched tracks are) and a flat high band.
    fn track(period: f64, phase: f64, low_loud: u8) -> Arc<TrackBands> {
        let rate = 150.0;
        let frames: Vec<[u8; 3]> = (0..(300.0 * rate) as usize)
            .map(|i| {
                let t = i as f64 / rate;
                let loud = ((t + phase) / period).floor() as i64 % 2 == 0;
                let beat = if (t * 2.0).fract() < 0.25 { 200 } else { 60 };
                [if loud { low_loud } else { 20 }, beat, 90]
            })
            .collect();
        Arc::new(TrackBands::new(rate as f32, frames, 1.0, vec![0]))
    }

    fn two_decks() -> DeckEar {
        DeckEar::new(
            vec![
                EarDeck {
                    deck: 0,
                    bands: track(3.0, 0.0, 220),
                },
                // Different section lengths, as two real tracks have; a shared beat pulse.
                EarDeck {
                    deck: 1,
                    bands: track(5.0, 1.0, 220),
                },
            ],
            0.0,
        )
    }

    /// Plays both decks from `start` for `secs`, with deck 0 at `gain0(t)` and deck 1 at
    /// `gain1(t)` in the room (0..1), and returns the ear's choice at the end.
    fn run(
        ear: &mut DeckEar,
        start: f64,
        secs: f64,
        gain0: impl Fn(f64) -> f32,
        gain1: impl Fn(f64) -> f32,
        master: Option<u8>,
    ) -> Option<u8> {
        let mut choice = None;
        let step = 1.0 / 50.0;
        let mut t = 0.0;
        while t < secs {
            let now = start + t;
            let heard = |d: u8, pos: f64, g: f32| {
                let e = ear.expected(d, pos).unwrap();
                [e[0] * g, e[1] * g, e[2] * g]
            };
            let a = heard(0, 30.0 + now, gain0(t));
            let b = heard(1, 90.0 + now, gain1(t));
            let mix = [a[0] + b[0], a[1] + b[1], a[2] + b[2] + 0.05];
            ear.listen(now, mix, &[(0, 30.0 + now), (1, 90.0 + now)]);
            choice = ear.choose(now, master);
            t += step;
        }
        choice
    }

    #[test]
    fn hears_which_deck_is_up() {
        let mut ear = two_decks();
        let pick = run(&mut ear, 0.0, 8.0, |_| 1.0, |_| 0.1, None);
        let v = ear.verdict().unwrap();
        assert_eq!(pick, Some(0), "{v:?}");
        let share0 = v.shares.iter().find(|s| s.0 == 0).unwrap().1;
        // The shared beat pulse in the mids is split evenly, so the share sits below the
        // 1.0 : 0.1 gain ratio; the low band carries the difference.
        assert!(share0 > 0.7, "{v:?}");
        assert!(v.fit > 0.8, "{v:?}");
    }

    #[test]
    fn follows_a_bass_swap_after_it_settles_not_before() {
        let mut ear = two_decks();
        run(&mut ear, 0.0, 8.0, |_| 1.0, |_| 0.0, None);
        assert_eq!(ear.current(), Some(0));
        // A 31 s swap: deck 0 fades out as deck 1 fades in.
        let swap = 31.0;
        let out = move |t: f64| (1.0 - t / swap).clamp(0.0, 1.0) as f32;
        let inn = move |t: f64| (t / swap).clamp(0.0, 1.0) as f32;
        // Halfway, deck 1 is not yet clearly louder: still deck 0.
        let mid = run(&mut ear, 8.0, swap * 0.5, out, inn, None);
        assert_eq!(mid, Some(0), "no switch while the blend is even");
        // By the end deck 1 has held the room: switched.
        let end = run(
            &mut ear,
            8.0 + swap * 0.5,
            swap * 0.5 + 6.0,
            |t| out(t + swap * 0.5),
            |t| inn(t + swap * 0.5),
            None,
        );
        assert_eq!(end, Some(1));
    }

    #[test]
    fn an_even_blend_keeps_the_current_deck() {
        let mut ear = two_decks();
        run(&mut ear, 0.0, 8.0, |_| 1.0, |_| 0.0, None);
        let pick = run(&mut ear, 8.0, 20.0, |_| 0.5, |_| 0.5, None);
        assert_eq!(pick, Some(0));
    }

    #[test]
    fn master_breaks_a_near_tie_but_does_not_override_the_room() {
        let mut ear = two_decks();
        // Nothing followed yet: a near-even room goes to MASTER.
        let pick = run(&mut ear, 0.0, 8.0, |_| 0.5, |_| 0.55, Some(0));
        assert_eq!(pick, Some(0));
        // The room clearly on deck 1: MASTER on deck 0 does not hold it.
        let pick = run(&mut ear, 8.0, 12.0, |_| 0.1, |_| 1.0, Some(0));
        assert_eq!(pick, Some(1));
    }

    #[test]
    fn says_nothing_with_too_little_listening_or_a_poor_fit() {
        let mut ear = two_decks();
        assert_eq!(run(&mut ear, 0.0, 1.0, |_| 1.0, |_| 0.0, None), None);
        // Noise unrelated to either deck.
        let mut ear = two_decks();
        let mut t = 0.0;
        let mut x = 0.3f32;
        while t < 8.0 {
            x = (x * 7.3 + 0.17).fract();
            ear.listen(t, [x, 1.0 - x, x * x], &[(0, 30.0 + t), (1, 90.0 + t)]);
            t += 0.04;
        }
        assert_eq!(ear.choose(8.0, None), None);
    }

    #[test]
    fn a_deck_that_stops_drops_out_of_the_fit() {
        let mut ear = two_decks();
        run(&mut ear, 0.0, 8.0, |_| 0.2, |_| 1.0, None);
        assert_eq!(ear.current(), Some(1));
        // Deck 1 stops: only deck 0 is playing, so the ear follows it.
        let mut t = 8.0;
        while t < 16.0 {
            let e = ear.expected(0, 30.0 + t).unwrap();
            ear.listen(t, e, &[(0, 30.0 + t)]);
            t += 0.04;
        }
        assert_eq!(ear.choose(16.0, None), Some(0));
    }

    #[test]
    fn nnls_recovers_a_known_mix() {
        let a: Vec<f32> = (0..100).map(|i| ((i as f32) * 0.3).sin()).collect();
        let b: Vec<f32> = (0..100).map(|i| ((i as f32) * 0.07).cos()).collect();
        let x: Vec<f32> = a.iter().zip(&b).map(|(p, q)| 0.7 * p + 0.2 * q).collect();
        let w = nnls(&x, &[a, b]);
        assert!(
            (w[0] - 0.7).abs() < 0.02 && (w[1] - 0.2).abs() < 0.02,
            "{w:?}"
        );
        let neg: Vec<f32> = (0..100).map(|i| -((i as f32) * 0.3).sin()).collect();
        let w = nnls(
            &neg,
            &[(0..100).map(|i| ((i as f32) * 0.3).sin()).collect()],
        );
        assert!(w[0].abs() < 1e-6, "clamped at zero: {w:?}");
    }
}
