//! Replays a recorded set (`onset_core::recording`, written by the engine's recorder) so
//! the renderer can be run on identical input before and after a scene or mapping change.
use std::path::Path;

use onset_core::music_state::MusicState;
use onset_core::recording::{DecodeError, decode_all};

/// A whole recording in memory: tens of megabytes for a ten-minute set at 120 Hz.
#[derive(Debug, Clone)]
pub struct Recording {
    states: Vec<MusicState>,
    /// The file ended mid-record (the recording process was killed); what came before
    /// is intact.
    pub truncated: bool,
}

impl Recording {
    pub fn read(path: &Path) -> anyhow::Result<Self> {
        let bytes = std::fs::read(path)
            .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", path.display()))?;
        Self::from_bytes(&bytes).map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DecodeError> {
        let (states, truncated) = decode_all(bytes)?;
        Ok(Self { states, truncated })
    }

    pub fn len(&self) -> usize {
        self.states.len()
    }

    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }

    /// Every recorded state, in order, each at its own `time_s`.
    pub fn frames(&self) -> impl ExactSizeIterator<Item = &MusicState> + '_ {
        self.states.iter()
    }

    /// `time_s` of the first and last states.
    pub fn span(&self) -> Option<(f64, f64)> {
        Some((self.states.first()?.time_s, self.states.last()?.time_s))
    }

    pub fn duration_s(&self) -> f64 {
        self.span().map_or(0.0, |(a, b)| b - a)
    }

    /// The newest state at or before `time_s`, as the renderer would have seen it: `None`
    /// before the recording starts.
    pub fn at(&self, time_s: f64) -> Option<&MusicState> {
        let n = self.states.partition_point(|s| s.time_s <= time_s);
        n.checked_sub(1).map(|i| &self.states[i])
    }

    /// The states a renderer running at `fps` would read, with each frame's own time:
    /// the recording's start plus the frame count over `fps`.
    pub fn sampled(&self, fps: f64) -> impl Iterator<Item = (f64, &MusicState)> + '_ {
        let (start, end) = self.span().unwrap_or((0.0, -1.0));
        let frames = if end < start {
            0
        } else {
            ((end - start) * fps).floor() as u64 + 1
        };
        (0..frames).map(move |f| {
            let t = start + f as f64 / fps;
            (t, self.at(t).expect("t is within the recording"))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use onset_core::recording::{Encoder, header};

    fn recording(times: &[f64]) -> Recording {
        let mut bytes = header().to_vec();
        let mut enc = Encoder::new();
        for &t in times {
            enc.push(
                &MusicState {
                    time_s: t,
                    intensity: t as f32,
                    ..MusicState::default()
                },
                &mut bytes,
            );
        }
        Recording::from_bytes(&bytes).unwrap()
    }

    #[test]
    fn at_returns_the_newest_state_not_after_the_time() {
        let r = recording(&[1.0, 1.1, 1.2]);
        assert!(r.at(0.5).is_none());
        assert!((r.at(1.0).unwrap().time_s - 1.0).abs() < f64::EPSILON);
        assert!((r.at(1.15).unwrap().time_s - 1.1).abs() < f64::EPSILON);
        assert!((r.at(9.0).unwrap().time_s - 1.2).abs() < f64::EPSILON);
        assert!((r.duration_s() - 0.2).abs() < 1e-9);
    }

    #[test]
    fn sampling_walks_the_recording_at_the_frame_rate() {
        let r = recording(&[2.0, 2.05, 2.1, 2.15, 2.2]);
        let frames: Vec<(f64, f64)> = r.sampled(10.0).map(|(t, s)| (t, s.time_s)).collect();
        assert_eq!(frames.len(), 3, "{frames:?}");
        assert!((frames[1].0 - 2.1).abs() < 1e-9);
        assert!((frames[1].1 - 2.1).abs() < 1e-9);
        assert!((frames[2].1 - 2.2).abs() < 1e-9);
        assert_eq!(recording(&[]).sampled(10.0).count(), 0);
    }
}
