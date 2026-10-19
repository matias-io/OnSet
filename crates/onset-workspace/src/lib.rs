//! Read-only access to the preparation workspace: the structure, cues and energy the DJ
//! prepared and corrected before the show. The Python service owns the database and is its
//! only writer; the show opens it read-only, so it can never change it.
//!
//! [`Workspace::prepared`] finds a track by its rekordbox content id or file path and returns
//! what preparation knows about it; [`phrase_map`] and [`merged_cues`] turn that into the
//! structure and cues the show director already understands.

mod convert;
mod workspace;

pub use convert::{merged_cues, phrase_map};
pub use workspace::{DB_NAME, Error, Workspace, default_home, move_legacy_home};

/// One section of a track as preparation resolved it, in seconds of track time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Section {
    pub kind: SectionKind,
    pub start_s: f64,
    pub end_s: f64,
}

/// The service's section vocabulary (`onset_service/structure.py` `LABELS`), without the silence
/// and unknown labels, which carry no structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SectionKind {
    Intro,
    Groove,
    Vocal,
    Breakdown,
    Build,
    Drop,
    Stripped,
    Outro,
}

impl SectionKind {
    /// Parses one of the service's section labels; `None` for silence, unknown and anything new.
    pub fn from_label(label: &str) -> Option<Self> {
        Some(match label {
            "intro" => Self::Intro,
            "groove" => Self::Groove,
            "vocal" => Self::Vocal,
            "breakdown" => Self::Breakdown,
            "build" => Self::Build,
            "drop" => Self::Drop,
            "stripped" => Self::Stripped,
            "outro" => Self::Outro,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Intro => "Intro",
            Self::Groove => "Groove",
            Self::Vocal => "Vocal",
            Self::Breakdown => "Breakdown",
            Self::Build => "Build",
            Self::Drop => "Drop",
            Self::Stripped => "Stripped",
            Self::Outro => "Outro",
        }
    }
}

/// Where the sections came from inside the track's profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionSource {
    /// The full resolved structure (`analysis.structure.sections`).
    Resolved,
    /// Built from the key events alone (first full groove, breakdown, later drop, outro),
    /// as historical imports and the event baseline store them.
    Events,
}

/// A cue the DJ approved in cue review.
#[derive(Debug, Clone, PartialEq)]
pub struct ApprovedCue {
    /// The cue's role letter, e.g. `D` for the first sustained full groove.
    pub role: String,
    pub name: String,
    pub time_s: f64,
    /// Loop end, for loop cues.
    pub end_s: Option<f64>,
}

/// Everything the show takes from preparation for one track.
#[derive(Debug, Clone, PartialEq)]
pub struct Prepared {
    /// The workspace's internal track id.
    pub track_id: String,
    pub sections: Vec<Section>,
    pub section_source: SectionSource,
    pub approved_cues: Vec<ApprovedCue>,
    /// Combined energy per bar, on the service's own scale; empty when not analysed.
    pub energy_per_bar: Vec<f32>,
}
