//! Writes every tick's `MusicState` to a file in `onset_core::recording`'s format, so a
//! real set can be replayed through the headless renderer later.
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use onset_core::music_state::MusicState;
use onset_core::recording::{Encoder, header};

/// How many states go out between flushes: a killed process loses at most this many ticks.
const FLUSH_EVERY: u64 = 12;

pub struct Recorder {
    path: PathBuf,
    file: BufWriter<File>,
    encoder: Encoder,
    buf: Vec<u8>,
    frames: u64,
}

impl Recorder {
    /// Creates `path` (and its folder) and writes the header.
    pub fn create(path: &Path) -> std::io::Result<Self> {
        if let Some(dir) = path.parent()
            && !dir.as_os_str().is_empty()
        {
            std::fs::create_dir_all(dir)?;
        }
        let mut file = BufWriter::new(File::create(path)?);
        file.write_all(&header())?;
        Ok(Self {
            path: path.to_path_buf(),
            file,
            encoder: Encoder::new(),
            buf: Vec::with_capacity(1024),
            frames: 0,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn frames(&self) -> u64 {
        self.frames
    }

    pub fn push(&mut self, ms: &MusicState) -> std::io::Result<()> {
        self.buf.clear();
        self.encoder.push(ms, &mut self.buf);
        self.file.write_all(&self.buf)?;
        self.frames += 1;
        if self.frames.is_multiple_of(FLUSH_EVERY) {
            self.file.flush()?;
        }
        Ok(())
    }

    /// Flushes and closes the file; returns where it is and how many states it holds.
    pub fn finish(mut self) -> std::io::Result<(PathBuf, u64)> {
        self.file.flush()?;
        Ok((self.path, self.frames))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use onset_core::recording::decode_all;

    #[test]
    fn writes_a_file_the_decoder_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sets").join("one.onsetrec");
        let mut rec = Recorder::create(&path).unwrap();
        for i in 0..30u8 {
            let ms = MusicState {
                time_s: f64::from(i) / 120.0,
                intensity: 0.5 + f32::from(i) * 0.01,
                ..MusicState::default()
            };
            rec.push(&ms).unwrap();
        }
        let (where_, n) = rec.finish().unwrap();
        assert_eq!(where_, path);
        assert_eq!(n, 30);
        let (states, truncated) = decode_all(&std::fs::read(&path).unwrap()).unwrap();
        assert!(!truncated);
        assert_eq!(states.len(), 30);
        assert!((states[29].intensity - 0.79).abs() < 1e-6);
    }
}
