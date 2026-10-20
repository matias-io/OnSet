//! The full-screen visuals (`onset.exe`, the `onset-visuals` crate) ship beside the desktop
//! app and start from the OnSet Live window. They read rekordbox and the `offsets/` folder
//! beside them on their own; this only starts the program, once.
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::Mutex;

use tauri::State;

/// The visuals program this app started, while it runs.
#[derive(Default)]
pub struct Visuals(Mutex<Option<Child>>);

const PROGRAM: &str = "onset.exe";

/// `onset.exe` beside this executable: in the install folder, and in `target/<profile>`
/// for a development build once `onset-visuals` is built.
fn program_beside(exe: &Path) -> Option<PathBuf> {
    let path = exe.parent()?.join(PROGRAM);
    path.is_file().then_some(path)
}

/// Starts the visuals unless the ones this app started are still running. Returns
/// `"started"` or `"running"`.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri commands take owned arguments
pub fn visuals_start(visuals: State<'_, Visuals>) -> Result<&'static str, String> {
    let mut child = visuals.0.lock().map_err(|e| e.to_string())?;
    if let Some(c) = child.as_mut()
        && matches!(c.try_wait(), Ok(None))
    {
        return Ok("running");
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let program = program_beside(&exe).ok_or_else(|| {
        format!(
            "The visuals program ({PROGRAM}) is not installed beside OnSet in {}",
            exe.parent().map_or_else(String::new, |p| p.display().to_string())
        )
    })?;
    let mut command = Command::new(&program);
    command.current_dir(program.parent().unwrap_or_else(|| Path::new(".")));
    // `onset.exe` is a console program (its command-line options print there); started
    // from the app it needs no console window of its own.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let started = command
        .spawn()
        .map_err(|e| format!("Cannot start {}: {e}", program.display()))?;
    tracing::info!(program = %program.display(), pid = started.id(), "visuals started");
    *child = Some(started);
    Ok("started")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_program_is_found_beside_the_app_only() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("onset-desktop.exe");
        assert_eq!(program_beside(&exe), None, "not installed yet");
        std::fs::write(dir.path().join(PROGRAM), b"").unwrap();
        assert_eq!(program_beside(&exe), Some(dir.path().join(PROGRAM)));
    }
}
