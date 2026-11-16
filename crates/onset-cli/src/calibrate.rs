//! `calibrate`: the guided calibration session on the terminal. The session itself lives in
//! `onset_transport::memory::calibrator`; this prints its progress lines.
#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use onset_core::track::TrackId;
use onset_transport::memory::REKORDBOX_EXE;
use onset_transport::memory::calibrator::{self, Session};
use onset_transport::memory::offsets::Offsets;
use onset_transport::memory::process::Process;
use onset_transport::memory::reader::ChainReader;
use onset_transport::memory::scan::{find_bytes, find_u64_values};

use crate::common::open_library;

pub fn calibrate(out_dir: &Path, step_seconds: u64) -> anyhow::Result<()> {
    let cancel = AtomicBool::new(false);
    let mut print = |line: &str| println!("{line}");
    let mut session = Session {
        step_limit: Duration::from_secs(step_seconds),
        report: &mut print,
        cancel: &cancel,
    };
    calibrator::calibrate(out_dir, &mut session)?;
    Ok(())
}

pub fn refine(dir: &Path) -> anyhow::Result<()> {
    let cancel = AtomicBool::new(false);
    let mut print = |line: &str| println!("{line}");
    let mut session = Session {
        step_limit: Duration::from_secs(60),
        report: &mut print,
        cancel: &cancel,
    };
    calibrator::refine_existing(dir, &mut session)?;
    Ok(())
}

/// Writes the calibration every file in `files` agrees on to `<out_dir>/<version>.toml`.
pub fn merge(files: &[PathBuf], out_dir: &Path) -> anyhow::Result<()> {
    let runs: Vec<Offsets> = files
        .iter()
        .map(|f| Offsets::load(f))
        .collect::<anyhow::Result<_>>()?;
    let merged = calibrator::merge_offsets(&runs)?;
    let sig = merged.signature.as_ref().map_or(0, |s| s.anchors.len());
    let path = out_dir.join(format!("{}.toml", merged.rekordbox_version));
    std::fs::create_dir_all(out_dir)?;
    merged.save(&path)?;
    println!(
        "{} anchor(s) shared by {} calibration(s); wrote {}",
        sig,
        runs.len(),
        path.display()
    );
    Ok(())
}

/// Prints what the live link reads for each deck of the running rekordbox.
pub fn decks(offsets_dir: &Path, app_dir: Option<PathBuf>) -> anyhow::Result<()> {
    let process = Process::open(REKORDBOX_EXE)?;
    let version = process
        .file_version()
        .ok_or_else(|| anyhow::anyhow!("rekordbox version unknown"))?;
    let offsets = Offsets::find_version(offsets_dir, &version).ok_or_else(|| {
        anyhow::anyhow!(
            "no offsets for rekordbox {version} in {}",
            offsets_dir.display()
        )
    })?;
    let (_, library) = open_library(app_dir)?;
    let mut reader = ChainReader::new(process, offsets);
    let needle = reader
        .signature_needle()
        .ok_or_else(|| anyhow::anyhow!("the offsets file has no signature"))?;
    let hits = find_bytes(reader.mem(), &needle, 64);
    let decks = reader.adopt_hits(&hits);
    if let Some(values) = reader.deck_list_values() {
        let hits = find_u64_values(reader.mem(), &values, 256);
        if let Some(order) = reader.adopt_deck_list(&hits) {
            println!("deck order from rekordbox's deck list: objects {order:?} by address");
        } else {
            println!("rekordbox's deck list not found; decks in address order");
        }
    }
    let players = match reader.player_needle() {
        Some(n) => {
            let hits = find_bytes(reader.mem(), &n, 256);
            reader.adopt_player_hits(&hits)
        }
        None => 0,
    };
    println!("rekordbox {version}: {decks} deck object(s), {players} player object(s)");
    match reader.master_deck() {
        Some(m) => println!("  MASTER: deck object {m}"),
        None => println!("  MASTER: none read"),
    }
    for i in 0..decks {
        let Some(d) = reader.deck(i) else {
            println!("  deck object {i}: unreadable");
            continue;
        };
        let bpm = d.track_bpm.map_or("-".into(), |b| format!("{b:.2}"));
        let track = match reader.track_id(i) {
            Some(id) => library
                .by_id(TrackId(u64::from(id)))
                .map_or(format!("id {id} (not in the library)"), |t| {
                    format!("{} - {} (id {id})", t.artist, t.title)
                }),
            None => "-".into(),
        };
        println!(
            "  deck object {i}: {:8.2} s  track BPM {bpm:>7}  {track}",
            d.position_s
        );
    }
    Ok(())
}

pub fn default_out_dir() -> PathBuf {
    std::env::var_os("ONSET_OFFSETS").map_or_else(|| PathBuf::from("offsets"), PathBuf::from)
}
