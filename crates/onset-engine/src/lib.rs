//! The engine thread: polls the transport, drives the clock, structure, analyzer and
//! director at 120 Hz, and publishes one `MusicState` for the renderer to read each frame.
//!
//! The transport is rekordbox's memory by default. The simulator is a developer tool: it
//! only runs when asked for explicitly, and Onset never plays music on its own otherwise.
pub mod deck_ear;
pub mod priority;
pub mod recorder;
pub mod stream_match;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use onset_audio::analyzer::Analyzer;
use onset_audio::capture::LoopbackCapture;
use onset_core::audio_features::AudioFeatures;
use onset_core::clock::Clock;
use onset_core::director::Director;
use onset_core::music_state::MusicState;
use onset_core::structure::{Structure, StructureSource};
use onset_core::track::{HotCue, TrackMeta};
use onset_core::transport::{TrackRef, TransportSnapshot};
use onset_rekordbox::anlz::Analysis;
use onset_rekordbox::library::Library;
use onset_rekordbox::paths::RekordboxPaths;
use onset_transport::blend::{BlendDeck, BlendPlayer, BlendScript};
use onset_transport::sim::SimPlayer;
use onset_transport::source::{SourceStatus, TransportSource};

const TICK: Duration = Duration::from_micros(1_000_000 / 120);

/// Seconds the captured audio is taken to trail a deck's playhead until the hit lock has
/// measured it.
const EAR_LAG_S: f64 = 0.1;
/// Hit-lock measurements needed before its lag is trusted for the ear.
const EAR_LAG_SAMPLES: u32 = 20;
/// Seconds a playing deck's playhead may move other than with time before the ear forgets
/// what it heard (the waveform it compares against is no longer where it was).
const EAR_JUMP_S: f64 = 0.5;
/// Tracks whose waveforms are kept for the ear: a set's decks, with room for reloads.
const EAR_CACHE: usize = 16;
/// The two-deck simulator: seconds of the first track alone before the blend, and the
/// blend's length in bars.
const BLEND_LEAD_S: f64 = 30.0;
const BLEND_BARS: u32 = 16;
/// The furthest the simulator nudges the second track's tempo to match the first, as a
/// fraction; further apart, both play at their own tempo.
const BLEND_MAX_NUDGE: f64 = 0.08;

/// The ear listening to every deck with a known track (see [`deck_ear`]).
struct Ear {
    ear: deck_ear::DeckEar,
    /// The decks and tracks it was built for, in deck order; a change rebuilds it.
    tracks: Vec<(u8, TrackRef)>,
    /// Each playing deck's playhead at the last listen, and when: `(deck, playhead_s, time_s)`.
    heads: Vec<(u8, f64, f64)>,
}

pub struct EngineConfig {
    /// rekordbox data folder; `None` discovers `%APPDATA%\Pioneer\rekordbox`.
    pub app_dir: Option<PathBuf>,
    pub cache_dir: PathBuf,
    /// Where `<version>.toml` pointer-chain files live for the memory transport.
    pub offsets_dir: PathBuf,
    /// Developer mode: play this library title through the simulator instead of reading
    /// rekordbox. `None` is the normal case.
    pub sim_track: Option<String>,
    /// Developer mode, with `sim_track`: a second library title the simulator blends into
    /// on a second deck (`sim_track` alone, a 16-bar crossfade, then this one alone), so the
    /// deck-by-ear can be watched without rekordbox.
    pub sim_blend: Option<String>,
    pub sim_seek_s: f64,
    pub sim_gain: f32,
    /// Loopback endpoint substring; default output device when `None`.
    pub audio_device: Option<String>,
    /// Capture the system mix (rekordbox's output) for the analyzer. With the simulator this
    /// is normally off (it taps its own audio); on, the simulator's output comes back
    /// through the capture, so the whole audio path and its lag are exercised without a
    /// deck.
    pub capture_audio: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EngineCommand {
    Pause,
    Resume,
    Seek(f64),
    SetRate(f32),
    LoadTrack(String),
    /// Capture this device (name fragment), or pick automatically with `None`; applies now.
    SetAudioDevice(Option<String>),
    /// Listen to the room's sound, or stop: the app's live view needs only the decks, its
    /// projector the audio as well.
    SetCapture(bool),
    /// Look up and cache the lyrics of every track in the collection, in the background.
    PrefetchLyrics,
    /// Reload the rekordbox calibration and find the decks again, after a new calibration.
    Reconnect,
    /// Write every tick's `MusicState` to this file (`onset_core::recording`), replacing
    /// any recording in progress, until `StopRecording`.
    StartRecording(PathBuf),
    StopRecording,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EngineStatus {
    Starting,
    /// No usable source yet; the text says what is missing ("waiting for rekordbox").
    Waiting(String),
    /// A source is connected but no track is loaded on the master deck.
    Idle,
    /// A deck is playing but its track could not be identified: the show runs on audio
    /// and playhead alone, without the analysed structure.
    Unidentified {
        source: String,
    },
    Running {
        source: String,
        track: String,
    },
    Error(String),
}

impl EngineStatus {
    /// One line for the HUD.
    pub fn line(&self) -> String {
        match self {
            Self::Starting => "starting".to_string(),
            Self::Waiting(why) => why.clone(),
            Self::Idle => "connected, no track".to_string(),
            Self::Unidentified { source } => format!("{source}: unidentified track (audio only)"),
            Self::Running { source, track } => format!("{source}: {track}"),
            Self::Error(e) => format!("error: {e}"),
        }
    }
}

/// One library row as the settings panel lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackEntry {
    /// What `EngineCommand::LoadTrack` takes.
    pub title: String,
    /// "Artist — Title", what the panel shows.
    pub label: String,
}

/// "Artist — Title", or just the title when rekordbox has no artist for it.
pub fn track_label(artist: &str, title: &str) -> String {
    if artist.trim().is_empty() {
        title.to_string()
    } else {
        format!("{artist} — {title}")
    }
}

pub struct Engine {
    state: Arc<ArcSwap<MusicState>>,
    status: Arc<ArcSwap<EngineStatus>>,
    commands: crossbeam_channel::Sender<EngineCommand>,
    tracks: Vec<TrackEntry>,
    simulator: bool,
    /// The audio device being captured, for the launcher.
    audio_device: Arc<ArcSwap<String>>,
    /// Progress of the whole-library lyrics look-up, empty when none has run.
    lyrics_progress: Arc<ArcSwap<String>>,
    /// The clock `MusicState::time_s` counts from.
    started: Instant,
    _thread: std::thread::JoinHandle<()>,
}

/// Looks up the lyrics of every track (cached ones are instant) and reports progress.
fn prefetch_lyrics(tracks: &[TrackMeta], cache_dir: &std::path::Path, progress: &ArcSwap<String>) {
    use onset_lyrics::{LyricsStore, Outcome, Query};
    let store = LyricsStore::new(&cache_dir.join("lyrics"));
    let total = tracks.len();
    let mut synced = 0;
    for (i, t) in tracks.iter().enumerate() {
        let q = Query::from_meta(t);
        let cached = store.cached(&q).is_some();
        if matches!(store.lookup(&q, true), Outcome::Synced(_)) {
            synced += 1;
        }
        progress.store(Arc::new(format!(
            "Checked {} of {total} tracks; {synced} have synced lyrics",
            i + 1
        )));
        if !cached {
            std::thread::sleep(Duration::from_millis(300)); // gentle on the free service
        }
    }
    progress.store(Arc::new(format!(
        "Done: {synced} of {total} tracks have synced lyrics (the rest are instrumentals, samples or not in LRCLIB)"
    )));
}

/// The track the master deck holds, with everything the structure engine needs.
struct Current {
    meta: TrackMeta,
    analysis: Option<Analysis>,
    cues: Vec<HotCue>,
    /// Whose phrases `analysis` carries: the prepared structure when the DJ prepared the track.
    structure_source: StructureSource,
    /// How the source referred to it, so a repeat read is not a reload.
    track_ref: TrackRef,
    /// The deck it plays on (live only), so a switch to an unknown track clears it.
    deck: Option<u8>,
}

struct SimSource {
    player: SimPlayer,
    tap: crossbeam_channel::Receiver<Vec<f32>>,
    analyzer: Analyzer,
}

/// The two-deck simulator, heard through its own tap like [`SimSource`].
struct BlendSource {
    player: BlendPlayer,
    tap: crossbeam_channel::Receiver<Vec<f32>>,
    analyzer: Analyzer,
}

enum Source {
    Sim(Box<SimSource>),
    Blend(Box<BlendSource>),
    Live(Box<dyn TransportSource>),
}

impl Source {
    fn name(&self) -> &'static str {
        match self {
            Self::Sim(_) | Self::Blend(_) => "simulator",
            Self::Live(s) => s.name(),
        }
    }

    fn status(&self) -> SourceStatus {
        match self {
            Self::Sim(_) | Self::Blend(_) => SourceStatus::Connected,
            Self::Live(s) => s.status(),
        }
    }

    fn poll(&mut self) -> Option<TransportSnapshot> {
        match self {
            Self::Sim(s) => s.player.poll(),
            Self::Blend(s) => s.player.poll(),
            Self::Live(s) => s.poll(),
        }
    }
}

struct Capture {
    stream: LoopbackCapture,
    rx: crossbeam_channel::Receiver<Vec<f32>>,
    analyzer: Analyzer,
}

/// How often the capture checks that it still has the right device.
const AUDIO_CHECK: Duration = Duration::from_secs(3);

/// Asks every few seconds, off the engine thread (listing devices takes tens of
/// milliseconds), which device the capture should use for the current choice.
fn spawn_audio_watch(
    choice: Arc<parking_lot::Mutex<Option<String>>>,
) -> crossbeam_channel::Receiver<Option<String>> {
    let (tx, rx) = crossbeam_channel::bounded(1);
    let spawned = std::thread::Builder::new()
        .name("onset-audio-watch".into())
        .spawn(move || {
            loop {
                let wanted = choice.lock().clone();
                let name = onset_audio::capture::preferred_endpoint(wanted.as_deref());
                if tx.send(name).is_err() {
                    break;
                }
                std::thread::sleep(AUDIO_CHECK);
            }
        });
    if let Err(e) = spawned {
        tracing::warn!(error = %e, "cannot start the audio device watch");
    }
    rx
}

fn open_capture(choice: Option<&str>) -> Option<Capture> {
    match LoopbackCapture::start(choice) {
        Ok((stream, rx, rate)) => Some(Capture {
            stream,
            rx,
            analyzer: Analyzer::new(rate),
        }),
        Err(e) => {
            tracing::warn!("audio capture unavailable: {e:#}");
            None
        }
    }
}

/// Finds the library row a transport reference points at.
pub fn resolve_track(library: &Library, r: &TrackRef) -> Option<TrackMeta> {
    match r {
        TrackRef::Unknown => None,
        TrackRef::Id(id) => library.by_id(*id).cloned(),
        TrackRef::AnalysisPath(path) => library.by_analysis_path(path).cloned(),
        TrackRef::TitleArtist { title, artist, .. } => library
            .find_by_title_artist(title, artist)
            .or_else(|| library.find_by_title_artist(title, ""))
            .cloned(),
        TrackRef::FilePath(path) => library.by_file_path(path).cloned(),
    }
}

fn load_current(
    library: &Library,
    paths: &RekordboxPaths,
    workspace: Option<&onset_workspace::Workspace>,
    meta: TrackMeta,
    track_ref: TrackRef,
) -> Current {
    let mut analysis = meta.analysis_path.as_deref().and_then(|rel| {
        onset_rekordbox::anlz::load_analysis(paths, rel)
            .map_err(|e| tracing::warn!(title = %meta.title, "analysis unavailable: {e}"))
            .ok()
    });
    let mut cues = library.cues(meta.id);
    let mut structure_source = StructureSource::Rekordbox;

    // The structure the DJ prepared replaces rekordbox's phrases, placed on
    // rekordbox's own beat grid; approved cues join rekordbox's.
    let prepared = workspace.and_then(|ws| {
        ws.prepared(Some(meta.id.0), meta.file_path.as_deref())
            .map_err(|e| tracing::warn!(title = %meta.title, "workspace: {e}"))
            .ok()
            .flatten()
    });
    if let (Some(p), Some(a)) = (&prepared, analysis.as_mut())
        && let Some(map) = onset_workspace::phrase_map(p, &a.grid)
    {
        tracing::info!(
            title = %meta.title,
            sections = map.phrases.len(),
            approved_cues = p.approved_cues.len(),
            "prepared structure"
        );
        a.phrases = Some(map);
        cues = onset_workspace::merged_cues(&cues, &p.approved_cues);
        structure_source = StructureSource::Prepared;
    }
    Current {
        meta,
        analysis,
        cues,
        structure_source,
        track_ref,
        deck: None,
    }
}

/// Library tracks rekordbox streams (TIDAL, Beatport...) rather than reads from a file,
/// with their 3-band waveforms, for [`crate::stream_match::StreamMatcher`].
fn streaming_candidates(
    library: &Library,
    paths: &RekordboxPaths,
) -> Vec<crate::stream_match::Candidate> {
    library
        .tracks()
        .iter()
        .filter(|t| {
            t.file_path.as_ref().is_some_and(|p| {
                let s = p.to_string_lossy();
                // A service URI such as "tidal:tracks:123" rather than a drive path.
                s.find(':').is_some_and(|i| i > 1) && !s.contains('\\')
            })
        })
        .filter_map(|t| {
            let rel = t.analysis_path.as_deref()?;
            let bytes = std::fs::read(paths.resolve_share(rel).with_extension("2EX")).ok()?;
            let bands = onset_rekordbox::anlz::parse_2ex(&bytes)?;
            (!bands.frames.is_empty()).then(|| crate::stream_match::Candidate {
                meta: t.clone(),
                bands: Arc::new(bands),
            })
        })
        .collect()
}

/// The library track titled `title` and its audio file, for the simulator.
fn sim_file(library: &Library, title: &str) -> anyhow::Result<(TrackMeta, PathBuf)> {
    let meta = library
        .find_by_title_artist(title, "")
        .ok_or_else(|| anyhow::anyhow!("no track titled {title:?}"))?
        .clone();
    let file = meta
        .file_path
        .clone()
        .filter(|p| p.exists())
        .ok_or_else(|| anyhow::anyhow!("audio file for {title:?} not found"))?;
    Ok((meta, file))
}

fn start_sim(
    library: &Library,
    paths: &RekordboxPaths,
    workspace: Option<&onset_workspace::Workspace>,
    title: &str,
    cfg: &EngineConfig,
) -> anyhow::Result<(SimSource, Current)> {
    let (meta, file) = sim_file(library, title)?;
    let player = SimPlayer::start(&file, TrackRef::Id(meta.id), meta.bpm.unwrap_or(120.0))?;
    player.set_gain(cfg.sim_gain);
    player.seek(cfg.sim_seek_s);
    let tap = player.tap_audio();
    let analyzer = Analyzer::new(player.tap_sample_rate());
    let track_ref = TrackRef::Id(meta.id);
    let current = load_current(library, paths, workspace, meta, track_ref);
    Ok((
        SimSource {
            player,
            tap,
            analyzer,
        },
        current,
    ))
}

/// The two-deck simulator: `title` alone from `cfg.sim_seek_s`, a [`BLEND_BARS`]-bar
/// crossfade into `blend` (from its start, nudged to the first track's tempo when close),
/// then `blend` alone while the first track runs on, faded out.
fn start_blend(
    library: &Library,
    paths: &RekordboxPaths,
    workspace: Option<&onset_workspace::Workspace>,
    (title, blend): (&str, &str),
    cfg: &EngineConfig,
) -> anyhow::Result<(BlendSource, Current)> {
    let (first, first_file) = sim_file(library, title)?;
    let (second, second_file) = sim_file(library, blend)?;
    let bpm = |m: &TrackMeta| m.bpm.filter(|b| *b > 0.0).unwrap_or(120.0);
    let (bpm_first, bpm_second) = (bpm(&first), bpm(&second));
    let nudge = f64::from(bpm_first) / f64::from(bpm_second);
    let decks = [
        BlendDeck {
            audio: Arc::new(onset_transport::decode::decode_file(&first_file)?),
            track: TrackRef::Id(first.id),
            bpm: bpm_first,
            cue_s: cfg.sim_seek_s,
            rate: 1.0,
        },
        BlendDeck {
            audio: Arc::new(onset_transport::decode::decode_file(&second_file)?),
            track: TrackRef::Id(second.id),
            bpm: bpm_second,
            cue_s: 0.0,
            rate: if (nudge - 1.0).abs() <= BLEND_MAX_NUDGE {
                nudge
            } else {
                1.0
            },
        },
    ];
    let player = BlendPlayer::start(
        decks,
        BlendScript::bars(BLEND_LEAD_S, BLEND_BARS, bpm_first),
    )?;
    player.set_gain(cfg.sim_gain);
    let tap = player.tap_audio();
    let analyzer = Analyzer::new(player.tap_sample_rate());
    let track_ref = TrackRef::Id(first.id);
    let current = load_current(library, paths, workspace, first, track_ref);
    Ok((
        BlendSource {
            player,
            tap,
            analyzer,
        },
        current,
    ))
}

/// What the engine follows: the two-deck simulator, the simulator, or rekordbox, with the
/// simulator's track already loaded.
fn start_source(
    library: &Library,
    paths: &RekordboxPaths,
    workspace: Option<&onset_workspace::Workspace>,
    cfg: &EngineConfig,
) -> anyhow::Result<(Source, Option<Current>)> {
    Ok(match (&cfg.sim_track, &cfg.sim_blend) {
        (Some(title), Some(blend)) => {
            let titles = (title.as_str(), blend.as_str());
            let (sim, current) = start_blend(library, paths, workspace, titles, cfg)?;
            (Source::Blend(Box::new(sim)), Some(current))
        }
        (Some(title), None) => {
            let (sim, current) = start_sim(library, paths, workspace, title, cfg)?;
            (Source::Sim(Box::new(sim)), Some(current))
        }
        (None, _) => (Source::Live(live_source_for(cfg, library)), None),
    })
}

impl Engine {
    pub fn start(cfg: EngineConfig) -> anyhow::Result<Self> {
        let state = Arc::new(ArcSwap::from_pointee(MusicState::default()));
        let status = Arc::new(ArcSwap::from_pointee(EngineStatus::Starting));
        let (tx, rx) = crossbeam_channel::unbounded::<EngineCommand>();

        // Load the library on the caller's thread so configuration errors surface immediately.
        let paths = match &cfg.app_dir {
            Some(dir) => RekordboxPaths::from_app_dir(dir)?,
            None => RekordboxPaths::discover()?,
        };
        let library = Library::open(&paths, &cfg.cache_dir)?;
        let mut tracks: Vec<TrackEntry> = library
            .tracks()
            .iter()
            .filter(|t| t.file_path.as_ref().is_some_and(|p| p.exists()))
            .map(|t| TrackEntry {
                title: t.title.clone(),
                label: track_label(&t.artist, &t.title),
            })
            .collect();
        tracks.sort_by_key(|t| t.label.to_lowercase());

        // The preparation workspace, when the DJ prepares tracks; read-only.
        let workspace = onset_workspace::Workspace::open_default();
        if let Some(ws) = &workspace {
            tracing::info!(path = %ws.path().display(), "workspace found");
        }

        let (source, current) = start_source(&library, &paths, workspace.as_ref(), &cfg)?;
        let simulator = matches!(source, Source::Sim(_) | Source::Blend(_));
        // With the simulator, capture is off unless asked for: then the simulator's output
        // comes back through the system capture, the way a real deck's does.
        let capture = if cfg.capture_audio {
            open_capture(cfg.audio_device.as_deref())
        } else {
            None
        };
        let audio_device = Arc::new(ArcSwap::from_pointee(
            capture
                .as_ref()
                .map_or_else(|| "none".to_string(), |c| c.stream.name().to_string()),
        ));
        let audio_device_w = audio_device.clone();
        let lyrics_progress = Arc::new(ArcSwap::from_pointee(String::new()));
        let lyrics_progress_w = lyrics_progress.clone();
        let audio_choice = Arc::new(parking_lot::Mutex::new(cfg.audio_device.clone()));

        let (state_w, status_w) = (state.clone(), status.clone());
        let started = Instant::now();
        let thread = std::thread::Builder::new()
            .name("onset-engine".into())
            .spawn(move || {
                if !priority::join_audio_class() {
                    tracing::debug!("engine thread keeps normal priority");
                }
                let mut engine = Running {
                    cfg,
                    paths,
                    library,
                    workspace,
                    source,
                    current,
                    capture,
                    clock: Clock::new(),
                    director: Director::new(),
                    hit_lock: onset_core::hits::HitLock::new(),
                    started,
                    last_tick: Instant::now(),
                    last_status: None,
                    unresolved: None,
                    matcher: None,
                    deck: None,
                    audio_checked: Instant::now(),
                    audio_device: audio_device_w,
                    audio_choice: audio_choice.clone(),
                    audio_want: spawn_audio_watch(audio_choice),
                    lyrics_progress: lyrics_progress_w,
                    prefetching: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                    recorder: None,
                    ear: None,
                    ear_bands: Vec::new(),
                    ear_lag_s: EAR_LAG_S,
                };

                engine.run(&rx, &state_w, &status_w);
            })?;
        Ok(Self {
            state,
            status,
            commands: tx,
            tracks,
            simulator,
            audio_device,
            lyrics_progress,
            started,
            _thread: thread,
        })
    }

    /// How old `state` is: the time since the engine assembled it.
    pub fn age(&self, state: &MusicState) -> Duration {
        let now = self.started.elapsed().as_secs_f64();
        Duration::from_secs_f64((now - state.time_s).max(0.0))
    }

    /// The latest assembled state; cheap to call every frame.
    pub fn state(&self) -> Arc<MusicState> {
        self.state.load_full()
    }

    pub fn status(&self) -> EngineStatus {
        (**self.status.load()).clone()
    }

    /// Playable library tracks (audio file present), sorted by artist and title.
    pub fn tracks(&self) -> &[TrackEntry] {
        &self.tracks
    }

    /// The audio device being captured ("none" when capture could not start).
    pub fn audio_device(&self) -> String {
        (**self.audio_device.load()).clone()
    }

    /// Progress of the whole-library lyrics look-up; empty when none has run.
    pub fn lyrics_progress(&self) -> String {
        (**self.lyrics_progress.load()).clone()
    }

    /// True when the developer simulator is the source (transport controls apply).
    pub fn is_simulator(&self) -> bool {
        self.simulator
    }

    pub fn command(&self, cmd: EngineCommand) {
        let _ = self.commands.send(cmd);
    }
}

/// The rekordbox source, told how long each library file is and its BPM.
fn live_source_for(cfg: &EngineConfig, library: &Library) -> Box<dyn TransportSource> {
    let mut live = live_source(cfg);
    live.set_file_durations(
        library
            .tracks()
            .iter()
            .filter_map(|t| Some((t.file_path.clone()?, f64::from(t.duration_s?))))
            .collect(),
    );
    live.set_file_bpms(
        library
            .tracks()
            .iter()
            .filter_map(|t| Some((t.file_path.clone()?, t.bpm.filter(|b| *b > 0.0)?)))
            .collect(),
    );
    live
}

#[cfg(windows)]
fn live_source(cfg: &EngineConfig) -> Box<dyn TransportSource> {
    Box::new(onset_transport::memory::reader::MemoryTransport::new(
        cfg.offsets_dir.clone(),
    ))
}

#[cfg(not(windows))]
fn live_source(_cfg: &EngineConfig) -> Box<dyn TransportSource> {
    struct Never;
    impl TransportSource for Never {
        fn name(&self) -> &'static str {
            "rekordbox"
        }
        fn status(&self) -> SourceStatus {
            SourceStatus::Unsupported("rekordbox memory reading needs Windows".into())
        }
        fn poll(&mut self) -> Option<TransportSnapshot> {
            None
        }
    }
    Box::new(Never)
}

/// State owned by the engine thread.
struct Running {
    cfg: EngineConfig,
    paths: RekordboxPaths,
    library: Library,
    /// The preparation workspace, opened read-only, when one exists.
    workspace: Option<onset_workspace::Workspace>,
    source: Source,
    current: Option<Current>,
    capture: Option<Capture>,
    clock: Clock,
    director: Director,
    /// Locks the kick and snare to the beat grid, confirmed by the audio.
    hit_lock: onset_core::hits::HitLock,
    started: Instant,
    last_tick: Instant,
    last_status: Option<EngineStatus>,
    /// The last track reference the library could not resolve, so it is reported once.
    unresolved: Option<TrackRef>,
    /// Names streamed tracks by their sound when no file does; built the first time a
    /// deck plays something no file names.
    matcher: Option<crate::stream_match::StreamMatcher>,
    /// The deck the last live snapshot came from.
    deck: Option<u8>,
    /// When the capture last checked it has the right device.
    audio_checked: Instant,
    /// Shared with the handle: the device being captured.
    audio_device: Arc<ArcSwap<String>>,
    /// The device choice, shared with the watch thread.
    audio_choice: Arc<parking_lot::Mutex<Option<String>>>,
    /// The watch thread's latest answer: the device the capture should use.
    audio_want: crossbeam_channel::Receiver<Option<String>>,
    /// Shared with the handle: the whole-library lyrics look-up's progress.
    lyrics_progress: Arc<ArcSwap<String>>,
    /// Set while the whole-library look-up runs, so a second press does not start another.
    prefetching: Arc<std::sync::atomic::AtomicBool>,
    /// Writes each tick's state to a file while a recording runs.
    recorder: Option<recorder::Recorder>,
    /// Weighs every playing deck against what the room hears (live only).
    ear: Option<Ear>,
    /// Waveforms loaded for the ear, by track, newest last.
    ear_bands: Vec<(TrackRef, Option<Arc<onset_core::bands::TrackBands>>)>,
    /// Seconds the captured audio trails the playhead, from the hit lock once measured.
    ear_lag_s: f64,
}

impl Running {
    fn status_now(&self) -> EngineStatus {
        match (self.source.status(), &self.current) {
            (SourceStatus::Searching, _) => EngineStatus::Waiting("waiting for rekordbox".into()),
            (SourceStatus::Scanning, _) => {
                EngineStatus::Waiting("rekordbox found, finding its decks".into())
            }
            (SourceStatus::Unsupported(why) | SourceStatus::Error(why), _) => {
                EngineStatus::Waiting(why)
            }
            (SourceStatus::Connected, Some(c)) => EngineStatus::Running {
                source: self.source.name().to_string(),
                track: format!("{} - {}", c.meta.artist, c.meta.title),
            },
            (SourceStatus::Connected, None) if self.clock.is_playing() => {
                EngineStatus::Unidentified {
                    source: self.source.name().to_string(),
                }
            }
            (SourceStatus::Connected, None) => EngineStatus::Idle,
        }
    }

    fn publish_status(&mut self, status: &ArcSwap<EngineStatus>) {
        let s = self.status_now();
        if self.last_status.as_ref() != Some(&s) {
            tracing::info!(status = %s.line(), "engine");
            status.store(Arc::new(s.clone()));
            self.last_status = Some(s);
        }
    }

    fn handle(&mut self, cmd: &EngineCommand, status: &ArcSwap<EngineStatus>) {
        match (cmd, &mut self.source) {
            (EngineCommand::Pause, Source::Sim(s)) => s.player.pause(true),
            (EngineCommand::Resume, Source::Sim(s)) => s.player.pause(false),
            (EngineCommand::Seek(t), Source::Sim(s)) => s.player.seek(*t),
            (EngineCommand::SetRate(r), Source::Sim(s)) => s.player.set_rate(*r),
            (EngineCommand::LoadTrack(title), Source::Sim(_)) => {
                match start_sim(
                    &self.library,
                    &self.paths,
                    self.workspace.as_ref(),
                    title,
                    &self.cfg,
                ) {
                    Ok((sim, current)) => {
                        self.source = Source::Sim(Box::new(sim));
                        self.current = Some(current);
                        self.clock = Clock::new();
                    }
                    Err(e) => {
                        tracing::error!("load failed: {e:#}");
                        status.store(Arc::new(EngineStatus::Error(format!("{e:#}"))));
                        self.last_status = None;
                    }
                }
            }
            (EngineCommand::PrefetchLyrics, _) => {
                use std::sync::atomic::Ordering;
                if !self.prefetching.swap(true, Ordering::AcqRel) {
                    let tracks = self.library.tracks().to_vec();
                    let cache = self.cfg.cache_dir.clone();
                    let progress = self.lyrics_progress.clone();
                    let running = self.prefetching.clone();
                    progress.store(Arc::new("Starting the lyrics look-up...".into()));
                    let spawned = std::thread::Builder::new()
                        .name("onset-lyrics-all".into())
                        .spawn(move || {
                            prefetch_lyrics(&tracks, &cache, &progress);
                            running.store(false, Ordering::Release);
                        });
                    if let Err(e) = spawned {
                        tracing::warn!(error = %e, "cannot start the lyrics look-up");
                        self.prefetching.store(false, Ordering::Release);
                    }
                }
            }
            (EngineCommand::SetAudioDevice(choice), _) => {
                self.cfg.audio_device.clone_from(choice);
                self.audio_choice.lock().clone_from(choice);
                self.tend_audio(true);
            }
            (EngineCommand::SetCapture(on), _) => {
                if self.cfg.capture_audio != *on {
                    tracing::info!(on, "audio capture");
                    self.cfg.capture_audio = *on;
                    if *on {
                        self.tend_audio(true);
                    } else {
                        self.capture = None;
                        self.audio_device.store(Arc::new("none".to_string()));
                    }
                }
            }
            (EngineCommand::Reconnect, Source::Live(_)) => {
                tracing::info!("reloading the rekordbox calibration");
                self.source = Source::Live(live_source_for(&self.cfg, &self.library));
                self.current = None;
                self.deck = None;
                self.ear = None;
                self.unresolved = None;
                self.clock = Clock::new();
                self.last_status = None;
            }
            (EngineCommand::Pause, Source::Blend(s)) => s.player.pause(true),
            (EngineCommand::Resume, Source::Blend(s)) => s.player.pause(false),
            (EngineCommand::Reconnect, Source::Sim(_)) => {}
            (EngineCommand::StartRecording(path), _) => {
                self.stop_recording();
                match recorder::Recorder::create(path) {
                    Ok(r) => {
                        tracing::info!(path = %path.display(), "recording the set");
                        self.recorder = Some(r);
                    }
                    Err(e) => tracing::error!(path = %path.display(), "cannot record: {e}"),
                }
            }
            (EngineCommand::StopRecording, _) => self.stop_recording(),
            (_, Source::Live(_)) => {
                tracing::debug!(?cmd, "transport command ignored: rekordbox is in control");
            }
            (_, Source::Blend(_)) => {
                tracing::debug!(
                    ?cmd,
                    "transport command ignored: the blend plays as scripted"
                );
            }
        }
    }

    fn stop_recording(&mut self) {
        if let Some(r) = self.recorder.take() {
            match r.finish() {
                Ok((path, frames)) => {
                    tracing::info!(path = %path.display(), frames, "recording finished");
                }
                Err(e) => tracing::error!("recording not closed cleanly: {e}"),
            }
        }
    }

    /// The deck the room hears, when the ear has a verdict: a snapshot of that deck to
    /// follow in place of the transport's choice. The transport picks by MASTER and play
    /// state, which cannot see the mixer; the ear fits the captured audio to every playing
    /// deck's waveform and switches only once another deck clearly carries the room.
    fn hear_deck(&mut self, now: Instant, time_s: f64) -> Option<TransportSnapshot> {
        let live = match &self.source {
            // rekordbox's decks, heard through the capture.
            Source::Live(source) if self.capture.is_some() => source.live_decks(),
            // The two-deck simulator, heard through its tap (or the capture, when on).
            Source::Blend(s) => s.player.live_decks(),
            _ => return None,
        };
        self.tend_ear(&live.decks);
        let g = self.audio(now).groups;
        let heard = [
            f32::midpoint(g[0], g[1]),
            f32::midpoint(g[2], g[3]),
            f32::midpoint(g[4], g[5]),
        ];
        let positions: Vec<(u8, f64)> = live
            .decks
            .iter()
            .filter(|d| d.playing)
            .map(|d| (d.deck, d.playhead_s))
            .collect();
        let state = self.ear.as_mut()?;
        let jumped = positions.iter().any(|&(deck, at)| {
            state.heads.iter().any(|&(d, before, then)| {
                // A playhead that moved other than with time: a cue press, a jump, a loop.
                d == deck && {
                    let moved = at - before;
                    let elapsed = time_s - then;
                    moved < -EAR_JUMP_S || moved > elapsed * 1.5 + EAR_JUMP_S
                }
            })
        });
        state.heads = positions.iter().map(|&(d, at)| (d, at, time_s)).collect();
        let ear = &mut state.ear;
        if jumped {
            ear.reset();
        }
        ear.set_lag(self.ear_lag_s);
        ear.listen(time_s, heard, &positions);
        ear.set_current(self.deck);
        let pick = ear.choose(time_s, live.master)?;
        if self.deck != Some(pick) {
            tracing::info!(deck = pick, verdict = ?ear.verdict(), "the room hears another deck");
        }
        live.decks.into_iter().find(|d| d.deck == pick)
    }

    /// Rebuilds the ear when the decks' tracks change, keeping the deck it follows.
    fn tend_ear(&mut self, decks: &[TransportSnapshot]) {
        let tracks: Vec<(u8, TrackRef)> = decks
            .iter()
            .filter(|d| d.track != TrackRef::Unknown)
            .map(|d| (d.deck, d.track.clone()))
            .collect();
        if self.ear.as_ref().is_some_and(|e| e.tracks == tracks) {
            return;
        }
        let ear_decks: Vec<deck_ear::EarDeck> = tracks
            .iter()
            .filter_map(|(deck, track)| {
                Some(deck_ear::EarDeck {
                    deck: *deck,
                    bands: self.bands_for(track)?,
                })
            })
            .collect();
        tracing::info!(
            decks = ear_decks.len(),
            of = tracks.len(),
            "listening for the decks' waveforms"
        );
        let following = self.ear.as_ref().and_then(|e| e.ear.current());
        let mut ear = deck_ear::DeckEar::new(ear_decks, self.ear_lag_s);
        ear.set_current(following.or(self.deck));
        self.ear = Some(Ear {
            ear,
            tracks,
            heads: Vec::new(),
        });
    }

    /// rekordbox's 3-band waveform for a deck's track, loaded once per track.
    fn bands_for(&mut self, track: &TrackRef) -> Option<Arc<onset_core::bands::TrackBands>> {
        if let Some((_, bands)) = self.ear_bands.iter().find(|(t, _)| t == track) {
            return bands.clone();
        }
        let bands = resolve_track(&self.library, track).and_then(|meta| {
            let rel = meta.analysis_path.as_deref()?;
            onset_rekordbox::anlz::load_analysis(&self.paths, rel)
                .map_err(|e| tracing::warn!(title = %meta.title, "no waveform for the ear: {e}"))
                .ok()?
                .bands
                .map(Arc::new)
        });
        if self.ear_bands.len() >= EAR_CACHE {
            self.ear_bands.remove(0);
        }
        self.ear_bands.push((track.clone(), bands.clone()));
        bands
    }

    /// A live snapshot names a track; keep `current` in step with it.
    fn follow_track(&mut self, snapshot: &TransportSnapshot) {
        if self.deck != Some(snapshot.deck) {
            self.deck = Some(snapshot.deck);
            if let Some(m) = self.matcher.as_mut() {
                m.reset();
            }
        }
        if snapshot.track == TrackRef::Unknown {
            // A deck with no file (a streamed track): keep what is known about this deck,
            // but never show the previous deck's track over it.
            if self
                .current
                .as_ref()
                .is_some_and(|c| c.deck != Some(snapshot.deck))
            {
                tracing::info!(
                    deck = snapshot.deck,
                    "playing deck's track is not identified yet"
                );
                self.current = None;
                self.clock = Clock::new();
            }
            return;
        }
        let same = self
            .current
            .as_ref()
            .is_some_and(|c| c.track_ref == snapshot.track);
        if same {
            if let Some(c) = self.current.as_mut() {
                c.deck = Some(snapshot.deck);
            }
            return;
        }
        if let Some(meta) = resolve_track(&self.library, &snapshot.track) {
            tracing::info!(title = %meta.title, artist = %meta.artist, "master deck track");
            if let Source::Live(s) = &mut self.source {
                s.set_deck_track(snapshot.deck, meta.sample_rate, meta.bpm);
            }
            let mut current = load_current(
                &self.library,
                &self.paths,
                self.workspace.as_ref(),
                meta,
                snapshot.track.clone(),
            );
            current.deck = Some(snapshot.deck);
            self.current = Some(current);
            self.clock = Clock::new();
            return;
        }
        if self.unresolved.as_ref() != Some(&snapshot.track) {
            tracing::warn!(track = ?snapshot.track, "master deck track not in the library");
            self.unresolved = Some(snapshot.track.clone());
        }
        if self.current.is_some() {
            self.current = None;
            self.clock = Clock::new();
        }
    }

    /// While the playing deck's track is unknown, listens for one of the library's streamed
    /// tracks and adopts it once the sound matches.
    fn recognise_stream(&mut self, now: Instant, time_s: f64) {
        if !matches!(self.source, Source::Live(_)) || !self.clock.is_playing() {
            return;
        }
        if self.matcher.is_none() {
            let candidates = streaming_candidates(&self.library, &self.paths);
            tracing::info!(
                tracks = candidates.len(),
                "streaming tracks that can be recognised by ear"
            );
            self.matcher = Some(crate::stream_match::StreamMatcher::new(candidates));
        }
        if !self
            .matcher
            .as_ref()
            .is_some_and(crate::stream_match::StreamMatcher::has_candidates)
        {
            return;
        }
        let Some(playhead) = self.clock.playhead_at(now) else {
            return;
        };
        let a = self.audio(now);
        let g = a.groups;
        let Some(matcher) = self.matcher.as_mut() else {
            return;
        };
        matcher.listen(
            time_s,
            playhead,
            [
                f32::midpoint(g[0], g[1]),
                f32::midpoint(g[2], g[3]),
                f32::midpoint(g[4], g[5]),
            ],
        );
        if let Some(meta) = matcher.best().cloned() {
            tracing::info!(title = %meta.title, artist = %meta.artist, "streamed track recognised by ear");
            if let (Source::Live(s), Some(deck)) = (&mut self.source, self.deck) {
                s.set_deck_track(deck, meta.sample_rate, meta.bpm);
            }
            let mut current = load_current(
                &self.library,
                &self.paths,
                self.workspace.as_ref(),
                meta,
                TrackRef::Unknown,
            );
            current.deck = self.deck;
            self.current = Some(current);
            if let Some(m) = self.matcher.as_mut() {
                m.reset();
            }
        }
    }

    /// Keeps the capture on the right device: reopens it when the device went away (a
    /// controller unplugged), and switches when the preferred one changes (the controller
    /// plugged back in, or a new choice in the launcher). Checks every few seconds, or now
    /// when `force` is set.
    fn tend_audio(&mut self, force: bool) {
        if !self.cfg.capture_audio {
            return;
        }
        let lost = self.capture.as_ref().is_none_or(|c| c.stream.is_lost());
        let have = self.capture.as_ref().map(|c| c.stream.name().to_string());
        // The watch thread's newest answer, if it has one.
        let mut want = None;
        while let Ok(w) = self.audio_want.try_recv() {
            want = Some(w);
        }
        let switch = match &want {
            Some(Some(name)) => have.as_ref() != Some(name),
            _ => false,
        };
        let retry_lost = lost && self.audio_checked.elapsed() >= Duration::from_secs(1);
        if !(force || switch || retry_lost) {
            return;
        }
        self.audio_checked = Instant::now();
        tracing::info!(from = ?have, to = ?want.flatten(), lost, "switching audio capture");
        self.capture = None; // close the old stream before opening the new one
        self.capture = open_capture(self.cfg.audio_device.as_deref());
        let name = self
            .capture
            .as_ref()
            .map_or_else(|| "none".to_string(), |c| c.stream.name().to_string());
        self.audio_device.store(Arc::new(name));
    }

    fn audio(&mut self, now: Instant) -> AudioFeatures {
        if let Some(cap) = self.capture.as_mut() {
            while let Ok(block) = cap.rx.try_recv() {
                cap.analyzer.push(&block);
            }
            return cap.analyzer.latest(now);
        }
        let (tap, analyzer) = match &mut self.source {
            Source::Sim(s) => (&s.tap, &mut s.analyzer),
            Source::Blend(s) => (&s.tap, &mut s.analyzer),
            Source::Live(_) => return AudioFeatures::silent(),
        };
        while let Ok(block) = tap.try_recv() {
            analyzer.push_at(&block, now);
        }
        analyzer.latest(now)
    }

    /// One tick of the engine at `now`: the present, or a simulated clock in tests.
    fn tick_at(&mut self, now: Instant) -> MusicState {
        let dt = now.duration_since(self.last_tick).as_secs_f32();
        self.last_tick = now;
        let time_s = now.duration_since(self.started).as_secs_f64();

        if let Some(mut snapshot) = self.source.poll() {
            // Sources with several decks: follow the one the room hears.
            if matches!(self.source, Source::Live(_) | Source::Blend(_)) {
                if let Some(heard) = self.hear_deck(now, time_s) {
                    snapshot = heard;
                }
                self.follow_track(&snapshot);
                // Memory reads carry the playing tempo only; the analysed tempo is ours.
                if let Some(bpm) = self.current.as_ref().and_then(|c| c.meta.bpm)
                    && bpm > 0.0
                {
                    snapshot.bpm_original = bpm;
                }
            }
            self.clock.observe(&snapshot);
        }

        self.tend_audio(false);
        if self.current.is_none() {
            self.recognise_stream(now, time_s);
        }
        let Some(current) = self.current.as_ref() else {
            // No identified track: the visuals still get the audio and the playhead.
            let st = onset_core::structure::StructureState::default();
            let intensity = self.director.update(&st, dt);
            let audio = self.audio(now);
            let playing = self.clock.is_playing();
            let mut ms = MusicState::assemble(
                time_s,
                self.clock.playhead_at(now).unwrap_or(0.0),
                playing,
                self.clock.rate(),
                &st,
                audio,
                intensity,
                None,
            );
            // No grid: the lock passes the audio through and keeps its lag estimate.
            ms.hits = self.hit_lock.update(&st, playing, 0.0, None, &audio, dt);
            ms.ear = self.ear_shares();
            return ms;
        };
        let playhead = self.clock.playhead_at(now).unwrap_or(0.0);
        let meta = current.meta.clone();
        let st = match &current.analysis {
            Some(a) => {
                Structure::new(&a.grid, a.phrases.as_ref(), &current.cues).at(playhead * 1000.0)
            }
            None => onset_core::structure::StructureState {
                bpm: meta.bpm.unwrap_or(0.0),
                ..Default::default()
            },
        };
        let analysis_now = current
            .analysis
            .as_ref()
            .and_then(|a| a.bands.as_ref())
            .map(|b| b.at(playhead));
        let structure_source = current.analysis.as_ref().map(|_| current.structure_source);
        let intensity = self.director.update(&st, dt);
        let audio = self.audio(now);
        let playing = self.clock.is_playing();
        let mut ms = MusicState::assemble(
            time_s,
            playhead,
            playing,
            self.clock.rate(),
            &st,
            audio,
            intensity,
            Some(meta),
        );
        if let Some(at) = analysis_now {
            ms.analysis = at;
        }
        ms.structure_source = structure_source;
        ms.hits = self
            .hit_lock
            .update(&st, playing, ms.bpm, analysis_now, &audio, dt);
        ms.ear = self.ear_shares();
        ms
    }

    /// What the ear makes of the room right now, for the HUD: each playing deck's share of
    /// what is heard and how well the mix explains it. `None` when it has nothing to say.
    fn ear_shares(&self) -> Option<onset_core::music_state::DeckShares> {
        let verdict = self.ear.as_ref()?.ear.verdict()?;
        Some(onset_core::music_state::DeckShares {
            shares: verdict.shares,
            fit: verdict.fit,
        })
    }

    fn run(
        &mut self,
        rx: &crossbeam_channel::Receiver<EngineCommand>,
        state: &ArcSwap<MusicState>,
        status: &ArcSwap<EngineStatus>,
    ) {
        self.publish_status(status);
        loop {
            while let Ok(cmd) = rx.try_recv() {
                self.handle(&cmd, status);
            }
            let tick_started = Instant::now();
            let ms = self.tick_at(tick_started);
            if ms.hits.lag_samples >= EAR_LAG_SAMPLES {
                self.ear_lag_s = f64::from(ms.hits.lag_s);
            }
            if let Some(r) = self.recorder.as_mut()
                && let Err(e) = r.push(&ms)
            {
                tracing::error!(path = %r.path().display(), "recording stopped: {e}");
                self.recorder = None;
            }
            state.store(Arc::new(ms));
            self.publish_status(status);

            // Sleep the remainder of the tick; the renderer reads whatever is newest.
            let spent = tick_started.elapsed();
            if spent < TICK {
                std::thread::sleep(TICK.saturating_sub(spent));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    #[test]
    fn track_label_drops_the_dash_without_an_artist() {
        assert_eq!(track_label("Adam Port", "Move"), "Adam Port — Move");
        assert_eq!(track_label("  ", "HORN"), "HORN");
    }

    #[test]
    fn status_lines_say_what_is_missing() {
        assert_eq!(
            EngineStatus::Waiting("waiting for rekordbox".into()).line(),
            "waiting for rekordbox"
        );
        assert_eq!(
            EngineStatus::Running {
                source: "rekordbox".into(),
                track: "A - B".into()
            }
            .line(),
            "rekordbox: A - B"
        );
    }

    fn fixture_root() -> Option<PathBuf> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/private");
        root.join("master.db").exists().then_some(root)
    }

    /// The private fixtures, when present together with the audio file of the demo track.
    fn fixtures() -> Option<PathBuf> {
        let root = fixture_root()?;
        let paths = RekordboxPaths::from_app_dir(&root).ok()?;
        let cache = tempfile::tempdir().ok()?;
        let library = Library::open(&paths, cache.path()).ok()?;
        let meta = library.find_by_title_artist("Move", "")?;
        meta.file_path
            .as_ref()
            .is_some_and(|p| p.exists())
            .then_some(root)
    }

    fn config(app_dir: PathBuf, cache: &Path, sim_track: Option<&str>) -> EngineConfig {
        EngineConfig {
            app_dir: Some(app_dir),
            cache_dir: cache.to_path_buf(),
            offsets_dir: cache.join("no-offsets-here"),
            sim_track: sim_track.map(str::to_string),
            sim_blend: None,
            sim_seek_s: 30.0,
            sim_gain: 0.0,
            audio_device: None,
            capture_audio: false,
        }
    }

    #[test]
    fn without_a_simulator_the_engine_waits_for_rekordbox_and_plays_nothing() {
        let Some(app_dir) = fixture_root() else {
            eprintln!("skipping: fixtures missing");
            return;
        };
        let cache = tempfile::tempdir().unwrap();
        let engine = Engine::start(config(app_dir, cache.path(), None)).expect("engine");
        assert!(!engine.is_simulator());
        std::thread::sleep(Duration::from_millis(400));
        match engine.status() {
            EngineStatus::Waiting(why) => assert!(
                why.contains("rekordbox"),
                "the reason names rekordbox: {why}"
            ),
            other => panic!("expected Waiting, got {other:?}"),
        }
        let s = engine.state();
        assert!(!s.playing && s.track.is_none(), "nothing plays on its own");
    }

    #[test]
    fn resolve_track_by_analysis_path_and_by_title() {
        let Some(app_dir) = fixture_root() else {
            eprintln!("skipping: fixtures missing");
            return;
        };
        let paths = RekordboxPaths::from_app_dir(&app_dir).unwrap();
        let cache = tempfile::tempdir().unwrap();
        let library = Library::open(&paths, cache.path()).unwrap();
        let by_title = resolve_track(
            &library,
            &TrackRef::TitleArtist {
                title: "Move".into(),
                artist: "Adam Port".into(),
                album: String::new(),
            },
        )
        .expect("title match");
        let rel = by_title.analysis_path.clone().expect("analysis path");
        let by_path = resolve_track(&library, &TrackRef::AnalysisPath(rel)).expect("path match");
        assert_eq!(by_path.id, by_title.id);
        assert!(resolve_track(&library, &TrackRef::Unknown).is_none());
    }

    #[test]
    fn simulator_engine_publishes_live_state() {
        let Some(app_dir) = fixtures() else {
            eprintln!("skipping: fixtures or track missing");
            return;
        };
        let cache = tempfile::tempdir().unwrap();
        let Ok(engine) = Engine::start(config(app_dir, cache.path(), Some("Move"))) else {
            eprintln!("skipping: no audio device");
            return;
        };
        assert!(engine.is_simulator());
        std::thread::sleep(Duration::from_millis(600));
        let a = engine.state();
        assert!(a.playing, "{a:?}");
        assert!((a.bpm - 120.0).abs() < 0.1, "bpm {}", a.bpm);
        assert!(
            a.playhead_s >= 30.0 && a.playhead_s < 32.0,
            "{}",
            a.playhead_s
        );
        assert!(a.phrase.is_some(), "structure must be known for Move");
        assert!(a.track.as_ref().is_some_and(|t| t.title == "Move"));

        engine.command(EngineCommand::Pause);
        std::thread::sleep(Duration::from_millis(150));
        let b = engine.state();
        std::thread::sleep(Duration::from_millis(150));
        let c = engine.state();
        assert!(!b.playing);
        assert!(
            (c.playhead_s - b.playhead_s).abs() < 1e-3,
            "paused playhead moved"
        );
        assert!(matches!(engine.status(), EngineStatus::Running { .. }));
    }

    // The engine following a blend by ear, with the two-deck simulator in place of
    // rekordbox and a simulated clock in place of the tick thread.

    /// A track for the blend tests: an id, a title and a tempo.
    fn track_meta(id: u64, title: &str) -> TrackMeta {
        TrackMeta {
            id: onset_core::track::TrackId(id),
            title: title.into(),
            artist: String::new(),
            album: String::new(),
            year: None,
            key: None,
            bpm: Some(128.0),
            duration_s: None,
            sample_rate: None,
            file_path: None,
            artwork_path: None,
            analysis_path: None,
            isrc: None,
            genre: None,
            extra: onset_core::track::TrackExtra::default(),
        }
    }

    /// An engine as the tick thread runs it, without the thread, a device or rekordbox.
    fn headless_engine(
        library: Library,
        paths: RekordboxPaths,
        source: Source,
        epoch: Instant,
    ) -> Running {
        let nowhere = PathBuf::from("no-such-folder");
        Running {
            cfg: EngineConfig {
                app_dir: None,
                cache_dir: nowhere.clone(),
                offsets_dir: nowhere,
                sim_track: None,
                sim_blend: None,
                sim_seek_s: 0.0,
                sim_gain: 0.0,
                audio_device: None,
                capture_audio: false,
            },
            paths,
            library,
            workspace: None,
            source,
            current: None,
            capture: None,
            clock: Clock::new(),
            director: Director::new(),
            hit_lock: onset_core::hits::HitLock::new(),
            started: epoch,
            last_tick: epoch,
            last_status: None,
            unresolved: None,
            matcher: None,
            deck: None,
            audio_checked: epoch,
            audio_device: Arc::new(ArcSwap::from_pointee("none".to_string())),
            audio_choice: Arc::new(parking_lot::Mutex::new(None)),
            audio_want: crossbeam_channel::never(),
            lyrics_progress: Arc::new(ArcSwap::from_pointee(String::new())),
            prefetching: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            recorder: None,
            ear: None,
            ear_bands: Vec::new(),
            ear_lag_s: EAR_LAG_S,
        }
    }

    fn blend_source(player: BlendPlayer) -> Source {
        let tap = player.tap_audio();
        let analyzer = Analyzer::new(player.tap_sample_rate());
        Source::Blend(Box::new(BlendSource {
            player,
            tap,
            analyzer,
        }))
    }

    /// Plays the blend to `end_s` at 60 ticks a second and returns, for each tick, its time
    /// and the deck the engine followed, with the title of the track it showed last.
    fn play_blend(
        engine: &mut Running,
        epoch: Instant,
        end_s: f64,
    ) -> (Vec<(f64, Option<u8>)>, MusicState) {
        const STEP_S: f64 = 1.0 / 60.0;
        let mut followed = Vec::new();
        let mut last = MusicState::default();
        let mut ticks = 0u32;
        while f64::from(ticks) * STEP_S < end_s {
            if let Source::Blend(s) = &mut engine.source {
                s.player.advance(STEP_S);
            }
            ticks += 1;
            let t = f64::from(ticks) * STEP_S;
            last = engine.tick_at(epoch + Duration::from_secs_f64(t));
            followed.push((t, engine.deck));
        }
        (followed, last)
    }

    /// The followed deck moves from 0 to 1 once, after the blend is half done (when deck 1
    /// is no louder than deck 0 yet) and within `settle_s` of its end, and stays there.
    /// Returns when it moved.
    fn assert_follows_the_blend(
        followed: &[(f64, Option<u8>)],
        script: &BlendScript,
        settle_s: f64,
    ) -> f64 {
        let first_1 = followed.iter().position(|(_, d)| *d == Some(1));
        let Some(i) = first_1 else {
            panic!("never followed deck 1: {:?}", followed.last());
        };
        let switched_s = followed[i].0;
        assert!(
            followed[..i].iter().all(|(t, d)| *d == Some(0) || *t < 0.1),
            "deck 0 until the switch"
        );
        assert!(
            followed[i..].iter().all(|(_, d)| *d == Some(1)),
            "deck 1 from the switch on: {:?}",
            followed[i..].iter().find(|(_, d)| *d != Some(1))
        );
        assert!(
            switched_s > script.fade_start_s + script.fade_s * 0.5,
            "switched at {switched_s:.1} s, before the blend was half done ({:.1}-{:.1} s)",
            script.fade_start_s,
            script.fade_end_s()
        );
        assert!(
            switched_s < script.fade_end_s() + settle_s,
            "switched at {switched_s:.1} s, long after the blend ended at {:.1} s",
            script.fade_end_s()
        );
        switched_s
    }

    /// A seeded noise source (xorshift), -1..1.
    struct Noise(u32);

    impl Noise {
        fn sample(&mut self) -> f32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 17;
            self.0 ^= self.0 << 5;
            self.0 as f32 / u32::MAX as f32 * 2.0 - 1.0
        }
    }

    /// A band-pass biquad (RBJ, 0 dB at the centre).
    struct BandPass {
        b: [f32; 3],
        a: [f32; 2],
        x: [f32; 2],
        y: [f32; 2],
    }

    impl BandPass {
        fn new(centre_hz: f32, q: f32, rate: f32) -> Self {
            let w = 2.0 * std::f32::consts::PI * centre_hz / rate;
            let alpha = w.sin() / (2.0 * q);
            let a0 = 1.0 + alpha;
            Self {
                b: [alpha / a0, 0.0, -alpha / a0],
                a: [-2.0 * w.cos() / a0, (1.0 - alpha) / a0],
                x: [0.0; 2],
                y: [0.0; 2],
            }
        }

        fn run(&mut self, x: f32) -> f32 {
            let y = self.b[0] * x + self.b[1] * self.x[0] + self.b[2] * self.x[1]
                - self.a[0] * self.y[0]
                - self.a[1] * self.y[1];
            self.x = [x, self.x[0]];
            self.y = [y, self.y[0]];
            y
        }
    }

    /// A synthetic track and the waveform rekordbox would draw for it: noise in the low,
    /// mid and high bands, each band switching between loud and quiet sections of its own
    /// length (`(section_s, offset_s)`), so two tracks differ the way real ones do, in when
    /// their sections change.
    fn synthetic_track(
        seed: u32,
        sections: [(f64, f64); 3],
        secs: f64,
        rate: u32,
    ) -> (
        onset_transport::decode::DecodedAudio,
        onset_core::bands::TrackBands,
    ) {
        const CENTRES_HZ: [f32; 3] = [70.0, 450.0, 4500.0];
        let level = |band: usize, t: f64| {
            let (len, offset) = sections[band];
            if ((t + offset) / len).floor() as i64 % 2 == 0 {
                1.0f32
            } else {
                0.2
            }
        };
        let mut noise = [
            Noise(seed),
            Noise(seed ^ 0x9e37_79b9),
            Noise(seed ^ 0x7f4a_7c15),
        ];
        // Two passes per band for steeper skirts, so one band's sections stay out of the
        // others.
        let mut filters: Vec<[BandPass; 2]> = CENTRES_HZ
            .iter()
            .map(|&hz| [0, 1].map(|_| BandPass::new(hz, 0.8, rate as f32)))
            .collect();
        let frames = (secs * f64::from(rate)) as usize;
        let samples = (0..frames)
            .map(|i| {
                let t = i as f64 / f64::from(rate);
                (0..3)
                    .map(|b| {
                        let [f1, f2] = &mut filters[b];
                        f2.run(f1.run(noise[b].sample())) * level(b, t)
                    })
                    .sum::<f32>()
            })
            .collect();
        let wave_rate = 150.0;
        let wave = (0..(secs * wave_rate) as usize)
            .map(|i| {
                let t = i as f64 / wave_rate;
                [0, 1, 2].map(|b| (level(b, t) * 255.0) as u8)
            })
            .collect();
        (
            onset_transport::decode::DecodedAudio {
                sample_rate: rate,
                channels: 1,
                samples,
            },
            onset_core::bands::TrackBands::new(wave_rate as f32, wave, 1.0, vec![0]),
        )
    }

    #[test]
    fn the_ear_moves_the_show_to_the_incoming_deck_after_a_blend() {
        const RATE: u32 = 44_100;
        // Deck 0 alone for 20 s, a 16-bar blend at 128 BPM (30 s), deck 1 alone for 12 s,
        // with deck 0 still running underneath as the transport keeps naming it.
        let script = BlendScript::bars(20.0, 16, 128.0);
        let end_s = script.fade_end_s() + 12.0;
        let (audio_out, bands_out) = synthetic_track(
            0x1234_5678,
            [(3.0, 0.0), (4.2, 1.3), (2.4, 0.7)],
            end_s + 5.0,
            RATE,
        );
        let (audio_in, bands_in) = synthetic_track(
            0x0bad_cafe,
            [(3.6, 1.1), (2.8, 0.4), (5.0, 2.2)],
            end_s + 5.0,
            RATE,
        );
        let deck = |id: u64, audio: onset_transport::decode::DecodedAudio| BlendDeck {
            audio: Arc::new(audio),
            track: TrackRef::Id(onset_core::track::TrackId(id)),
            bpm: 128.0,
            cue_s: 0.0,
            rate: 1.0,
        };
        let epoch = Instant::now();
        let player =
            BlendPlayer::headless([deck(1, audio_out), deck(2, audio_in)], script, RATE, epoch);
        let library =
            Library::from_tracks(vec![track_meta(1, "Outgoing"), track_meta(2, "Incoming")]);
        let nowhere = PathBuf::from("no-such-folder");
        let paths = RekordboxPaths {
            master_db: nowhere.join("master.db"),
            master_wal: nowhere.join("master.db-wal"),
            share_dir: nowhere.join("share"),
            app_dir: nowhere,
        };
        let mut engine = headless_engine(library, paths, blend_source(player), epoch);
        // The waveforms rekordbox would have drawn, where the ear looks them up.
        let id = |n| TrackRef::Id(onset_core::track::TrackId(n));
        engine.ear_bands = vec![
            (id(1), Some(Arc::new(bands_out))),
            (id(2), Some(Arc::new(bands_in))),
        ];

        let (followed, last) = play_blend(&mut engine, epoch, end_s);
        let switched_s = assert_follows_the_blend(&followed, &script, 8.0);
        eprintln!(
            "followed deck 1 from {switched_s:.2} s (blend {:.0}-{:.0} s)",
            script.fade_start_s,
            script.fade_end_s()
        );
        assert_eq!(
            last.track.as_ref().map(|t| t.title.as_str()),
            Some("Incoming"),
            "the show names the incoming track"
        );
        let ear = last.ear.expect("the ear has a verdict at the end");
        let share_in = ear.shares.iter().find(|s| s.0 == 1).map_or(0.0, |s| s.1);
        assert!(share_in > 0.8, "{ear:?}");
    }

    /// The same blend with two tracks from the real rekordbox library and rekordbox's own
    /// waveforms, named in `ONSET_BLEND` as `first title|second title`. Reads the library
    /// read-only (decrypting a copy into a temporary folder) and decodes both files, so it
    /// takes a while; run by hand with `--ignored`.
    #[test]
    #[ignore = "needs the rekordbox library and ONSET_BLEND=\"first title|second title\""]
    fn the_ear_follows_a_blend_of_two_library_tracks() {
        let titles = std::env::var("ONSET_BLEND").expect("ONSET_BLEND=\"first|second\"");
        let (first, second) = titles.split_once('|').expect("two titles split by |");
        let paths = RekordboxPaths::discover().expect("rekordbox data folder");
        let cache = tempfile::tempdir().unwrap();
        let library = Library::open(&paths, cache.path()).expect("library");
        let (a, file_a) = sim_file(&library, first.trim()).unwrap();
        let (b, file_b) = sim_file(&library, second.trim()).unwrap();
        let bpm = |m: &TrackMeta| m.bpm.unwrap_or(120.0);
        let script = BlendScript::bars(20.0, 16, bpm(&a));
        let end_s = script.fade_end_s() + 12.0;
        let decks = [
            BlendDeck {
                audio: Arc::new(onset_transport::decode::decode_file(&file_a).unwrap()),
                track: TrackRef::Id(a.id),
                bpm: bpm(&a),
                // Into the body of the outgoing track, from the start of the incoming one.
                cue_s: 60.0,
                rate: 1.0,
            },
            BlendDeck {
                audio: Arc::new(onset_transport::decode::decode_file(&file_b).unwrap()),
                track: TrackRef::Id(b.id),
                bpm: bpm(&b),
                cue_s: 0.0,
                rate: 1.0,
            },
        ];
        let epoch = Instant::now();
        let player = BlendPlayer::headless(decks, script, 44_100, epoch);
        let mut engine = headless_engine(library, paths, blend_source(player), epoch);
        let (followed, last) = play_blend(&mut engine, epoch, end_s);
        eprintln!("ear at the end: {:?}", last.ear);
        let switched_s = assert_follows_the_blend(&followed, &script, 8.0);
        eprintln!(
            "{first} -> {second}: followed deck 1 from {switched_s:.2} s (blend {:.0}-{:.0} s)",
            script.fade_start_s,
            script.fade_end_s()
        );
    }
}
