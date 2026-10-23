//! A synthetic set for the headless tests: four-on-the-floor at 128 BPM with a build-up
//! and a drop. The gallery renders it directly; the replay tests record and replay it.
#![allow(dead_code)] // each test binary uses a subset

use onset_core::audio_features::{AudioFeatures, BANDS};
use onset_core::music_state::MusicState;
use onset_core::phrase::{Mood, PhraseKind};
use onset_core::track::{TrackId, TrackMeta};

pub const BPM: f32 = 128.0;

pub fn shader_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/shaders")
}

pub fn track() -> TrackMeta {
    TrackMeta {
        id: TrackId(42),
        title: "Gallery".into(),
        artist: "Onset".into(),
        album: String::new(),
        year: None,
        key: Some("8A".into()),
        bpm: Some(BPM),
        duration_s: Some(240.0),
        sample_rate: Some(44_100),
        file_path: None,
        artwork_path: std::env::var_os("ONSET_GALLERY_ART").map(Into::into),
        analysis_path: None,
        isrc: None,
        genre: Some("Techno".into()),
        extra: onset_core::track::TrackExtra::default(),
    }
}

/// Four-on-the-floor with hats on the offbeat and a snare on 2 and 4. `bass` and `treble`
/// scale those halves of the spectrum.
pub fn audio(t: f32, bass: f32, treble: f32) -> AudioFeatures {
    let beat = t * BPM / 60.0;
    let phase = beat.fract();
    let kick = (1.0 - phase).powf(6.0);
    let hat = (1.0 - (phase + 0.5).fract()).powf(10.0);
    let snare = if (beat as u32) % 2 == 1 {
        (1.0 - phase).powf(8.0)
    } else {
        0.0
    };
    let mut a = AudioFeatures::silent();
    a.silent = false;
    for (i, l) in a.levels.iter_mut().enumerate() {
        let x = i as f32 / (BANDS - 1) as f32;
        let wobble = 0.5 + 0.5 * (t * 1.7 + x * 9.0).sin();
        *l = if x < 0.3 {
            bass * (0.4 + 0.6 * kick)
        } else if x < 0.7 {
            0.3 + 0.4 * wobble * (0.5 + snare)
        } else {
            treble * (0.3 + 0.7 * hat) * wobble
        }
        .clamp(0.0, 1.0);
    }
    a.bands = a.levels;
    for (g, (lo, hi)) in
        a.groups
            .iter_mut()
            .zip([(0, 3), (3, 6), (6, 10), (10, 14), (14, 18), (18, 24)])
    {
        *g = a.levels[lo..hi].iter().sum::<f32>() / (hi - lo) as f32;
    }
    a.kick = kick * bass;
    a.snare = snare;
    a.hat = hat * treble;
    a.loudness = 0.5 + 0.4 * bass;
    a.brightness = 0.3 + 0.4 * treble;
    a.rms = 0.3;
    a
}

/// The state at `t` seconds: a build-up until `drop_at`, then the chorus.
pub fn state(t: f32, drop_at: f32, bass: f32, treble: f32) -> MusicState {
    let beats = t * BPM / 60.0;
    let chorus = t >= drop_at;
    let beats_to_drop = ((drop_at - t) * BPM / 60.0).ceil().max(0.0) as u32;
    let audio = audio(t, bass, treble);
    MusicState {
        time_s: f64::from(t),
        playhead_s: f64::from(t) + 60.0,
        playing: true,
        bpm: BPM,
        bpm_grid: BPM,
        beat_phase: beats.fract(),
        bar_phase: (beats / 4.0).fract(),
        phrase_phase: (beats / 32.0).fract(),
        phrase: Some(if chorus {
            PhraseKind::Chorus
        } else {
            PhraseKind::Up
        }),
        next_phrase: Some(if chorus {
            PhraseKind::Down
        } else {
            PhraseKind::Chorus
        }),
        beats_to_next_phrase: Some(if chorus { 32 } else { beats_to_drop }),
        drop_countdown_beats: (!chorus).then_some(beats_to_drop),
        intensity: if chorus {
            1.0
        } else {
            0.55 + 0.25 * (t / drop_at)
        },
        hits: onset_core::hits::BeatHits {
            kick: audio.kick,
            snare: audio.snare,
            audio_kick: audio.kick,
            audio_snare: audio.snare,
            ..onset_core::hits::BeatHits::default()
        },
        audio,
        track: Some(track()),
        beat_index: Some(128 + beats as u32),
        mood: Some(Mood::High),
        ..MusicState::default()
    }
}

/// Mean absolute difference per channel between two RGBA frames.
pub fn diff(a: &[u8], b: &[u8]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(x, y)| f64::from((i16::from(*x) - i16::from(*y)).unsigned_abs()))
        .sum::<f64>()
        / a.len() as f64
}
