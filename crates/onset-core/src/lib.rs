//! Onset domain model: tracks, beat grids, phrases, the playhead clock, the per-frame
//! `MusicState` and the director that turns structure into intensity. No I/O lives here.
#![forbid(unsafe_code)]

pub mod audio_features;
pub mod bands;
pub mod clock;
pub mod director;
pub mod grid;
pub mod hits;
pub mod lyrics;
pub mod music_state;
pub mod overlay_options;
pub mod phrase;
pub mod quality;
pub mod recording;
pub mod show;
pub mod structure;
pub mod track;
pub mod transport;
