//! Onset: real-time, structure-aware visuals for rekordbox DJs. The desktop app shows them in
//! its projector window; this binary runs them on their own, for development and the bench.
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use onset_visuals::app::{AppOptions, BenchPace};
use onset_visuals::config::{Config, MonitorChoice};
use onset_visuals::engine::{Engine, EngineCommand, EngineConfig};

#[derive(Parser)]
#[command(
    name = "onset",
    about = "Real-time, structure-aware visuals for rekordbox DJs",
    version
)]
struct Cli {
    /// Output monitor: a zero-based index, or part of its name (overrides the config)
    #[arg(long)]
    monitor: Option<String>,
    /// Developer mode: play this library title through the built-in simulator instead of
    /// reading rekordbox
    #[arg(long)]
    sim: Option<String>,

    /// With --sim: hear the simulator through the system audio capture instead of its own
    /// tap, so the show runs the whole audio path (and the HUD scope measures its lag)
    /// without a deck. Turn the speakers on: silence captures as silence.
    #[arg(long, requires = "sim")]
    sim_loopback: bool,
    /// rekordbox data folder (defaults to %APPDATA%\Pioneer\rekordbox)
    #[arg(long)]
    app_dir: Option<PathBuf>,
    /// Simulator start position in seconds
    #[arg(long, default_value_t = 0.0)]
    seek: f64,
    /// Simulator output gain 0..1
    #[arg(long, default_value_t = 0.5)]
    gain: f32,
    /// Start on this scene (overrides the config)
    #[arg(long)]
    scene: Option<String>,
    /// Close after this many seconds (for unattended checks)
    #[arg(long)]
    exit_after: Option<f64>,
    /// Print where the config file lives and exit
    #[arg(long)]
    config_path: bool,
    /// Run for this many seconds without vsync, then print frame statistics and exit
    #[arg(long, value_name = "SECONDS")]
    bench: Option<f64>,
    /// With --bench: `uncapped` presents without vsync; `show` keeps the configured present
    /// mode, as in a show
    #[arg(long, value_enum, default_value_t = BenchPace::Uncapped)]
    bench_pace: BenchPace,
    /// Frames the GPU may queue ahead of the display (1 or 2); fewer means less delay
    #[arg(long, default_value_t = 2, value_parser = clap::value_parser!(u32).range(1..=2))]
    frame_latency: u32,
    /// Save one frame as PNG here, a second before exit (or 3 s in without --exit-after)
    #[arg(long, value_name = "PNG")]
    screenshot: Option<PathBuf>,
    /// Record the engine's state every tick to this file, for replaying through the
    /// headless renderer (defaults to a timestamped file in the app's data folder)
    #[arg(long, value_name = "FILE")]
    #[allow(clippy::option_option)] // clap's shape for a flag with an optional value
    record: Option<Option<PathBuf>>,
}

fn parse_monitor(s: &str) -> MonitorChoice {
    s.parse::<usize>().map_or_else(
        |_| MonitorChoice::NameContains(s.to_string()),
        MonitorChoice::Index,
    )
}

/// `ONSET_OFFSETS` if set, else `offsets/` beside the executable, else `offsets/` in the
/// working directory (the development layout).
fn offsets_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("ONSET_OFFSETS") {
        return PathBuf::from(dir);
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let beside = dir.join("offsets");
        if beside.is_dir() {
            return beside;
        }
    }
    PathBuf::from("offsets")
}

fn cache_dir() -> PathBuf {
    directories::ProjectDirs::from("", "Onset", "Onset")
        .map_or_else(|| PathBuf::from(".cache"), |d| d.cache_dir().to_path_buf())
}

/// `recordings/set-<unix seconds>.onsetrec` in the app's data folder.
fn default_recording_path() -> PathBuf {
    let data = directories::ProjectDirs::from("", "Onset", "Onset").map_or_else(
        || PathBuf::from(".data"),
        |d| d.data_local_dir().to_path_buf(),
    );
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    data.join("recordings")
        .join(format!("set-{stamp}.{}", onset_core::recording::EXTENSION))
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new("info,wgpu_core=warn,wgpu_hal=warn")
            }),
        )
        .with_writer(std::io::stderr)
        .init();
    let cli = Cli::parse();
    if cli.config_path {
        println!("{}", Config::path().display());
        return Ok(());
    }
    let config = Config::load();
    let offsets_dir = offsets_dir();

    // Onset is a visualiser: rekordbox is the source. The simulator runs only when asked.
    let engine = match Engine::start(EngineConfig {
        app_dir: cli.app_dir.clone(),
        cache_dir: cache_dir(),
        offsets_dir: offsets_dir.clone(),
        sim_track: cli.sim.clone(),
        sim_seek_s: cli.seek,
        sim_gain: cli.gain,
        audio_device: config.audio_device.clone(),
        capture_audio: cli.sim.is_none() || cli.sim_loopback,
    }) {
        Ok(e) => {
            tracing::info!(status = ?e.status(), "engine started");
            if let Some(target) = &cli.record {
                let path = target.clone().unwrap_or_else(default_recording_path);
                e.command(EngineCommand::StartRecording(path));
            }
            Some(Arc::new(e))
        }
        Err(e) => {
            tracing::error!("engine did not start (visuals will idle): {e:#}");
            None
        }
    };

    let opts = AppOptions {
        config,
        monitor: cli.monitor.as_deref().map(parse_monitor),
        scene: cli.scene.clone(),
        exit_after: cli.exit_after.or(cli.bench).map(Duration::from_secs_f64),
        bench: cli.bench.map(|_| cli.bench_pace),
        frame_latency: cli.frame_latency,
        hosted: false,
        keep_clear: None,
        screenshot: cli.screenshot.clone(),
        cache_dir: cache_dir(),
        engine: engine.clone(),
    };
    let result = onset_visuals::run(opts);
    if let Some(e) = engine.filter(|_| cli.record.is_some()) {
        // Close the recording before the process ends; the engine thread does it at once.
        e.command(EngineCommand::StopRecording);
        std::thread::sleep(Duration::from_millis(50));
    }
    result
}
