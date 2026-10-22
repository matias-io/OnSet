//! The interface every live-state source implements.
use onset_core::transport::TransportSnapshot;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceStatus {
    /// Delivering snapshots.
    Connected,
    /// Waiting for its target (rekordbox not running, no OSC yet).
    Searching,
    /// Attached to the target and still locating what it reads (the rekordbox decks).
    Scanning,
    /// The target exists but this source cannot read it (e.g. no offsets for this version).
    Unsupported(String),
    Error(String),
}

pub trait TransportSource: Send {
    fn name(&self) -> &'static str;
    fn status(&self) -> SourceStatus;
    /// Called at up to 120 Hz. `None` when nothing new can be reported.
    fn poll(&mut self) -> Option<TransportSnapshot>;

    /// What the library knows about the track the engine believes deck `deck` holds: its
    /// sample rate, for sources whose position counts in samples of the file (rekordbox),
    /// and its analysed tempo, for sources without a readable tempo field. Kept per deck
    /// until that deck takes another track. Default: ignored.
    fn set_deck_track(&mut self, _deck: u8, _sample_rate: Option<u32>, _bpm: Option<f32>) {}

    /// Lengths of the library's files in seconds, for sources that learn which files are
    /// loaded but have to work out which deck holds which. Default: ignored.
    fn set_file_durations(&mut self, _table: Vec<(std::path::PathBuf, f64)>) {}

    /// The library's BPM for each file, for the same purpose. Default: ignored.
    fn set_file_bpms(&mut self, _table: Vec<(std::path::PathBuf, f32)>) {}
}
