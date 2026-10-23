//! Two-deck simulator: plays two tracks at once and mixes them by a scripted crossfade, the
//! way a DJ blends one track into the next, and reports a snapshot per deck. It exercises
//! what the one-deck simulator cannot: a room that hears one deck while the transport still
//! names the other, which is what the engine's deck-by-ear is for.
//!
//! Like rekordbox during a blend, both decks report playing; nothing here says which one
//! the room hears. [`BlendPlayer::poll`] keeps naming deck 0 while it plays, as a MASTER
//! flag left on the outgoing deck would, so only listening can move the show to deck 1.
//!
//! The mixer can drive an output device or run headless, advanced by hand, so tests can
//! play a blend faster than real time without an audio device.
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use onset_core::transport::{TrackRef, TransportSnapshot};

use crate::decode::DecodedAudio;
use crate::sim::sample_at;
use crate::source::{LiveDecks, SourceStatus, TransportSource};

/// Mono blocks handed to the analyzer.
const TAP_BLOCK: usize = 256;
/// Tap blocks buffered for the analyzer (about a third of a second at 48 kHz).
const TAP_QUEUE: usize = 64;

/// When each deck plays and how loud the room hears it, in seconds of the mix: deck 0
/// alone, an equal-power crossfade to deck 1, then deck 1 alone.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlendScript {
    /// When the crossfade from deck 0 to deck 1 starts.
    pub fade_start_s: f64,
    /// How long it takes.
    pub fade_s: f64,
    /// When deck 1 starts playing; it stays silent until the fade reaches it.
    pub start_1_s: f64,
    /// When deck 0 stops; `None` leaves it running, faded out, as DJs often do.
    pub stop_0_s: Option<f64>,
}

impl BlendScript {
    /// Deck 0 alone for `lead_s`, a blend of `bars` 4-beat bars at `bpm`, then deck 1
    /// alone; deck 1 starts with the fade and deck 0 keeps running.
    pub fn bars(lead_s: f64, bars: u32, bpm: f32) -> Self {
        let fade_s = f64::from(bars) * 4.0 * 60.0 / f64::from(bpm.max(1.0));
        Self {
            fade_start_s: lead_s,
            fade_s,
            start_1_s: lead_s,
            stop_0_s: None,
        }
    }

    pub fn fade_end_s(&self) -> f64 {
        self.fade_start_s + self.fade_s
    }

    /// When `deck` started and stopped playing.
    fn span(&self, deck: usize) -> (f64, f64) {
        if deck == 0 {
            (0.0, self.stop_0_s.unwrap_or(f64::INFINITY))
        } else {
            (self.start_1_s, f64::INFINITY)
        }
    }

    pub fn playing(&self, deck: usize, mix_s: f64) -> bool {
        let (start, stop) = self.span(deck);
        mix_s >= start && mix_s < stop
    }

    /// Seconds `deck` has played by `mix_s`: zero before it starts, held once it stops.
    pub fn played_s(&self, deck: usize, mix_s: f64) -> f64 {
        let (start, stop) = self.span(deck);
        (mix_s.min(stop) - start).max(0.0)
    }

    /// How loud the room hears each deck at `mix_s`, 0..1, after the crossfader.
    pub fn gains(&self, mix_s: f64) -> [f32; 2] {
        let x = if self.fade_s > 0.0 {
            ((mix_s - self.fade_start_s) / self.fade_s).clamp(0.0, 1.0)
        } else if mix_s < self.fade_start_s {
            0.0
        } else {
            1.0
        };
        let angle = x * std::f64::consts::FRAC_PI_2;
        let fader = [angle.cos() as f32, angle.sin() as f32];
        [0, 1].map(|d| {
            if self.playing(d, mix_s) {
                fader[d]
            } else {
                0.0
            }
        })
    }
}

/// One deck of the blend: its track and where and how fast it plays.
pub struct BlendDeck {
    pub audio: Arc<DecodedAudio>,
    pub track: TrackRef,
    /// The track's analysed tempo.
    pub bpm: f32,
    /// Where its playhead starts, seconds into the track.
    pub cue_s: f64,
    /// Playback rate; 1.0 is the file's tempo (a DJ beat-matching nudges it).
    pub rate: f64,
}

impl BlendDeck {
    fn playhead_s(&self, script: &BlendScript, deck: usize, mix_s: f64) -> f64 {
        self.cue_s + script.played_s(deck, mix_s) * self.rate
    }
}

/// What the audio side and the transport side share: how far the mix has played.
struct Shared {
    /// Output frames rendered while not paused.
    frames: AtomicU64,
    paused: AtomicBool,
    gain_bits: AtomicU32,
}

/// Renders the blend: the room's sound to the output and, before the output gain, to the
/// analyzer tap.
struct Mixer {
    decks: Arc<[BlendDeck; 2]>,
    script: BlendScript,
    out_rate: u32,
    shared: Arc<Shared>,
    tap: crossbeam_channel::Sender<Vec<f32>>,
    mono_block: Vec<f32>,
}

impl Mixer {
    /// Fills `out` (interleaved, `channels` per frame) and advances the mix.
    fn render(&mut self, out: &mut [f32], channels: usize) {
        let paused = self.shared.paused.load(Ordering::Relaxed);
        let g = f32::from_bits(self.shared.gain_bits.load(Ordering::Relaxed));
        let mut frames = self.shared.frames.load(Ordering::Relaxed);
        let rate = f64::from(self.out_rate);
        for frame in out.chunks_mut(channels.max(1)) {
            let (l, r) = if paused {
                (0.0, 0.0)
            } else {
                let mix_s = frames as f64 / rate;
                frames += 1;
                let gains = self.script.gains(mix_s);
                let mut sum = (0.0, 0.0);
                for (d, deck) in self.decks.iter().enumerate() {
                    if gains[d] <= 0.0 {
                        continue;
                    }
                    let at = deck.playhead_s(&self.script, d, mix_s);
                    let (dl, dr) = sample_at(&deck.audio, at * f64::from(deck.audio.sample_rate));
                    sum.0 += dl * gains[d];
                    sum.1 += dr * gains[d];
                }
                sum
            };
            for (c, slot) in frame.iter_mut().enumerate() {
                *slot = if c % 2 == 0 { l } else { r } * g;
            }
            self.mono_block.push(f32::midpoint(l, r));
            if self.mono_block.len() == TAP_BLOCK {
                let _ = self.tap.try_send(std::mem::replace(
                    &mut self.mono_block,
                    Vec::with_capacity(TAP_BLOCK),
                ));
            }
        }
        self.shared.frames.store(frames, Ordering::Relaxed);
    }
}

/// How the mix is driven: by an output device, or by hand.
enum Drive {
    /// The stream plays while it is held.
    Device { _stream: cpal::Stream },
    /// Headless: [`BlendPlayer::advance`] renders, and the mix's own time, counted from
    /// this instant, stamps the snapshots.
    Manual { mixer: Mixer, epoch: Instant },
}

pub struct BlendPlayer {
    drive: Drive,
    decks: Arc<[BlendDeck; 2]>,
    script: BlendScript,
    out_rate: u32,
    shared: Arc<Shared>,
    audio_rx: crossbeam_channel::Receiver<Vec<f32>>,
}

impl BlendPlayer {
    fn parts(
        decks: [BlendDeck; 2],
        script: BlendScript,
        out_rate: u32,
    ) -> (Mixer, Arc<Shared>, crossbeam_channel::Receiver<Vec<f32>>) {
        let shared = Arc::new(Shared {
            frames: AtomicU64::new(0),
            paused: AtomicBool::new(false),
            gain_bits: AtomicU32::new(1.0f32.to_bits()),
        });
        let (tap, audio_rx) = crossbeam_channel::bounded::<Vec<f32>>(TAP_QUEUE);
        let mixer = Mixer {
            decks: Arc::new(decks),
            script,
            out_rate,
            shared: shared.clone(),
            tap,
            mono_block: Vec::with_capacity(TAP_BLOCK),
        };
        (mixer, shared, audio_rx)
    }

    /// Plays the blend through the default output device, starting now.
    pub fn start(decks: [BlendDeck; 2], script: BlendScript) -> anyhow::Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| anyhow::anyhow!("no default output device"))?;
        let supported = device.default_output_config()?;
        let out_rate = supported.sample_rate();
        let config: cpal::StreamConfig = supported.config();
        let channels = usize::from(config.channels);
        let (mut mixer, shared, audio_rx) = Self::parts(decks, script, out_rate);
        let decks = mixer.decks.clone();
        let stream = device.build_output_stream(
            config,
            move |out: &mut [f32], _| mixer.render(out, channels),
            |e| tracing::error!("blend output stream error: {e}"),
            None,
        )?;
        stream.play()?;
        tracing::info!(
            out_rate,
            fade_start_s = script.fade_start_s,
            fade_s = script.fade_s,
            "two-deck simulator started"
        );
        Ok(Self {
            drive: Drive::Device { _stream: stream },
            decks,
            script,
            out_rate,
            shared,
            audio_rx,
        })
    }

    /// A blend that plays only when [`Self::advance`] says so, rendering at `out_rate`
    /// with no device; its snapshots are stamped `epoch` plus the mix's time.
    pub fn headless(
        decks: [BlendDeck; 2],
        script: BlendScript,
        out_rate: u32,
        epoch: Instant,
    ) -> Self {
        let (mixer, shared, audio_rx) = Self::parts(decks, script, out_rate);
        let decks = mixer.decks.clone();
        Self {
            drive: Drive::Manual { mixer, epoch },
            decks,
            script,
            out_rate,
            shared,
            audio_rx,
        }
    }

    /// Headless only: renders the next `secs` of the mix into the tap. Steps longer than
    /// the tap holds (a third of a second or so) drop blocks unless the tap is drained.
    pub fn advance(&mut self, secs: f64) {
        let Drive::Manual { mixer, .. } = &mut self.drive else {
            return;
        };
        let frames = (secs.max(0.0) * f64::from(self.out_rate)).round() as usize;
        let mut out = vec![0.0f32; frames];
        mixer.render(&mut out, 1);
    }

    pub fn pause(&self, paused: bool) {
        self.shared.paused.store(paused, Ordering::Relaxed);
    }

    /// Output gain, 0..1. The analyzer tap is taken before gain.
    pub fn set_gain(&self, gain: f32) {
        self.shared
            .gain_bits
            .store(gain.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    /// Mono blocks of the room's sound at [`Self::tap_sample_rate`], pre-gain.
    pub fn tap_audio(&self) -> crossbeam_channel::Receiver<Vec<f32>> {
        self.audio_rx.clone()
    }

    pub fn tap_sample_rate(&self) -> u32 {
        self.out_rate
    }

    pub fn script(&self) -> &BlendScript {
        &self.script
    }

    /// Seconds of the mix played so far.
    pub fn mix_s(&self) -> f64 {
        self.shared.frames.load(Ordering::Relaxed) as f64 / f64::from(self.out_rate)
    }

    fn snapshot(&self, deck: usize, mix_s: f64, read_at: Instant) -> TransportSnapshot {
        let d = &self.decks[deck];
        TransportSnapshot {
            deck: deck as u8,
            track: d.track.clone(),
            playhead_s: d.playhead_s(&self.script, deck, mix_s),
            bpm_now: d.bpm * d.rate as f32,
            bpm_original: d.bpm,
            playing: self.script.playing(deck, mix_s)
                && !self.shared.paused.load(Ordering::Relaxed),
            read_at,
        }
    }

    fn read_at(&self, mix_s: f64) -> Instant {
        match &self.drive {
            Drive::Manual { epoch, .. } => *epoch + Duration::from_secs_f64(mix_s),
            Drive::Device { .. } => Instant::now(),
        }
    }

    /// The deck the transport names: deck 0 while it plays, as a MASTER flag left on the
    /// outgoing deck would, then deck 1.
    fn named(&self, mix_s: f64) -> usize {
        usize::from(!self.script.playing(0, mix_s))
    }
}

impl TransportSource for BlendPlayer {
    fn name(&self) -> &'static str {
        "sim-blend"
    }

    fn status(&self) -> SourceStatus {
        SourceStatus::Connected
    }

    fn poll(&mut self) -> Option<TransportSnapshot> {
        let mix_s = self.mix_s();
        Some(self.snapshot(self.named(mix_s), mix_s, self.read_at(mix_s)))
    }

    fn live_decks(&self) -> LiveDecks {
        let mix_s = self.mix_s();
        let read_at = self.read_at(mix_s);
        LiveDecks {
            decks: (0..2).map(|d| self.snapshot(d, mix_s, read_at)).collect(),
            master: u8::try_from(self.named(mix_s)).ok(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mono track holding `level` throughout, at 1 kHz.
    fn flat(level: f32, secs: f64) -> Arc<DecodedAudio> {
        Arc::new(DecodedAudio {
            sample_rate: 1000,
            channels: 1,
            samples: vec![level; (secs * 1000.0) as usize],
        })
    }

    fn deck(id: u64, level: f32, cue_s: f64) -> BlendDeck {
        BlendDeck {
            audio: flat(level, 120.0),
            track: TrackRef::Id(onset_core::track::TrackId(id)),
            bpm: 128.0,
            cue_s,
            rate: 1.0,
        }
    }

    #[test]
    fn script_fades_equal_power_from_deck_0_to_deck_1() {
        let s = BlendScript::bars(10.0, 16, 128.0);
        assert!((s.fade_s - 30.0).abs() < 1e-9);
        let alone = s.gains(5.0);
        assert!((alone[0] - 1.0).abs() < 1e-6 && alone[1].abs() < 1e-6);
        assert!(!s.playing(1, 5.0) && s.playing(1, 10.0));
        let mid = s.gains(25.0);
        assert!(
            (mid[0] - mid[1]).abs() < 1e-6
                && (mid[0] * mid[0] + mid[1] * mid[1] - 1.0).abs() < 1e-5
        );
        let end = s.gains(45.0);
        assert!(end[0].abs() < 1e-6 && (end[1] - 1.0).abs() < 1e-6);
        assert!(s.playing(0, 45.0), "deck 0 keeps running, faded out");
        let stopped = BlendScript {
            stop_0_s: Some(40.0),
            ..s
        };
        assert!(!stopped.playing(0, 41.0));
        assert!(
            (stopped.played_s(0, 50.0) - 40.0).abs() < 1e-9,
            "held at the stop"
        );
    }

    #[test]
    fn headless_blend_reports_both_decks_and_taps_the_mix() {
        let epoch = Instant::now();
        let script = BlendScript::bars(2.0, 1, 120.0); // a 2 s fade
        let mut p = BlendPlayer::headless(
            [deck(1, 0.5, 30.0), deck(2, 0.25, 0.0)],
            script,
            1000,
            epoch,
        );
        let tap = p.tap_audio();
        let mut heard = Vec::new();
        let at = |p: &mut BlendPlayer, heard: &mut Vec<f32>, secs: f64| {
            for _ in 0..(secs * 10.0).round() as usize {
                p.advance(0.1);
                while let Ok(b) = tap.try_recv() {
                    heard.extend(b);
                }
            }
        };

        at(&mut p, &mut heard, 1.0);
        let live = p.live_decks();
        assert_eq!(live.decks.len(), 2);
        assert!(live.decks[0].playing && !live.decks[1].playing);
        assert!((live.decks[0].playhead_s - 31.0).abs() < 1e-6);
        assert!(live.decks[1].playhead_s.abs() < 1e-9);
        assert_eq!(live.master, Some(0));
        assert_eq!(live.decks[0].read_at, epoch + Duration::from_secs(1));

        at(&mut p, &mut heard, 4.0); // 5 s: the fade is over, both decks still play
        let live = p.live_decks();
        assert!(live.decks.iter().all(|d| d.playing));
        assert!((live.decks[1].playhead_s - 3.0).abs() < 1e-6);
        let polled = p.poll().unwrap();
        assert_eq!(
            polled.deck, 0,
            "the transport still names the outgoing deck"
        );

        // What the tap heard: deck 0 alone, then deck 1 alone (mono = the decks' level).
        assert!((heard[500] - 0.5).abs() < 1e-6, "{}", heard[500]);
        assert!((heard[4800] - 0.25).abs() < 1e-3, "{}", heard[4800]);
        // Equal power halfway through the fade: both at cos 45°.
        let mid = (0.5 + 0.25) * std::f32::consts::FRAC_1_SQRT_2;
        assert!((heard[3000] - mid).abs() < 1e-3, "{}", heard[3000]);

        p.pause(true);
        at(&mut p, &mut heard, 1.0);
        assert!(
            (p.mix_s() - 5.0).abs() < 1e-3,
            "paused mix does not advance"
        );
        assert!(p.live_decks().decks.iter().all(|d| !d.playing));
    }

    #[test]
    fn transport_names_deck_1_once_deck_0_stops() {
        let script = BlendScript {
            stop_0_s: Some(3.0),
            ..BlendScript::bars(1.0, 1, 120.0)
        };
        let mut p = BlendPlayer::headless(
            [deck(1, 0.5, 0.0), deck(2, 0.5, 0.0)],
            script,
            1000,
            Instant::now(),
        );
        for _ in 0..40 {
            p.advance(0.1);
        }
        assert_eq!(p.poll().unwrap().deck, 1);
        assert_eq!(p.live_decks().master, Some(1));
    }
}
