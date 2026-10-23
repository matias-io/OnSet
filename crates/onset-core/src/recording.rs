//! A recording of the engine's `MusicState`s, one per tick, so a real set can be captured
//! once and replayed through the renderer on identical input. This module is the byte
//! format only: the engine writes the bytes to a file, the renderer reads them back.
//!
//! Layout, all little-endian: an 8-byte magic and a `u32` version, then records of a `u32`
//! length followed by that many bytes. A record starts with a kind byte: `1` carries the
//! track (written only when it changes), `2` one `MusicState` without its track. Fields
//! that `FrameUniforms` never reads (file paths, cue lists, the track's tags) are left out,
//! so a recording of a set holds no paths from the DJ's machine.
use crate::audio_features::{AudioFeatures, BANDS, GROUPS};
use crate::bands::BandsAt;
use crate::hits::BeatHits;
use crate::music_state::{CueAhead, MusicState};
use crate::phrase::{Mood, PhraseKind};
use crate::structure::StructureSource;
use crate::track::{TrackExtra, TrackId, TrackMeta};

pub const MAGIC: [u8; 8] = *b"ONSETREC";
pub const VERSION: u32 = 1;
/// The file extension recordings are written with.
pub const EXTENSION: &str = "onsetrec";

const KIND_TRACK: u8 = 1;
const KIND_STATE: u8 = 2;
const NONE_U32: u32 = u32::MAX;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DecodeError {
    #[error("not an Onset recording (bad magic)")]
    BadMagic,
    #[error("recording version {0} is newer than this build reads ({VERSION})")]
    Version(u32),
    #[error("the recording ends in the middle of a record")]
    Truncated,
    #[error("unknown record kind {0}")]
    Kind(u8),
    #[error("a text field is not UTF-8")]
    Text,
}

/// The 12-byte file header.
pub fn header() -> [u8; 12] {
    let mut h = [0u8; 12];
    h[..8].copy_from_slice(&MAGIC);
    h[8..].copy_from_slice(&VERSION.to_le_bytes());
    h
}

/// Appends states to a byte stream, writing the track only when it changes.
#[derive(Debug, Default)]
pub struct Encoder {
    /// The track the last record named, once one has been written.
    last_track: Option<TrackId>,
    started: bool,
}

impl Encoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends one or two records (a track change, then the state) to `out`.
    pub fn push(&mut self, ms: &MusicState, out: &mut Vec<u8>) {
        let track = ms.track.as_ref().map(|t| t.id);
        if !self.started || self.last_track != track {
            self.started = true;
            self.last_track = track;
            record(out, KIND_TRACK, |w| write_track(w, ms.track.as_ref()));
        }
        record(out, KIND_STATE, |w| write_state(w, ms));
    }
}

fn record(out: &mut Vec<u8>, kind: u8, body: impl FnOnce(&mut Vec<u8>)) {
    let at = out.len();
    out.extend_from_slice(&0u32.to_le_bytes());
    out.push(kind);
    body(out);
    let len = (out.len() - at - 4) as u32;
    out[at..at + 4].copy_from_slice(&len.to_le_bytes());
}

fn put_f32(w: &mut Vec<u8>, v: f32) {
    w.extend_from_slice(&v.to_le_bytes());
}

fn put_u32(w: &mut Vec<u8>, v: u32) {
    w.extend_from_slice(&v.to_le_bytes());
}

fn put_opt_u32(w: &mut Vec<u8>, v: Option<u32>) {
    put_u32(w, v.unwrap_or(NONE_U32));
}

fn put_str(w: &mut Vec<u8>, s: &str) {
    let bytes = &s.as_bytes()[..s.len().min(usize::from(u16::MAX))];
    // Never cut a UTF-8 sequence in half.
    let mut end = bytes.len();
    while end < s.len() && !s.is_char_boundary(end) {
        end -= 1;
    }
    w.extend_from_slice(&(end as u16).to_le_bytes());
    w.extend_from_slice(&bytes[..end]);
}

fn put_opt_str(w: &mut Vec<u8>, s: Option<&str>) {
    match s {
        Some(s) => {
            w.push(1);
            put_str(w, s);
        }
        None => w.push(0),
    }
}

fn phrase_byte(p: Option<PhraseKind>) -> u8 {
    match p {
        None => 0,
        Some(PhraseKind::Intro) => 1,
        Some(PhraseKind::Verse) => 2,
        Some(PhraseKind::Bridge) => 3,
        Some(PhraseKind::Chorus) => 4,
        Some(PhraseKind::Outro) => 5,
        Some(PhraseKind::Up) => 6,
        Some(PhraseKind::Down) => 7,
    }
}

fn phrase_from(b: u8) -> Option<PhraseKind> {
    Some(match b {
        1 => PhraseKind::Intro,
        2 => PhraseKind::Verse,
        3 => PhraseKind::Bridge,
        4 => PhraseKind::Chorus,
        5 => PhraseKind::Outro,
        6 => PhraseKind::Up,
        7 => PhraseKind::Down,
        _ => return None,
    })
}

fn write_track(w: &mut Vec<u8>, t: Option<&TrackMeta>) {
    let Some(t) = t else {
        w.push(0);
        return;
    };
    w.push(1);
    w.extend_from_slice(&t.id.0.to_le_bytes());
    put_str(w, &t.title);
    put_str(w, &t.artist);
    put_str(w, &t.album);
    match t.year {
        Some(y) => {
            w.push(1);
            w.extend_from_slice(&y.to_le_bytes());
        }
        None => w.push(0),
    }
    put_opt_str(w, t.key.as_deref());
    put_f32(w, t.bpm.unwrap_or(f32::NAN));
    put_f32(w, t.duration_s.unwrap_or(f32::NAN));
    put_opt_u32(w, t.sample_rate);
    put_opt_str(w, t.genre.as_deref());
}

fn write_state(w: &mut Vec<u8>, ms: &MusicState) {
    w.extend_from_slice(&ms.time_s.to_le_bytes());
    w.extend_from_slice(&ms.playhead_s.to_le_bytes());
    let flags = u8::from(ms.playing)
        | u8::from(ms.audio.onset) << 1
        | u8::from(ms.audio.silent) << 2
        | u8::from(ms.hits.locked) << 3;
    w.push(flags);
    for v in [
        ms.bpm,
        ms.bpm_grid,
        ms.beat_phase,
        ms.bar_phase,
        ms.phrase_phase,
        ms.intensity,
    ] {
        put_f32(w, v);
    }
    w.push(phrase_byte(ms.phrase));
    w.push(phrase_byte(ms.next_phrase));
    put_opt_u32(w, ms.beats_to_next_phrase);
    put_opt_u32(w, ms.drop_countdown_beats);
    put_opt_u32(w, ms.beat_index);
    w.push(match ms.mood {
        None => 0,
        Some(Mood::Low) => 1,
        Some(Mood::Mid) => 2,
        Some(Mood::High) => 3,
    });
    w.push(match ms.structure_source {
        None => 0,
        Some(StructureSource::Rekordbox) => 1,
        Some(StructureSource::Prepared) => 2,
    });
    let a = &ms.audio;
    for v in a.bands.iter().chain(&a.levels).chain(&a.groups) {
        put_f32(w, *v);
    }
    for v in [a.rms, a.kick, a.snare, a.hat, a.loudness, a.brightness] {
        put_f32(w, v);
    }
    for c in &ms.theme {
        for v in c {
            put_f32(w, *v);
        }
    }
    for v in [
        ms.analysis.low,
        ms.analysis.mid,
        ms.analysis.high,
        ms.analysis.vocal,
    ] {
        put_f32(w, v);
    }
    let h = &ms.hits;
    for v in [
        h.kick,
        h.snare,
        h.audio_kick,
        h.audio_snare,
        h.kick_confidence,
        h.snare_confidence,
        h.lag_s,
    ] {
        put_f32(w, v);
    }
    put_u32(w, h.lag_samples);
    match ms.next_cue {
        Some(c) => {
            w.push(1);
            w.push(c.slot);
            put_f32(w, c.seconds);
            match c.color {
                Some(rgb) => {
                    w.push(1);
                    w.extend_from_slice(&rgb);
                }
                None => w.push(0),
            }
        }
        None => w.push(0),
    }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        let end = self.pos.checked_add(n).ok_or(DecodeError::Truncated)?;
        let s = self.bytes.get(self.pos..end).ok_or(DecodeError::Truncated)?;
        self.pos = end;
        Ok(s)
    }

    fn u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, DecodeError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    fn u32(&mut self) -> Result<u32, DecodeError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn opt_u32(&mut self) -> Result<Option<u32>, DecodeError> {
        Ok(Some(self.u32()?).filter(|v| *v != NONE_U32))
    }

    fn u64(&mut self) -> Result<u64, DecodeError> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    fn f32(&mut self) -> Result<f32, DecodeError> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn f64(&mut self) -> Result<f64, DecodeError> {
        Ok(f64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    fn str(&mut self) -> Result<String, DecodeError> {
        let n = usize::from(self.u16()?);
        std::str::from_utf8(self.take(n)?)
            .map(str::to_string)
            .map_err(|_| DecodeError::Text)
    }

    fn opt_str(&mut self) -> Result<Option<String>, DecodeError> {
        if self.u8()? == 0 {
            Ok(None)
        } else {
            self.str().map(Some)
        }
    }

    fn f32s<const N: usize>(&mut self) -> Result<[f32; N], DecodeError> {
        let mut out = [0.0f32; N];
        for v in &mut out {
            *v = self.f32()?;
        }
        Ok(out)
    }
}

fn read_track(c: &mut Cursor) -> Result<Option<TrackMeta>, DecodeError> {
    if c.u8()? == 0 {
        return Ok(None);
    }
    let id = TrackId(c.u64()?);
    let title = c.str()?;
    let artist = c.str()?;
    let album = c.str()?;
    let year = if c.u8()? == 0 {
        None
    } else {
        Some(c.u16()?)
    };
    let key = c.opt_str()?;
    let bpm = Some(c.f32()?).filter(|v| !v.is_nan());
    let duration_s = Some(c.f32()?).filter(|v| !v.is_nan());
    let sample_rate = c.opt_u32()?;
    let genre = c.opt_str()?;
    Ok(Some(TrackMeta {
        id,
        title,
        artist,
        album,
        year,
        key,
        bpm,
        duration_s,
        sample_rate,
        file_path: None,
        artwork_path: None,
        analysis_path: None,
        isrc: None,
        genre,
        extra: TrackExtra::default(),
    }))
}

fn read_audio(c: &mut Cursor, flags: u8) -> Result<AudioFeatures, DecodeError> {
    let bands = c.f32s::<BANDS>()?;
    let levels = c.f32s::<BANDS>()?;
    let groups = c.f32s::<GROUPS>()?;
    let [rms, kick, snare, hat, loudness, brightness] = c.f32s::<6>()?;
    Ok(AudioFeatures {
        bands,
        rms,
        onset: flags & 2 != 0,
        silent: flags & 4 != 0,
        levels,
        groups,
        kick,
        snare,
        hat,
        loudness,
        brightness,
    })
}

fn read_hits(c: &mut Cursor, flags: u8) -> Result<BeatHits, DecodeError> {
    let [
        kick,
        snare,
        audio_kick,
        audio_snare,
        kick_confidence,
        snare_confidence,
        lag_s,
    ] = c.f32s::<7>()?;
    let lag_samples = c.u32()?;
    Ok(BeatHits {
        kick,
        snare,
        audio_kick,
        audio_snare,
        kick_confidence,
        snare_confidence,
        lag_s,
        lag_samples,
        locked: flags & 8 != 0,
    })
}

fn read_state(c: &mut Cursor, track: Option<&TrackMeta>) -> Result<MusicState, DecodeError> {
    let time_s = c.f64()?;
    let playhead_s = c.f64()?;
    let flags = c.u8()?;
    let [bpm, bpm_grid, beat_phase, bar_phase, phrase_phase, intensity] = c.f32s::<6>()?;
    let phrase = phrase_from(c.u8()?);
    let next_phrase = phrase_from(c.u8()?);
    let beats_to_next_phrase = c.opt_u32()?;
    let drop_countdown_beats = c.opt_u32()?;
    let beat_index = c.opt_u32()?;
    let mood = match c.u8()? {
        1 => Some(Mood::Low),
        2 => Some(Mood::Mid),
        3 => Some(Mood::High),
        _ => None,
    };
    let structure_source = match c.u8()? {
        1 => Some(StructureSource::Rekordbox),
        2 => Some(StructureSource::Prepared),
        _ => None,
    };
    let audio = read_audio(c, flags)?;
    let mut theme = [[0.0f32; 3]; 5];
    for colour in &mut theme {
        *colour = c.f32s::<3>()?;
    }
    let [low, mid, high, vocal] = c.f32s::<4>()?;
    let hits = read_hits(c, flags)?;
    let next_cue = if c.u8()? == 0 {
        None
    } else {
        let slot = c.u8()?;
        let seconds = c.f32()?;
        let color = if c.u8()? == 0 {
            None
        } else {
            Some(c.take(3)?.try_into().unwrap())
        };
        Some(CueAhead {
            slot,
            seconds,
            color,
        })
    };
    Ok(MusicState {
        time_s,
        playhead_s,
        playing: flags & 1 != 0,
        bpm,
        bpm_grid,
        beat_phase,
        bar_phase,
        phrase_phase,
        phrase,
        next_phrase,
        beats_to_next_phrase,
        drop_countdown_beats,
        intensity,
        audio,
        track: track.cloned(),
        beat_index,
        mood,
        next_cue,
        theme,
        analysis: BandsAt {
            low,
            mid,
            high,
            vocal,
        },
        structure_source,
        hits,
    })
}

/// Reads states back from a byte stream, in order. A recording cut short (the process
/// was killed mid-write) yields every whole state and then `Err(Truncated)` once.
pub struct Decoder<'a> {
    bytes: &'a [u8],
    pos: usize,
    track: Option<TrackMeta>,
    done: bool,
}

impl<'a> Decoder<'a> {
    pub fn new(bytes: &'a [u8]) -> Result<Self, DecodeError> {
        if bytes.len() < 12 || bytes[..8] != MAGIC {
            return Err(DecodeError::BadMagic);
        }
        let version = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        if version > VERSION {
            return Err(DecodeError::Version(version));
        }
        Ok(Self {
            bytes,
            pos: 12,
            track: None,
            done: false,
        })
    }

    fn next_record(&mut self) -> Result<Option<MusicState>, DecodeError> {
        loop {
            if self.pos >= self.bytes.len() {
                return Ok(None);
            }
            let mut head = Cursor {
                bytes: self.bytes,
                pos: self.pos,
            };
            let len = head.u32()? as usize;
            let body = head.take(len)?;
            self.pos = head.pos;
            let mut c = Cursor {
                bytes: body,
                pos: 0,
            };
            match c.u8()? {
                KIND_TRACK => self.track = read_track(&mut c)?,
                KIND_STATE => return read_state(&mut c, self.track.as_ref()).map(Some),
                other => return Err(DecodeError::Kind(other)),
            }
        }
    }
}

impl Iterator for Decoder<'_> {
    type Item = Result<MusicState, DecodeError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        match self.next_record() {
            Ok(Some(ms)) => Some(Ok(ms)),
            Ok(None) => {
                self.done = true;
                None
            }
            Err(e) => {
                self.done = true;
                Some(Err(e))
            }
        }
    }
}

/// Decodes a whole recording; stops at a truncated tail without failing.
pub fn decode_all(bytes: &[u8]) -> Result<(Vec<MusicState>, bool), DecodeError> {
    let mut states = Vec::new();
    let mut truncated = false;
    for r in Decoder::new(bytes)? {
        match r {
            Ok(ms) => states.push(ms),
            Err(DecodeError::Truncated) => truncated = true,
            Err(e) => return Err(e),
        }
    }
    Ok((states, truncated))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn busy_state(t: f64) -> MusicState {
        let mut audio = AudioFeatures::silent();
        audio.silent = false;
        audio.onset = true;
        for (i, b) in audio.bands.iter_mut().enumerate() {
            *b = i as f32 * 0.01;
        }
        audio.levels = audio.bands;
        audio.groups = [0.1, 0.2, 0.3, 0.4, 0.5, 0.6];
        audio.rms = 0.33;
        audio.kick = 0.9;
        audio.snare = 0.5;
        audio.hat = 0.25;
        audio.loudness = 0.7;
        audio.brightness = 0.6;
        MusicState {
            time_s: t,
            playhead_s: t + 60.0,
            playing: true,
            bpm: 127.5,
            bpm_grid: 128.0,
            beat_phase: 0.25,
            bar_phase: 0.0625,
            phrase_phase: 0.5,
            phrase: Some(PhraseKind::Up),
            next_phrase: Some(PhraseKind::Chorus),
            beats_to_next_phrase: Some(8),
            drop_countdown_beats: Some(8),
            intensity: 0.8,
            audio,
            track: Some(TrackMeta {
                id: TrackId(7),
                title: "Stardust — ünïcödé".into(),
                artist: "Someone".into(),
                album: "LP".into(),
                year: Some(2021),
                key: Some("9A".into()),
                bpm: Some(128.0),
                duration_s: Some(300.5),
                sample_rate: Some(44_100),
                file_path: Some("C:/music/private.flac".into()),
                artwork_path: Some("C:/music/art.jpg".into()),
                analysis_path: Some("/PIONEER/x.DAT".into()),
                isrc: Some("ISRC".into()),
                genre: Some("Techno".into()),
                extra: TrackExtra {
                    comment: Some("private".into()),
                    ..TrackExtra::default()
                },
            }),
            beat_index: Some(640),
            mood: Some(Mood::High),
            next_cue: Some(CueAhead {
                slot: 3,
                seconds: 4.5,
                color: Some([255, 0, 128]),
            }),
            theme: [[0.1; 3], [0.9; 3], [0.2, 0.6, 1.0], [1.0, 0.4, 0.2], [0.6, 0.2, 0.9]],
            analysis: BandsAt {
                low: 0.8,
                mid: 0.5,
                high: 0.3,
                vocal: 0.1,
            },
            structure_source: Some(StructureSource::Prepared),
            hits: BeatHits {
                kick: 0.95,
                snare: 0.0,
                audio_kick: 0.7,
                audio_snare: 0.1,
                kick_confidence: 0.8,
                snare_confidence: 0.6,
                lag_s: 0.07,
                lag_samples: 40,
                locked: true,
            },
        }
    }

    fn encode(states: &[MusicState]) -> Vec<u8> {
        let mut out = header().to_vec();
        let mut enc = Encoder::new();
        for ms in states {
            enc.push(ms, &mut out);
        }
        out
    }

    #[test]
    fn round_trips_everything_the_uniforms_read_and_drops_private_paths() {
        let original = busy_state(1.5);
        let bytes = encode(&[original.clone(), MusicState::default()]);
        let (states, truncated) = decode_all(&bytes).unwrap();
        assert!(!truncated);
        assert_eq!(states.len(), 2);
        let mut expected = original;
        let t = expected.track.as_mut().unwrap();
        t.file_path = None;
        t.artwork_path = None;
        t.analysis_path = None;
        t.isrc = None;
        t.extra = TrackExtra::default();
        assert_eq!(states[0], expected);
        assert_eq!(states[1], MusicState::default());
        assert!(
            !bytes.windows(7).any(|w| w == b"private"),
            "paths and comments never reach the file"
        );
    }

    #[test]
    fn the_track_is_written_once_per_change() {
        let a = busy_state(0.0);
        let mut b = busy_state(0.01);
        b.track = None;
        let same_track = encode(&[a.clone(), a.clone(), a.clone()]);
        let changing = encode(&[a.clone(), b.clone(), a.clone()]);
        assert!(changing.len() > same_track.len());
        let (states, _) = decode_all(&changing).unwrap();
        assert_eq!(states[0].track, states[2].track);
        assert!(states[1].track.is_none());
        // A frame is a few hundred bytes: minutes at 120 Hz stay in the tens of megabytes.
        let one = same_track.len() - encode(&[a.clone(), a.clone()]).len();
        assert!(one < 500, "{one} bytes per state");
    }

    #[test]
    fn a_cut_short_recording_keeps_the_whole_states_before_the_cut() {
        let bytes = encode(&[busy_state(0.0), busy_state(0.01), busy_state(0.02)]);
        let cut = &bytes[..bytes.len() - 10];
        let (states, truncated) = decode_all(cut).unwrap();
        assert_eq!(states.len(), 2);
        assert!(truncated);
        assert!((states[1].time_s - 0.01).abs() < f64::EPSILON);
    }

    #[test]
    fn rejects_other_files_and_newer_versions() {
        assert_eq!(
            Decoder::new(b"PNG\r\n\x1a\n000000").err(),
            Some(DecodeError::BadMagic)
        );
        let mut h = header().to_vec();
        h[8..12].copy_from_slice(&(VERSION + 1).to_le_bytes());
        assert_eq!(
            Decoder::new(&h).err(),
            Some(DecodeError::Version(VERSION + 1))
        );
        assert!(Decoder::new(&header()).unwrap().next().is_none());
    }
}
