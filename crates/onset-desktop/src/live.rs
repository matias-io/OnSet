//! The live layer for the screens: OnSet's engine follows rekordbox's master deck (with the
//! structure prepared in OnSet), and its state goes to the webviews as `onset:live` events
//! about 30 times a second. Read-only: nothing here touches rekordbox or the workspace.
use std::path::PathBuf;
use std::time::Duration;

use onset_core::music_state::MusicState;
use onset_core::phrase::PhraseKind;
use onset_engine::{Engine, EngineConfig, EngineStatus};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

const PERIOD: Duration = Duration::from_millis(33);

#[derive(Debug, Clone, Serialize)]
pub struct LiveTrack {
    pub title: String,
    pub artist: String,
    pub key: Option<String>,
    pub bpm: Option<f32>,
    pub rekordbox_id: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct LiveCue {
    /// 0 = memory cue, 1..=8 = hot cue A..H.
    pub slot: u8,
    pub seconds: f32,
}

/// One `onset:live` event.
#[derive(Debug, Clone, Serialize)]
pub struct LiveState {
    /// `waiting`, `idle`, `unidentified`, `running` or `error`.
    pub state: &'static str,
    /// What the engine says, in a line ("waiting for rekordbox", "rekordbox: Artist - Title").
    pub status: String,
    pub track: Option<LiveTrack>,
    pub playing: bool,
    pub playhead_s: f64,
    /// Tempo being played (follows the pitch fader).
    pub bpm: f32,
    pub beat_phase: f32,
    pub bar_phase: f32,
    pub section: Option<&'static str>,
    pub next_section: Option<&'static str>,
    pub beats_to_next_section: Option<u32>,
    pub drop_countdown_beats: Option<u32>,
    /// `Prepared` when the DJ prepared the track, else `rekordbox`.
    pub structure_source: Option<&'static str>,
    pub next_cue: Option<LiveCue>,
}

/// Section names in the service's words: the engine carries the prepared structure in rekordbox's
/// High-mood vocabulary (a drop is a Chorus, a build an Up, a breakdown a Down).
fn section_name(kind: PhraseKind) -> &'static str {
    match kind {
        PhraseKind::Intro => "Intro",
        PhraseKind::Verse => "Groove",
        PhraseKind::Bridge => "Stripped",
        PhraseKind::Chorus => "Drop",
        PhraseKind::Up => "Build",
        PhraseKind::Down => "Breakdown",
        PhraseKind::Outro => "Outro",
    }
}

fn snapshot(status: &EngineStatus, ms: &MusicState) -> LiveState {
    let state = match status {
        EngineStatus::Starting | EngineStatus::Waiting(_) => "waiting",
        EngineStatus::Idle => "idle",
        EngineStatus::Unidentified { .. } => "unidentified",
        EngineStatus::Running { .. } => "running",
        EngineStatus::Error(_) => "error",
    };
    LiveState {
        state,
        status: status.line(),
        track: ms.track.as_ref().map(|t| LiveTrack {
            title: t.title.clone(),
            artist: t.artist.clone(),
            key: t.key.clone(),
            bpm: t.bpm,
            rekordbox_id: t.id.0,
        }),
        playing: ms.playing,
        playhead_s: ms.playhead_s,
        bpm: ms.bpm,
        beat_phase: ms.beat_phase,
        bar_phase: ms.bar_phase,
        section: ms.phrase.map(section_name),
        next_section: ms.next_phrase.map(section_name),
        beats_to_next_section: ms.beats_to_next_phrase,
        drop_countdown_beats: ms.drop_countdown_beats,
        structure_source: ms
            .structure_source
            .map(onset_core::structure::StructureSource::name),
        next_cue: ms.next_cue.map(|c| LiveCue {
            slot: c.slot,
            seconds: c.seconds,
        }),
    }
}

/// `ONSET_OFFSETS`, else `offsets/` beside the executable, else the repository's `offsets/`.
fn offsets_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("ONSET_OFFSETS") {
        return PathBuf::from(dir);
    }
    if let Some(beside) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|d| d.join("offsets")))
        .filter(|d| d.is_dir())
    {
        return beside;
    }
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../offsets"))
}

fn cache_dir() -> PathBuf {
    directories::ProjectDirs::from("", "Onset", "Onset")
        .map_or_else(|| PathBuf::from(".cache"), |d| d.cache_dir().to_path_buf())
}

/// Starts the engine and the publishing thread. `ONSET_SIM=<title>` follows the developer
/// simulator, silent, instead of rekordbox.
pub fn spawn(app: AppHandle) {
    let spawned = std::thread::Builder::new()
        .name("onset-live".into())
        .spawn(move || {
            // Loading the rekordbox library takes a few seconds; say so meanwhile.
            let _ = app.emit(
                "onset:live",
                snapshot(
                    &EngineStatus::Waiting("loading the rekordbox library".into()),
                    &MusicState::default(),
                ),
            );
            let sim_track = std::env::var("ONSET_SIM").ok().filter(|s| !s.is_empty());
            let engine = Engine::start(EngineConfig {
                app_dir: None,
                cache_dir: cache_dir(),
                offsets_dir: offsets_dir(),
                sim_track,
                sim_seek_s: 0.0,
                sim_gain: 0.0,
                audio_device: None,
                // The screens need the deck and structure, not the room's sound.
                capture_audio: false,
            });
            let engine = match engine {
                Ok(e) => e,
                Err(e) => {
                    tracing::error!("live engine unavailable: {e:#}");
                    let failed = snapshot(
                        &EngineStatus::Error(format!("{e:#}")),
                        &MusicState::default(),
                    );
                    loop {
                        let _ = app.emit("onset:live", &failed);
                        std::thread::sleep(Duration::from_secs(1));
                    }
                }
            };
            loop {
                let state = snapshot(&engine.status(), &engine.state());
                let _ = app.emit("onset:live", &state);
                std::thread::sleep(PERIOD);
            }
        });
    if let Err(e) = spawned {
        tracing::error!("cannot start the live thread: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_waiting_engine_reports_no_track() {
        let s = snapshot(
            &EngineStatus::Waiting("waiting for rekordbox".into()),
            &MusicState::default(),
        );
        assert_eq!(s.state, "waiting");
        assert_eq!(s.status, "waiting for rekordbox");
        assert!(s.track.is_none());
        assert!(s.section.is_none());
    }

    #[test]
    fn sections_use_the_prepared_words() {
        let ms = MusicState {
            phrase: Some(PhraseKind::Up),
            next_phrase: Some(PhraseKind::Chorus),
            drop_countdown_beats: Some(16),
            structure_source: Some(onset_core::structure::StructureSource::Prepared),
            ..MusicState::default()
        };
        let s = snapshot(
            &EngineStatus::Running {
                source: "rekordbox".into(),
                track: "x".into(),
            },
            &ms,
        );
        assert_eq!(
            (s.section, s.next_section, s.drop_countdown_beats),
            (Some("Build"), Some("Drop"), Some(16))
        );
        assert_eq!(s.structure_source, Some("Prepared"));
    }
}
