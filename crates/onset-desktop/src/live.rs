//! The live layer for the screens: OnSet's engine follows rekordbox's master deck (with the
//! structure prepared in OnSet), and its state goes to the webviews as `onset:live` events.
//! An event goes out when something the screens show changes, and while a track plays at
//! least every half second so they can keep the beat from the tempo and phase in between.
//! Nothing is sent while every window is hidden or minimised. Read-only: nothing here
//! touches rekordbox or the workspace.
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use onset_core::music_state::MusicState;
use onset_core::phrase::PhraseKind;
use onset_engine::{Engine, EngineCommand, EngineConfig, EngineStatus};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

/// How often the engine's state is looked at.
const PERIOD: Duration = Duration::from_millis(33);
/// While playing, the screens get the tempo and phase at least this often.
const RESYNC: Duration = Duration::from_millis(500);

/// Set after a calibration: the engine reloads the offsets and finds the decks again.
static RECONNECT: AtomicBool = AtomicBool::new(false);

/// Asks the live engine to reload the rekordbox calibration (it reads it only on connecting).
pub fn reconnect() {
    RECONNECT.store(true, Ordering::Release);
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LiveTrack {
    pub title: String,
    pub artist: String,
    pub key: Option<String>,
    pub bpm: Option<f32>,
    pub rekordbox_id: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LiveCue {
    /// 0 = memory cue, 1..=8 = hot cue A..H.
    pub slot: u8,
    pub seconds: f32,
}

/// One `onset:live` event.
#[derive(Debug, Clone, PartialEq, Serialize)]
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

/// Whether the screens would show something different: everything except the playhead, the
/// beat and bar phase and the cue countdown, which they carry forward from the tempo
/// themselves. A cue is compared by where it is in the track, to a tenth of a second.
fn shows_differently(a: &LiveState, b: &LiveState) -> bool {
    let timeless = |s: &LiveState| LiveState {
        playhead_s: 0.0,
        beat_phase: 0.0,
        bar_phase: 0.0,
        next_cue: s.next_cue.as_ref().map(|c| LiveCue {
            slot: c.slot,
            #[allow(clippy::cast_possible_truncation)] // a track position in tenths
            seconds: ((s.playhead_s + f64::from(c.seconds)) * 10.0).round() as f32,
        }),
        ..s.clone()
    };
    timeless(a) != timeless(b)
}

/// Decides when the next event goes out.
#[derive(Default)]
struct Pacer {
    last: Option<(LiveState, Instant)>,
}

impl Pacer {
    fn due(&mut self, state: &LiveState, now: Instant) -> bool {
        let due = match &self.last {
            None => true,
            Some((last, at)) => {
                shows_differently(last, state)
                    || (state.playing && now.duration_since(*at) >= RESYNC)
            }
        };
        if due {
            self.last = Some((state.clone(), now));
        }
        due
    }

    /// After a pause in sending, the next state goes out whatever it is.
    fn forget(&mut self) {
        self.last = None;
    }
}

/// Sending stops only after every window has been hidden or minimised this long, so a
/// window state that flickers (while being dragged or restored) can't force a resend on
/// every tick.
const HIDDEN_GRACE: Duration = Duration::from_secs(1);

#[derive(Default)]
struct Watched {
    hidden_since: Option<Instant>,
}

impl Watched {
    /// Whether to keep sending, given whether a window shows the live state right now.
    fn update(&mut self, visible: bool, now: Instant) -> bool {
        if visible {
            self.hidden_since = None;
            return true;
        }
        let since = *self.hidden_since.get_or_insert(now);
        now.duration_since(since) < HIDDEN_GRACE
    }
}

/// Whether any window could show the live state.
fn anyone_watching(app: &AppHandle) -> bool {
    app.webview_windows()
        .values()
        .any(|w| w.is_visible().unwrap_or(true) && !w.is_minimized().unwrap_or(false))
}

/// `ONSET_OFFSETS`, else `offsets/` beside the executable, else the repository's `offsets/`.
/// The live view reads calibrations here first, and the calibration panel saves them here.
pub fn offsets_dir() -> PathBuf {
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
            let mut pacer = Pacer::default();
            let mut watched = Watched::default();
            loop {
                if RECONNECT.swap(false, Ordering::AcqRel) {
                    engine.command(EngineCommand::Reconnect);
                }
                let now = Instant::now();
                if watched.update(anyone_watching(&app), now) {
                    let state = snapshot(&engine.status(), &engine.state());
                    if pacer.due(&state, now) {
                        let _ = app.emit("onset:live", &state);
                    }
                } else {
                    pacer.forget();
                }
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

    fn playing(bar_phase: f32) -> LiveState {
        let mut s = snapshot(&EngineStatus::Idle, &MusicState::default());
        s.playing = true;
        s.bpm = 128.0;
        s.bar_phase = bar_phase;
        s.playhead_s = f64::from(bar_phase);
        s
    }

    #[test]
    fn sending_stops_only_after_a_second_hidden() {
        let mut w = Watched::default();
        let t0 = Instant::now();
        assert!(w.update(true, t0));
        assert!(w.update(false, t0), "just hidden");
        assert!(w.update(true, t0 + PERIOD), "a flicker changes nothing");
        assert!(w.update(false, t0 + PERIOD * 2));
        assert!(w.update(false, t0 + PERIOD * 2 + HIDDEN_GRACE / 2));
        assert!(
            !w.update(false, t0 + PERIOD * 2 + HIDDEN_GRACE),
            "hidden a second"
        );
        assert!(w.update(true, t0 + HIDDEN_GRACE * 3), "shown again");
    }

    #[test]
    fn only_visible_changes_and_resyncs_are_sent() {
        let mut pacer = Pacer::default();
        let t0 = Instant::now();
        assert!(
            pacer.due(&playing(0.0), t0),
            "the first state always goes out"
        );
        assert!(
            !pacer.due(&playing(0.1), t0 + Duration::from_millis(33)),
            "phase moved only"
        );
        let mut cue = playing(0.1);
        cue.next_cue = Some(LiveCue {
            slot: 1,
            seconds: 10.0,
        });
        assert!(
            pacer.due(&cue, t0 + Duration::from_millis(66)),
            "a cue appeared"
        );
        let mut closer = cue.clone();
        closer.playhead_s += 0.5;
        closer.next_cue = Some(LiveCue {
            slot: 1,
            seconds: 9.5,
        });
        assert!(
            !pacer.due(&closer, t0 + Duration::from_millis(99)),
            "same cue, nearer"
        );
        assert!(
            pacer.due(&playing(0.2), t0 + RESYNC),
            "resync while playing"
        );
        let mut later = playing(0.3);
        later.section = Some("Drop");
        assert!(
            pacer.due(&later, t0 + RESYNC + Duration::from_millis(33)),
            "section changed"
        );
        let mut paused = later.clone();
        paused.playing = false;
        assert!(
            pacer.due(&paused, t0 + RESYNC + Duration::from_millis(66)),
            "paused"
        );
        assert!(
            !pacer.due(&paused, t0 + RESYNC * 4),
            "paused and unchanged: no resync"
        );
        pacer.forget();
        assert!(
            pacer.due(&paused, t0 + RESYNC * 4),
            "after hiding, the state goes out again"
        );
    }

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
