//! OnSet desktop: the preparation screens and the live view in one Tauri app.
//!
//! The screens are the React UI in `ui/`: `bridge.js` gives them the `window.onset` API,
//! backed by the commands below. The
//! Python service runs as a sidecar ([`sidecar`]); OnSet's engine feeds the live view
//! ([`live`]). The DJ's rules carry over: service commands are allow-listed, audio is served
//! only for files the service names, and nothing here writes to rekordbox by itself.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ai;
mod calibration;
mod chatgpt;
mod live;
mod providers;
mod show;
mod show_settings;
mod sidecar;
mod visuals;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::{Value, json};
use tauri::http::{Response, StatusCode, header};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, WebviewUrl, WebviewWindowBuilder};

use crate::chatgpt::ChatGpt;
use crate::sidecar::{Runtime, Service};

const BRIDGE: &str = include_str!("bridge.js");

/// Service methods the screens may call.
const METHODS: &[&str] = &[
    "workspace-health",
    "workspace-backups",
    "workspace-backup",
    "workspace-restore-preview",
    "workspace-restore",
    "doctor",
    "tracks",
    "track",
    "import",
    "analyze",
    "edit",
    "review",
    "review-bulk",
    "undo",
    "jobs",
    "job-control",
    "catalogue",
    "transitions",
    "plan",
    "validate-plan",
    "import-plan",
    "import-plan-file",
    "preview",
    "export",
    "calibrate-energy",
    "energy-judgment",
    "benchmark",
    "evidence",
    "ai-context",
    "enrichment-import",
    "rekordbox-preview",
    "rekordbox-apply",
    "restore",
    "rekordbox-import",
    "manifests",
    "settings",
    "worker-setup",
    "sets",
    "reliability",
    "replan-order",
    "transition",
    "benchmark-label",
    "relocate",
    "transition-alternatives",
    "replan",
    "review-queue",
    "plan-job",
    "transitions-job",
    "import-workspace",
    "relocate-cache",
    "genre-suggestions",
];

const STEMS: &[&str] = &["drums", "bass", "vocals", "other"];

struct AppState {
    service: Service,
    chatgpt: Arc<ChatGpt>,
    /// Audio files the screens may play, by the id in their `onset-media` URL.
    media: Mutex<HashMap<String, PathBuf>>,
    next_media: AtomicU64,
}

impl AppState {
    fn media_url(&self, file: PathBuf) -> String {
        let id = format!("m{}", self.next_media.fetch_add(1, Ordering::Relaxed));
        self.media.lock().insert(id.clone(), file);
        // Windows webviews reach custom schemes as http://<scheme>.localhost/.
        format!("http://onset-media.localhost/{id}")
    }
}

/// The service's workspace folder, after moving a pre-OnSet (Autocue) workspace into it.
fn workspace_home() -> PathBuf {
    let home = onset_workspace::default_home().unwrap_or_else(|| PathBuf::from("workspace"));
    match onset_workspace::move_legacy_home(&home) {
        Ok(true) => {
            tracing::info!(home = %home.display(), "moved the Autocue workspace into OnSet");
        }
        Ok(false) => {}
        Err(e) => tracing::warn!(
            "could not move the Autocue workspace to {}: {e}",
            home.display()
        ),
    }
    home
}

#[tauri::command]
async fn service_call(
    app: AppHandle,
    state: State<'_, AppState>,
    method: String,
    params: Value,
) -> Result<Value, String> {
    match method.as_str() {
        "quit" => {
            app.exit(0);
            return Ok(json!({"quitting": true}));
        }
        "open-source" => {
            let url = params
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let ok = url.starts_with("https://")
                && !url["https://".len()..]
                    .split(['/', '?', '#'])
                    .next()
                    .unwrap_or_default()
                    .contains('@');
            if !ok {
                return Err("Source links must use HTTPS without credentials".into());
            }
            open::that_detached(url).map_err(|e| e.to_string())?;
            return Ok(json!({"opened": true}));
        }
        _ => {}
    }
    if !METHODS.contains(&method.as_str()) {
        return Err("Unknown service command".into());
    }
    if method == "import" && !params.get("paths").is_some_and(Value::is_array) {
        return Err("Expected import paths".into());
    }
    state.service.call(&method, params).await
}

#[tauri::command]
async fn pick_paths(window: tauri::WebviewWindow, kind: String) -> Result<Vec<String>, String> {
    let dialog = rfd::AsyncFileDialog::new().set_parent(&window);
    let picked = if kind == "folder" {
        dialog.pick_folder().await.map(|f| vec![f])
    } else if kind == "workspace" {
        // One exported workspace (JSON or SQLite) from another computer.
        dialog
            .add_filter("OnSet workspace export", &["json", "db"])
            .pick_file()
            .await
            .map(|f| vec![f])
    } else {
        let dialog = if kind == "json" {
            dialog.add_filter("JSON", &["json"])
        } else {
            dialog.add_filter(
                "Music and analysis",
                &[
                    "wav", "flac", "mp3", "m4a", "aiff", "aif", "ogg", "opus", "json",
                ],
            )
        };
        dialog.pick_files().await
    };
    Ok(picked
        .unwrap_or_default()
        .iter()
        .map(|f| f.path().to_string_lossy().into_owned())
        .collect())
}

#[tauri::command]
async fn media_url(
    state: State<'_, AppState>,
    id: String,
    stem: Option<String>,
) -> Result<String, String> {
    if let Some(s) = &stem
        && !STEMS.contains(&s.as_str())
    {
        return Err("Unknown stem".into());
    }
    let track = state.service.call("track", json!({"id": id})).await?;
    let file = match &stem {
        Some(s) => track.pointer(&format!("/analysis/deep/stems/{s}")),
        None => track.get("path"),
    }
    .and_then(Value::as_str)
    .map(PathBuf::from)
    .ok_or("This stem is unavailable; complete local deep analysis first")?;
    if !file.exists() {
        return Err(if stem.is_some() {
            "Local stem files are missing; rerun deep analysis".into()
        } else {
            "Audio is missing; use Locate moved audio to verify its new path".into()
        });
    }
    Ok(state.media_url(file))
}

#[tauri::command]
async fn preview_media(
    state: State<'_, AppState>,
    id: String,
    click: bool,
) -> Result<Value, String> {
    let mut result = state
        .service
        .call("preview", json!({"id": id, "click": click}))
        .await?;
    let path = result
        .get("path")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or("Preview has no audio")?;
    let url = state.media_url(path);
    if let Some(obj) = result.as_object_mut() {
        obj.insert("url".into(), url.into());
    }
    Ok(result)
}

#[tauri::command]
async fn export_file(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
    params: Value,
) -> Result<Value, String> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("onset.json");
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_parent(&window)
        .set_file_name(name)
        .save_file()
        .await
    else {
        return Ok(Value::Null);
    };
    let path = file.path().to_path_buf();
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
    {
        return Err(
            "PDF set sheets are not in the OnSet desktop build yet; save as HTML or JSON".into(),
        );
    }
    let mut params = params;
    if let Some(obj) = params.as_object_mut() {
        obj.insert("path".into(), path.to_string_lossy().into_owned().into());
    }
    state.service.call("export", params).await
}

/// Why a ChatGPT set plan's tracks fail validation, in terms of the context it was given:
/// how many IDs came back, which are unknown or repeated, and a sample of the unknown values.
fn plan_diagnosis(plan: &Value, context_tracks: &[Value]) -> String {
    let known: std::collections::HashSet<&str> = context_tracks
        .iter()
        .filter_map(|t| t["id"].as_str())
        .collect();
    let ids: Vec<&str> = plan
        .get("track_ids")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|v| v.as_str().unwrap_or("<not a string>"))
        .collect();
    let mut seen = std::collections::HashSet::new();
    let repeated: Vec<&str> = ids.iter().copied().filter(|i| !seen.insert(*i)).collect();
    let unknown: Vec<&str> = ids.iter().copied().filter(|i| !known.contains(i)).collect();
    let transitions = plan
        .get("transition_ids")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    format!(
        "{} track ids for {} context tracks, {transitions} transition ids; {} unknown (first: {:?}), {} repeated (first: {:?})",
        ids.len(),
        known.len(),
        unknown.len(),
        unknown.iter().take(3).collect::<Vec<_>>(),
        repeated.len(),
        repeated.iter().take(3).collect::<Vec<_>>(),
    )
}

/// Runs blocking account work (network, the sign-in wait) off the async runtime.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

/// "Continue with ChatGPT", with sign-in, model discovery and sign-out.
#[tauri::command]
async fn chatgpt_account(
    app: AppHandle,
    state: State<'_, AppState>,
    action: String,
    params: Value,
) -> Result<Value, String> {
    if action != "status" {
        tracing::info!(action, "ChatGPT account action");
    }
    let outcome = account_action(app, &state, action.clone(), params).await;
    if let Err(e) = &outcome {
        tracing::warn!(action, "ChatGPT account action failed: {e}");
    }
    outcome
}

async fn account_action(
    app: AppHandle,
    state: &AppState,
    action: String,
    params: Value,
) -> Result<Value, String> {
    let chat = state.chatgpt.clone();
    let id = params.get("id").and_then(Value::as_str).map(str::to_owned);
    match action.as_str() {
        "status" => Ok(chat.status()),
        "sign-in" => {
            blocking(move || {
                chat.sign_in(id.as_deref(), |url| {
                    open::that_detached(url).map_err(|e| e.to_string())
                })
            })
            .await
        }
        "cancel" => {
            chat.cancel();
            Ok(Value::Null)
        }
        "select" => chat.select(&id.ok_or("Unknown account")?),
        "disconnect" => chat.disconnect(&id.ok_or("Unknown account")?),
        "models" => blocking(move || chat.models()).await,
        "model" => {
            let id = id.ok_or("Model unavailable")?;
            blocking(move || chat.choose_model(&id)).await
        }
        "manage" => {
            open::that_detached("https://chatgpt.com/#settings").map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        "request" => ai_request(&app, state, chat, &params).await,
        _ => Err("Unknown account action".into()),
    }
}

/// A set plan or enrichment from ChatGPT, imported through the service. A plan the service
/// rejects is replaced by the service's own alternatives, with the reason.
async fn ai_request(
    app: &AppHandle,
    state: &AppState,
    chat: Arc<ChatGpt>,
    params: &Value,
) -> Result<Value, String> {
    let brief = params.get("brief").cloned().unwrap_or(Value::Null);
    let task = params.get("task").cloned().unwrap_or(Value::Null);
    let context = state
        .service
        .call(
            "ai-context",
            json!({"track_ids": params.get("track_ids"), "brief": brief}),
        )
        .await?;
    let mut ask = context.clone();
    if let Some(obj) = ask.as_object_mut() {
        obj.insert("task".into(), task.clone());
    }
    let model = params
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let research = params
        .get("research")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let events = app.clone();
    let mut result = blocking(move || {
        chat.request(&ask, &model, research, |delta| {
            let _ = events.emit(
                "service:event",
                json!({"method": "ai-progress", "params": {"delta": delta}}),
            );
        })
    })
    .await?;
    let tracks = context
        .get("tracks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if task == "enrichment" {
        if let Some(obj) = result.as_object_mut() {
            let ids: Vec<Value> = tracks.iter().map(|t| t["id"].clone()).collect();
            obj.insert("allowed_ids".into(), ids.into());
        }
        return state.service.call("enrichment-import", result).await;
    }
    if let Some(obj) = result.as_object_mut() {
        let revisions: serde_json::Map<String, Value> = tracks
            .iter()
            .filter_map(|t| Some((t["id"].as_str()?.to_owned(), t["revision"].clone())))
            .collect();
        obj.insert("brief".into(), brief.clone());
        obj.insert("revisions".into(), revisions.into());
    }
    match state
        .service
        .call("import-plan", json!({"plan": result.clone()}))
        .await
    {
        Ok(v) => Ok(v),
        Err(rejected) => {
            tracing::warn!(
                "ChatGPT plan rejected ({rejected}): {}",
                plan_diagnosis(&result, &tracks)
            );
            let replacement = state.service.call("plan", json!({"brief": brief})).await?;
            Ok(json!({
                "rejected_ai": rejected,
                "alternatives": replacement.get("plans"),
                "conflicts": replacement.get("conflicts"),
            }))
        }
    }
}

/// Which AI providers are available: each CLI's path and version, or how to install it, and
/// whether a ChatGPT account is connected.
#[tauri::command]
async fn providers_status(state: State<'_, AppState>) -> Result<Value, String> {
    let chatgpt = state.chatgpt.status();
    let clis = blocking(|| {
        Ok(providers::Provider::CLIS
            .into_iter()
            .map(providers::detect::find)
            .collect::<Vec<_>>())
    })
    .await?;
    Ok(json!({
        "clis": clis,
        "chatgpt": {
            "connected": chatgpt["active"].is_string(),
            "plan_enabled": chatgpt["accounts"].as_array().is_some_and(|a| a.iter().any(|x| x["plan_enabled"] == true)),
        },
    }))
}

/// One line per provider for the screens: how it went, never tokens or raw output.
fn provider_report(
    provider: providers::Provider,
    outcome: &Result<providers::Answer, String>,
) -> Value {
    match outcome {
        Ok(a) => json!({"provider": provider.name(), "ok": true, "seconds": a.seconds,
                        "consulted": a.consulted.len()}),
        Err(e) => {
            tracing::warn!(provider = provider.name(), "AI request failed: {e}");
            json!({"provider": provider.name(), "ok": false, "error": e})
        }
    }
}

/// Genre, mood, instrumentation, release and description for the chosen tracks from each
/// chosen provider, imported as cited claims labelled with their provider.
#[tauri::command]
async fn ai_details(
    state: State<'_, AppState>,
    providers: Vec<providers::Provider>,
    track_ids: Vec<String>,
    web: bool,
) -> Result<Value, String> {
    if providers.is_empty() || track_ids.is_empty() {
        return Err("Choose at least one AI provider and one track".into());
    }
    let context = state
        .service
        .call("ai-context", json!({"track_ids": track_ids}))
        .await?;
    let chat = state.chatgpt.clone();
    let ctx = context.clone();
    let results = blocking(move || {
        Ok(ai::ask_all(
            &providers,
            &|_| ai::details_ask(&ctx, web),
            &chat,
        ))
    })
    .await?;
    let report: Vec<Value> = results
        .iter()
        .map(|(p, r)| provider_report(*p, r))
        .collect();
    let answers: Vec<providers::Answer> = results.into_iter().filter_map(|(_, r)| r.ok()).collect();
    if answers.is_empty() {
        return Ok(json!({"providers": report, "imported": Value::Null}));
    }
    // Claims other providers made earlier stay; only these providers' are replaced.
    let mut existing = std::collections::BTreeMap::new();
    for id in &track_ids {
        let ev = state.service.call("evidence", json!({"id": id})).await?;
        if ev["enrichment"].is_object() {
            existing.insert(id.clone(), ev["enrichment"].clone());
        }
    }
    let mut merged = ai::merge_details(&answers, web, &existing);
    merged["allowed_ids"] = json!(track_ids);
    let imported = state.service.call("enrichment-import", merged).await?;
    Ok(json!({"providers": report, "imported": imported}))
}

/// The planner's best set beside each AI's own pick and its critique of the planner's.
/// Every AI set is checked by the service's validator; valid ones are saved as sets.
#[tauri::command]
async fn ai_compare_sets(
    state: State<'_, AppState>,
    providers: Vec<providers::Provider>,
    brief: Value,
    plans: Vec<Value>,
) -> Result<Value, String> {
    let planner = plans
        .first()
        .cloned()
        .ok_or("Find sets with the planner first")?;
    if providers.is_empty() {
        return Err("Choose at least one AI provider".into());
    }
    let ids = ai::shortlist(&plans, &brief, 40);
    let mut short_brief = brief.clone();
    short_brief["track_ids"] = json!(ids);
    let context = state
        .service
        .call(
            "ai-context",
            json!({"track_ids": ids, "brief": short_brief}),
        )
        .await?;
    let planner_choice = json!({
        "track_ids": planner["track_ids"], "transition_ids": planner["transition_ids"],
        "duration_seconds": planner["duration_seconds"], "explanation": planner["explanation"],
    });
    let chat = state.chatgpt.clone();
    let ctx = context.clone();
    let results = blocking(move || {
        Ok(ai::ask_all(
            &providers,
            &|_| ai::set_ask(&ctx, &planner_choice),
            &chat,
        ))
    })
    .await?;
    let revisions: serde_json::Map<String, Value> = context["tracks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|t| Some((t["id"].as_str()?.to_owned(), t["revision"].clone())))
        .collect();
    let planner_ids: Vec<String> = planner["track_ids"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    let mut picks = Vec::new();
    for (provider, outcome) in results {
        let mut row = provider_report(provider, &outcome);
        if let Ok(answer) = outcome {
            let ai_ids: Vec<String> = answer.value["track_ids"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect();
            let plan = json!({
                "schema_version": 1, "track_ids": answer.value["track_ids"],
                "transition_ids": answer.value["transition_ids"], "brief": short_brief,
                "explanation": answer.value["explanation"], "revisions": revisions,
            });
            row["explanation"] = answer.value["explanation"].clone();
            row["critique"] = answer.value["critique"].clone();
            row["prefers"] = answer.value["prefers"].clone();
            row["track_ids"] = json!(ai_ids);
            row["agreement"] = ai::agreement(&planner_ids, &ai_ids);
            match state
                .service
                .call("import-plan", json!({"plan": plan}))
                .await
            {
                Ok(saved) => {
                    row["valid"] = true.into();
                    row["set"] = saved;
                }
                Err(reason) => {
                    tracing::warn!(provider = provider.name(), "AI set rejected: {reason}");
                    row["valid"] = false.into();
                    row["reason"] = reason.into();
                }
            }
        }
        picks.push(row);
    }
    Ok(json!({"planner": planner, "shortlist": ids, "ai": picks}))
}

/// Where the first full groove, breakdown, later drop and outro are, read by each chosen
/// provider from the bar-gridded spectrogram, or with `audio` by Gemini from the audio file.
/// Sending audio is a deliberate choice made for this one track.
#[tauri::command]
async fn ai_read_structure(
    state: State<'_, AppState>,
    providers: Vec<providers::Provider>,
    track_id: String,
    audio: bool,
) -> Result<Value, String> {
    if providers.is_empty() {
        return Err("Choose at least one AI provider".into());
    }
    if audio && providers.iter().any(|p| *p != providers::Provider::Gemini) {
        return Err("Only Gemini listens to audio; choose Gemini alone for an audio read".into());
    }
    if providers.contains(&providers::Provider::ChatGpt) {
        return Err("ChatGPT can't read images here; choose Claude, Codex or Gemini".into());
    }
    let image = state
        .service
        .call("spectrogram-image", json!({"id": track_id}))
        .await?;
    let attachment = if audio {
        let track = state.service.call("track", json!({"id": track_id})).await?;
        PathBuf::from(
            track["path"]
                .as_str()
                .ok_or("This track has no audio file")?,
        )
    } else {
        PathBuf::from(image["path"].as_str().ok_or("No spectrogram image")?)
    };
    if !attachment.exists() {
        return Err("The audio file is missing; locate it first".into());
    }
    let chat = state.chatgpt.clone();
    let img = image.clone();
    let results = blocking(move || {
        Ok(ai::ask_all(
            &providers,
            &|_| ai::structure_ask(&img, attachment.clone(), audio),
            &chat,
        ))
    })
    .await?;
    let mut reads = Vec::new();
    for (provider, outcome) in results {
        let mut row = provider_report(provider, &outcome);
        if let Ok(answer) = outcome {
            match state
                .service
                .call(
                    "observations-import",
                    ai::structure_import(&answer, &image, audio),
                )
                .await
            {
                Ok(stored) => row["observations"] = stored["observations"].clone(),
                Err(reason) => {
                    row["ok"] = false.into();
                    row["error"] = reason.into();
                }
            }
        }
        reads.push(row);
    }
    Ok(
        json!({"track_id": track_id, "medium": if audio { "audio" } else { "spectrogram" }, "reads": reads}),
    )
}

/// Checks that one CLI answers: a tiny request with no web access. A failure usually means
/// the tool is not signed in.
#[tauri::command]
async fn providers_test(provider: providers::Provider) -> Result<Value, String> {
    let answer = blocking(move || {
        providers::ask_cli(
            provider,
            &providers::Ask {
                instructions: "Name the main genre of the track in the evidence.".into(),
                evidence: json!({"title": "Sandstorm", "artist": "Darude"}),
                schema: json!({"type": "object", "additionalProperties": false,
                    "required": ["genre"], "properties": {"genre": {"type": "string"}}}),
                web: false,
                images: Vec::new(),
                model: None,
                timeout: Duration::from_secs(120),
            },
        )
    })
    .await;
    match &answer {
        Ok(a) => tracing::info!(
            provider = provider.name(),
            seconds = a.seconds,
            "provider test passed"
        ),
        Err(e) => tracing::warn!(provider = provider.name(), "provider test failed: {e}"),
    }
    let a = answer?;
    Ok(json!({"provider": a.provider, "seconds": a.seconds, "answer": a.value}))
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri commands take owned arguments
fn open_live_window(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("live") {
        w.show().map_err(|e| e.to_string())?;
        return w.set_focus().map_err(|e| e.to_string());
    }
    live_window(&app).map_err(|e| e.to_string())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri commands take owned arguments
fn set_live_on_top(app: AppHandle, on_top: bool) -> Result<(), String> {
    app.get_webview_window("live")
        .ok_or("The live window is closed")?
        .set_always_on_top(on_top)
        .map_err(|e| e.to_string())
}

/// Only the app's own pages load in its windows; links never navigate them away.
fn allowed_navigation(url: &tauri::Url) -> bool {
    matches!(url.scheme(), "tauri")
        || url.host_str() == Some("tauri.localhost")
        || (cfg!(debug_assertions) && url.host_str() == Some("127.0.0.1"))
}

fn main_window(app: &AppHandle) -> tauri::Result<()> {
    WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("OnSet")
        .inner_size(1440.0, 950.0)
        .min_inner_size(1060.0, 700.0)
        .initialization_script(BRIDGE)
        .on_navigation(allowed_navigation)
        .build()?;
    Ok(())
}

fn live_window(app: &AppHandle) -> tauri::Result<()> {
    WebviewWindowBuilder::new(app, "live", WebviewUrl::App("live.html".into()))
        .title("OnSet Live")
        .inner_size(520.0, 380.0)
        .min_inner_size(360.0, 260.0)
        .initialization_script(BRIDGE)
        .on_navigation(allowed_navigation)
        .build()?;
    Ok(())
}

/// Serves `http://onset-media.localhost/<id>` for registered files, with byte ranges so
/// the audio elements can seek.
fn serve_media(app: &AppHandle, request: &tauri::http::Request<Vec<u8>>) -> Response<Vec<u8>> {
    let not_found = |why: &str| {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(why.as_bytes().to_vec())
            .expect("static response")
    };
    let id = request.uri().path().trim_start_matches('/');
    let Some(file) = app.state::<AppState>().media.lock().get(id).cloned() else {
        return not_found("Unknown media");
    };
    let Ok(len) = std::fs::metadata(&file).map(|m| m.len()) else {
        return not_found("Audio is missing");
    };
    let mime = match file
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("mp3") => "audio/mpeg",
        Some("flac") => "audio/flac",
        Some("wav") => "audio/wav",
        Some("m4a") => "audio/mp4",
        Some("aac") => "audio/aac",
        Some("ogg" | "opus") => "audio/ogg",
        Some("aif" | "aiff") => "audio/aiff",
        _ => "application/octet-stream",
    };
    let range = byte_range(
        request
            .headers()
            .get(header::RANGE)
            .and_then(|v| v.to_str().ok()),
        len,
    );
    let builder = Response::builder()
        .header(header::CONTENT_TYPE, mime)
        .header(header::ACCEPT_RANGES, "bytes");
    let (status, start, end) = match range {
        ByteRange::Whole => (StatusCode::OK, 0, len.saturating_sub(1)),
        ByteRange::Part(start, end) => (StatusCode::PARTIAL_CONTENT, start, end),
        ByteRange::Unsatisfiable => {
            return builder
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header(header::CONTENT_RANGE, format!("bytes */{len}"))
                .body(Vec::new())
                .expect("valid media response");
        }
    };
    // Read only the requested bytes: seeking in a long FLAC must not reload the whole file.
    let body = if len == 0 {
        Ok(Vec::new())
    } else {
        read_span(&file, start, end)
    };
    let Ok(body) = body else {
        return not_found("Audio is missing");
    };
    let builder = if status == StatusCode::PARTIAL_CONTENT {
        builder.header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{len}"))
    } else {
        builder
    };
    builder
        .status(status)
        .body(body)
        .expect("valid media response")
}

#[derive(Debug, PartialEq, Eq)]
enum ByteRange {
    Whole,
    Part(u64, u64),
    Unsatisfiable,
}

/// Parses a single `bytes=` range (open, closed or suffix) against a file length.
fn byte_range(header: Option<&str>, len: u64) -> ByteRange {
    let Some(header) = header else {
        return ByteRange::Whole;
    };
    let parsed = header.trim().strip_prefix("bytes=").and_then(|v| {
        let (a, b) = v.split_once('-')?;
        let last = len.checked_sub(1)?;
        if a.is_empty() {
            let suffix: u64 = b.parse().ok().filter(|n| *n > 0)?;
            return Some((len.saturating_sub(suffix), last));
        }
        let start: u64 = a.parse().ok()?;
        let end = if b.is_empty() {
            last
        } else {
            b.parse::<u64>().ok()?.min(last)
        };
        (start <= end).then_some((start, end))
    });
    parsed.map_or(ByteRange::Unsatisfiable, |(s, e)| ByteRange::Part(s, e))
}

fn read_span(file: &std::path::Path, start: u64, end: u64) -> std::io::Result<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut handle = std::fs::File::open(file)?;
    handle.seek(SeekFrom::Start(start))?;
    let mut body = vec![0; usize::try_from(end - start + 1).map_err(std::io::Error::other)?];
    handle.read_exact(&mut body)?;
    Ok(body)
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let runtime = match Runtime::locate() {
        Ok(r) => r,
        Err(why) => {
            rfd::MessageDialog::new()
                .set_level(rfd::MessageLevel::Error)
                .set_title("OnSet")
                .set_description(&why)
                .show();
            std::process::exit(1);
        }
    };
    let home = workspace_home();
    if let Err(e) = std::fs::create_dir_all(&home) {
        tracing::error!("cannot create {}: {e}", home.display());
    }

    let app = tauri::Builder::default()
        .register_asynchronous_uri_scheme_protocol("onset-media", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            std::thread::spawn(move || responder.respond(serve_media(&app, &request)));
        })
        .invoke_handler(tauri::generate_handler![
            service_call,
            pick_paths,
            media_url,
            preview_media,
            export_file,
            chatgpt_account,
            open_live_window,
            providers_status,
            providers_test,
            ai_details,
            ai_compare_sets,
            ai_read_structure,
            set_live_on_top,
            visuals::visuals_start,
            visuals::visuals_end,
            visuals::visuals_running,
            visuals::visuals_preview,
            show::show_mode,
            show_settings::show_settings,
            show_settings::show_settings_save,
            show_settings::show_monitors,
            show_settings::show_catalogue,
            show_settings::show_logo,
            calibration::calibration_status,
            calibration::calibration_start,
            calibration::calibration_cancel,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            let events = handle.clone();
            let service = Service::start(&runtime, &home, move |event| {
                let _ = events.emit("service:event", event);
            })?;
            tracing::info!(
                python = %runtime.python.display(),
                home = %home.display(),
                "service started"
            );
            app.manage(AppState {
                service,
                chatgpt: Arc::new(ChatGpt::load(&home)),
                media: Mutex::default(),
                next_media: AtomicU64::new(1),
            });
            app.manage(visuals::Visuals::default());
            main_window(&handle)?;
            live_window(&handle)?;
            live::spawn(handle);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("OnSet desktop failed to start");

    app.run(|app, event| {
        if let RunEvent::Exit = event
            && let Some(state) = app.try_state::<AppState>()
        {
            // Let the service checkpoint its jobs before the app exits.
            state.service.stop(Duration::from_secs(30));
        }
    });
}

#[cfg(test)]
mod media_tests {
    use super::{ByteRange, byte_range, plan_diagnosis, read_span};

    #[test]
    fn plan_diagnosis_names_unknown_and_repeated_tracks() {
        let tracks = vec![
            serde_json::json!({"id": "a"}),
            serde_json::json!({"id": "b"}),
        ];
        let plan = serde_json::json!({"track_ids": ["a", "Stop It", "a"], "transition_ids": ["t"]});
        assert_eq!(
            plan_diagnosis(&plan, &tracks),
            "3 track ids for 2 context tracks, 1 transition ids; 1 unknown (first: [\"Stop It\"]), 1 repeated (first: [\"a\"])"
        );
    }

    #[test]
    fn parses_open_closed_suffix_and_unsatisfiable_ranges() {
        assert_eq!(byte_range(None, 100), ByteRange::Whole);
        assert_eq!(byte_range(Some("bytes=0-"), 100), ByteRange::Part(0, 99));
        assert_eq!(
            byte_range(Some("bytes=10-19"), 100),
            ByteRange::Part(10, 19)
        );
        assert_eq!(
            byte_range(Some("bytes=90-500"), 100),
            ByteRange::Part(90, 99)
        );
        assert_eq!(byte_range(Some("bytes=-10"), 100), ByteRange::Part(90, 99));
        assert_eq!(
            byte_range(Some("bytes=100-"), 100),
            ByteRange::Unsatisfiable
        );
        assert_eq!(byte_range(Some("bytes=5-2"), 100), ByteRange::Unsatisfiable);
        assert_eq!(byte_range(Some("items=0-1"), 100), ByteRange::Unsatisfiable);
        assert_eq!(byte_range(Some("bytes=0-"), 0), ByteRange::Unsatisfiable);
    }

    #[test]
    fn reads_only_the_requested_span() {
        let file = std::env::temp_dir().join(format!("onset-media-{}.bin", std::process::id()));
        std::fs::write(&file, (0u8..100).collect::<Vec<_>>()).unwrap();
        assert_eq!(
            read_span(&file, 40, 49).unwrap(),
            (40u8..50).collect::<Vec<_>>()
        );
        std::fs::remove_file(file).unwrap();
    }
}
