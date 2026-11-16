//! Reading rekordbox's live state from its process memory (Windows only). `process` opens
//! and reads; `chain` describes pointer paths; `offsets` stores them per version; `scan`
//! discovers them; `calibrator` runs the guided session that derives them; `reader` turns
//! calibrated chains into `TransportSnapshot`s; `window` raises rekordbox while the
//! calibrator needs its deck playing.
//!
//! Nothing here writes to the process.
#[cfg(windows)]
pub mod process;

pub mod calibrate;
pub mod chain;
pub mod deck_track;
pub mod offsets;
pub mod reader;

#[cfg(windows)]
pub mod scan;

#[cfg(windows)]
pub mod calibrator;

#[cfg(windows)]
pub mod handles;

#[cfg(windows)]
pub mod window;

/// The executable this reader targets.
pub const REKORDBOX_EXE: &str = "rekordbox.exe";
