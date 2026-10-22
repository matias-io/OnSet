//! The projector: the show runs in a window of this app (the `onset-visuals` library), on its
//! own thread, following the same engine as the live view. Starting a show switches on the
//! engine's audio capture and show mode; when the show ends, however it ends, both go off
//! again and the screens hear about it as an `onset:show` event.
use std::sync::{Arc, Mutex};

use onset_engine::EngineCommand;
use onset_settings::Config;
use onset_visuals::Projector;
use onset_visuals::app::AppOptions;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{AppState, live, show};

/// The projector thread, started with the first show and kept for the next.
#[derive(Default)]
pub struct Visuals(Mutex<Option<Arc<Projector>>>);

/// The options a show starts with: the saved settings, straight into the show, with this
/// app owning the settings file.
fn show_options(config: Config) -> AppOptions {
    AppOptions {
        config,
        monitor: None,
        scene: None,
        exit_after: None,
        bench: None,
        frame_latency: 2,
        settings_open: false,
        save_settings: false,
        screenshot: None,
        launcher: false,
        offsets_dir: live::offsets_dir(),
        cache_dir: live::cache_dir(),
        engine: live::engine(),
    }
}

fn projector(app: &AppHandle, visuals: &Visuals) -> Result<Arc<Projector>, String> {
    let mut slot = visuals.0.lock().map_err(|e| e.to_string())?;
    if let Some(p) = slot.as_ref() {
        return Ok(p.clone());
    }
    let handle = app.clone();
    let projector =
        Arc::new(Projector::spawn(move || ended(&handle)).map_err(|e| format!("{e:#}"))?);
    *slot = Some(projector.clone());
    Ok(projector)
}

/// After a show: no more listening to the room, preparation may resume.
fn ended(app: &AppHandle) {
    tracing::info!("show ended");
    if let Some(engine) = live::engine() {
        engine.command(EngineCommand::SetCapture(false));
    }
    let _ = app.emit("onset:show", json!({ "running": false }));
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        if let Err(e) = show::set(&state.service, false).await {
            tracing::warn!("show mode did not end: {e}");
        }
    });
}

/// Starts the show on the configured screen. Returns `"started"`, or `"running"` when a
/// show is already on.
#[tauri::command]
pub async fn visuals_start(
    app: AppHandle,
    visuals: State<'_, Visuals>,
    state: State<'_, AppState>,
) -> Result<&'static str, String> {
    let projector = projector(&app, &visuals)?;
    if projector.is_running() {
        return Ok("running");
    }
    let config = Config::load();
    if let Some(engine) = live::engine() {
        engine.command(EngineCommand::SetAudioDevice(config.audio_device.clone()));
        engine.command(EngineCommand::SetCapture(true));
    }
    // Preparation steps aside first; a service that cannot answer does not stop the show.
    if let Err(e) = show::set(&state.service, true).await {
        tracing::warn!("show mode did not start: {e}");
    }
    projector.start(show_options(config))?;
    tracing::info!("show started");
    let _ = app.emit("onset:show", json!({ "running": true }));
    Ok("started")
}

/// Hands a running show the settings just saved.
pub fn update(visuals: &Visuals, config: Config) {
    if let Ok(slot) = visuals.0.lock()
        && let Some(p) = slot.as_ref()
    {
        p.update(config);
    }
}

/// Ends the running show, if any.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri commands take owned arguments
pub fn visuals_end(visuals: State<'_, Visuals>) -> Result<(), String> {
    if let Some(p) = visuals.0.lock().map_err(|e| e.to_string())?.as_ref() {
        p.end();
    }
    Ok(())
}

/// Whether a show is on.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri commands take owned arguments
pub fn visuals_running(visuals: State<'_, Visuals>) -> Result<bool, String> {
    Ok(visuals
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .as_ref()
        .is_some_and(|p| p.is_running()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_show_goes_straight_to_the_screen_and_leaves_the_settings_file_to_the_app() {
        let opts = show_options(Config::default());
        assert!(
            !opts.launcher,
            "no launcher page: the app holds the settings"
        );
        assert!(!opts.save_settings, "the app owns config.toml");
        assert!(opts.bench.is_none() && opts.exit_after.is_none());
    }
}
