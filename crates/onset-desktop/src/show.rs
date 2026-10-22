//! Show mode (plan O4): while the visuals run a show, preparation steps aside. The service
//! pauses its jobs at their next checkpoint and refuses rekordbox writes, and its process
//! drops to below-normal priority in Windows' efficiency mode, so analysis can't take
//! processor time the visuals and the live link need. Ending the show reverses both.
use serde_json::{Value, json};
use tauri::State;

use crate::AppState;
use crate::sidecar::Service;

/// Starts or ends show mode. Returns the service's answer: `{show, jobs}`, the jobs it
/// paused or resumed.
#[tauri::command]
pub async fn show_mode(state: State<'_, AppState>, on: bool) -> Result<Value, String> {
    set(&state.service, on).await
}

/// Starts or ends show mode; the projector calls this when a show starts and ends.
pub async fn set(service: &Service, on: bool) -> Result<Value, String> {
    let answer = service.call("show-mode", json!({ "on": on })).await?;
    // Jobs are already waiting; a priority that can't be changed is logged, not fatal.
    if let Err(e) = service.set_background(on) {
        tracing::warn!("{e}");
    }
    tracing::info!(on, jobs = %answer["jobs"], "show mode");
    Ok(answer)
}
