//! The rekordbox link panel: teaches OnSet where this rekordbox version keeps its deck
//! positions. It runs the calibrator (the same one as `onset-cli calibrate`) on a thread,
//! streams its prompts to the screens as `onset:calibration` events and, on success, saves
//! the result where the live view reads it, keeping a copy of the file it replaces. It only
//! reads rekordbox's memory: nothing here writes to rekordbox or its database.
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use parking_lot::Mutex;
use serde::Serialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter};

/// How long each step waits for the DJ before giving up.
#[cfg(windows)]
const STEP_LIMIT: std::time::Duration = std::time::Duration::from_secs(240);

/// The cancel flag of the calibration in progress, if any.
static RUNNING: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);

/// One `onset:calibration` event.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Progress {
    /// A line from the calibrator: a prompt ("Step 2: press play on that deck.") or progress.
    Line {
        text: String,
    },
    /// Saved at `path`; `backup` holds the calibration it replaced.
    Done {
        path: String,
        backup: Option<String>,
    },
    Failed {
        error: String,
    },
}

/// Where a new calibration went.
#[derive(Debug, PartialEq)]
pub struct Installed {
    pub path: PathBuf,
    pub backup: Option<PathBuf>,
}

/// Puts the calibration `new_file` into `dir` under the same name. An existing file is first
/// copied to `<name>.bak-<stamp>` (the live view only reads `<version>.toml`, so backups stay
/// out of its way), and the new file is written beside it and renamed over, so a failure
/// part-way never leaves a half-written calibration.
pub fn install(new_file: &Path, dir: &Path, stamp: u64) -> std::io::Result<Installed> {
    let name = new_file
        .file_name()
        .ok_or_else(|| std::io::Error::other("the calibration has no file name"))?
        .to_string_lossy()
        .into_owned();
    std::fs::create_dir_all(dir)?;
    let target = dir.join(&name);
    let backup = if target.exists() {
        let backup = dir.join(format!("{name}.bak-{stamp}"));
        std::fs::copy(&target, &backup)?;
        Some(backup)
    } else {
        None
    };
    let staged = dir.join(format!(".{name}.new"));
    std::fs::copy(new_file, &staged)?;
    if let Err(e) = std::fs::rename(&staged, &target) {
        let _ = std::fs::remove_file(&staged);
        return Err(e);
    }
    Ok(Installed {
        path: target,
        backup,
    })
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// What the panel shows before a calibration: is rekordbox running, which version, and does
/// the live view already have a calibration for it.
#[tauri::command]
pub async fn calibration_status() -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(status)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(windows)]
fn status() -> Value {
    use onset_transport::memory::calibrator::STEPS;
    use onset_transport::memory::offsets::Offsets;
    use onset_transport::memory::{REKORDBOX_EXE, process::Process};

    let dir = crate::live::offsets_dir();
    let (running, version, problem) = match Process::open(REKORDBOX_EXE) {
        Ok(p) => match p.file_version() {
            Some(v) => (true, Some(v), None),
            None => (
                true,
                None,
                Some("rekordbox's version could not be read".to_string()),
            ),
        },
        Err(e) => (
            onset_transport::memory::process::find_pid(REKORDBOX_EXE).is_some(),
            None,
            Some(format!("{e:#}")),
        ),
    };
    let calibration = version.as_deref().and_then(|v| {
        Offsets::search_dirs(&dir)
            .into_iter()
            .find(|d| Offsets::for_version(d, v).is_some())
            .map(|d| d.join(format!("{v}.toml")))
    });
    json!({
        "supported": true,
        "rekordbox_running": running,
        "version": version,
        "problem": problem,
        "calibrated": calibration.is_some(),
        "calibration_path": calibration.map(|p| p.display().to_string()),
        "offsets_dir": dir.display().to_string(),
        "calibrating": RUNNING.lock().is_some(),
        "steps": STEPS,
    })
}

#[cfg(not(windows))]
fn status() -> Value {
    json!({
        "supported": false,
        "problem": "reading rekordbox's decks needs Windows",
        "rekordbox_running": false,
        "calibrated": false,
        "calibrating": false,
        "steps": [],
    })
}

/// Starts a calibration in the background; progress arrives as `onset:calibration` events.
#[tauri::command]
pub fn calibration_start(app: AppHandle) -> Result<(), String> {
    let cancel = {
        let mut running = RUNNING.lock();
        if running.is_some() {
            return Err("a calibration is already running".into());
        }
        let cancel = Arc::new(AtomicBool::new(false));
        *running = Some(cancel.clone());
        cancel
    };
    let spawned = std::thread::Builder::new()
        .name("onset-calibration".into())
        .spawn(move || {
            let outcome = run(&app, &cancel);
            *RUNNING.lock() = None;
            let _ = app.emit("onset:calibration", outcome);
        });
    if let Err(e) = spawned {
        *RUNNING.lock() = None;
        return Err(format!("cannot start the calibration: {e}"));
    }
    Ok(())
}

/// Stops the calibration in progress at its next check; nothing is saved.
#[tauri::command]
pub fn calibration_cancel() {
    if let Some(cancel) = RUNNING.lock().as_ref() {
        cancel.store(true, std::sync::atomic::Ordering::Release);
    }
}

#[cfg(windows)]
fn run(app: &AppHandle, cancel: &AtomicBool) -> Progress {
    use onset_transport::memory::calibrator::{Session, calibrate};
    use onset_transport::memory::offsets::Offsets;

    // The calibrator writes into a scratch folder; only a finished, readable result is
    // moved to where the live view reads.
    let scratch = std::env::temp_dir().join(format!("onset-calibration-{}", unix_now()));
    let result = (|| -> anyhow::Result<Installed> {
        std::fs::create_dir_all(&scratch)?;
        let mut report = |line: &str| {
            let _ = app.emit(
                "onset:calibration",
                Progress::Line {
                    text: line.to_string(),
                },
            );
        };
        let mut session = Session {
            step_limit: STEP_LIMIT,
            report: &mut report,
            cancel,
        };
        let written = calibrate(&scratch, &mut session)?;
        Offsets::load(&written)?;
        Ok(install(&written, &crate::live::offsets_dir(), unix_now())?)
    })();
    let _ = std::fs::remove_dir_all(&scratch);
    match result {
        Ok(installed) => {
            tracing::info!(path = %installed.path.display(), "rekordbox calibration saved");
            crate::live::reconnect();
            Progress::Done {
                path: installed.path.display().to_string(),
                backup: installed.backup.map(|b| b.display().to_string()),
            }
        }
        Err(e) => {
            tracing::warn!("calibration failed: {e:#}");
            Progress::Failed {
                error: format!("{e:#}"),
            }
        }
    }
}

#[cfg(not(windows))]
fn run(_app: &AppHandle, _cancel: &AtomicBool) -> Progress {
    Progress::Failed {
        error: "reading rekordbox's decks needs Windows".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_first_calibration_is_saved_without_a_backup() {
        let scratch = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let new = scratch.path().join("7.2.18.toml");
        std::fs::write(&new, "new").unwrap();
        let installed = install(&new, &dir.path().join("offsets"), 7).unwrap();
        assert_eq!(installed.backup, None);
        assert_eq!(std::fs::read_to_string(&installed.path).unwrap(), "new");
    }

    #[test]
    fn a_new_calibration_keeps_the_one_it_replaces() {
        let scratch = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("7.2.18.toml"), "old").unwrap();
        let new = scratch.path().join("7.2.18.toml");
        std::fs::write(&new, "new").unwrap();
        let installed = install(&new, dir.path(), 1_760_000_000).unwrap();
        assert_eq!(std::fs::read_to_string(&installed.path).unwrap(), "new");
        let backup = installed.backup.unwrap();
        assert_eq!(
            backup.file_name().unwrap(),
            "7.2.18.toml.bak-1760000000",
            "the live view only reads <version>.toml, so the backup is never loaded"
        );
        assert_eq!(std::fs::read_to_string(backup).unwrap(), "old");
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), 2, "no staging file is left behind: {names:?}");
    }

    #[test]
    fn events_carry_their_kind() {
        let line = serde_json::to_value(Progress::Line {
            text: "Step 2: press play on that deck.".into(),
        })
        .unwrap();
        assert_eq!(line["kind"], "line");
        let done = serde_json::to_value(Progress::Done {
            path: "x".into(),
            backup: None,
        })
        .unwrap();
        assert_eq!(done["kind"], "done");
        assert!(done["backup"].is_null());
    }
}
