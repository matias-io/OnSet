//! The show screens read and write the visuals' settings file (`onset-settings`), the same
//! `config.toml` the visuals load when they start.
use std::path::Path;

use onset_settings::Config;
use serde_json::{Value, json};

/// The settings as the screens see them, with whether they may be saved.
fn describe(config: &Config, path: &Path) -> Result<Value, String> {
    let mut settings = serde_json::to_value(config).map_err(|e| e.to_string())?;
    // `read_only` is not part of the file; the screens get it beside the settings.
    if let Some(map) = settings.as_object_mut() {
        map.remove("read_only");
    }
    Ok(json!({
        "settings": settings,
        "read_only": config.read_only,
        "path": path.display().to_string(),
    }))
}

fn load(path: &Path) -> Result<Value, String> {
    describe(&Config::load_from(path), path)
}

/// Saves `settings` over the file at `path`. Missing fields take their defaults; a file that
/// failed to parse is never overwritten, so the DJ's own edits are not lost.
fn save(path: &Path, settings: Value) -> Result<Value, String> {
    if Config::load_from(path).read_only {
        return Err(format!(
            "{} could not be read, so it is not saved over; fix or remove it first",
            path.display()
        ));
    }
    let config: Config =
        serde_json::from_value(settings).map_err(|e| format!("Invalid show settings: {e}"))?;
    config.save_to(path).map_err(|e| e.to_string())?;
    describe(&config, path)
}

#[tauri::command]
pub fn show_settings() -> Result<Value, String> {
    load(&Config::path())
}

/// Saves the settings and hands them to a running show, which applies them at once; a new
/// audio input goes to the engine.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri commands take owned arguments
pub fn show_settings_save(
    settings: Value,
    visuals: tauri::State<'_, crate::visuals::Visuals>,
) -> Result<Value, String> {
    let before = Config::load_from(&Config::path());
    let saved = save(&Config::path(), settings)?;
    let config = Config::load_from(&Config::path());
    if config.audio_device != before.audio_device
        && let Some(engine) = crate::live::engine()
    {
        engine.command(onset_engine::EngineCommand::SetAudioDevice(
            config.audio_device.clone(),
        ));
    }
    crate::visuals::update(&visuals, config);
    Ok(saved)
}

/// The screens the show can go to. The settings name a screen by its Windows name
/// (`\\.\DISPLAY2`), which the visuals match, rather than by its place in the list.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri commands take owned arguments
pub fn show_monitors(app: tauri::AppHandle) -> Result<Value, String> {
    let primary = app
        .primary_monitor()
        .ok()
        .flatten()
        .and_then(|m| m.name().cloned());
    let monitors = app.available_monitors().map_err(|e| e.to_string())?;
    Ok(Value::Array(
        monitors
            .iter()
            .map(|m| {
                json!({
                    "name": m.name(),
                    "width": m.size().width,
                    "height": m.size().height,
                    "primary": m.name().is_some() && m.name() == primary.as_ref(),
                })
            })
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_screens_get_defaults_then_their_saved_changes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let first = load(&path).unwrap();
        assert_eq!(first["read_only"], false);
        assert_eq!(first["settings"]["scene"], "tunnel");
        assert!(first["settings"].get("read_only").is_none());

        let mut changed = first["settings"].clone();
        changed["scene"] = "warp".into();
        changed["show_hud"] = true.into();
        save(&path, changed).unwrap();
        let again = load(&path).unwrap();
        assert_eq!(again["settings"]["scene"], "warp");
        assert_eq!(again["settings"]["show_hud"], true);
    }

    #[test]
    fn missing_fields_take_defaults_and_bad_values_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let saved = save(&path, json!({"quality": "low"})).unwrap();
        assert_eq!(saved["settings"]["quality"], "low");
        assert_eq!(saved["settings"]["scene"], "tunnel");
        let bad = save(&path, json!({"quality": "extreme"})).unwrap_err();
        assert!(bad.contains("Invalid show settings"), "{bad}");
    }

    #[test]
    fn a_file_that_failed_to_parse_is_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "scene = 'ring'\nthis is = not [ toml").unwrap();
        assert_eq!(load(&path).unwrap()["read_only"], true);
        assert!(save(&path, json!({})).is_err());
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("not [ toml")
        );
    }
}
