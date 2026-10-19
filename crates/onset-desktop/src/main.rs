//! OnSet desktop: Autocue's preparation screens and the live view in one Tauri app.
//!
//! The screens are Autocue's React UI, unchanged: `bridge.js` gives them the same
//! `window.autocue` API the Electron preload did, backed by the commands below. Autocue's
//! Python service runs as a sidecar ([`sidecar`]); OnSet's engine feeds the live view
//! ([`live`]). The DJ's rules carry over: service commands are allow-listed, audio is served
//! only for files the service names, and nothing here writes to rekordbox by itself.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod live;
mod sidecar;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::{Value, json};
use tauri::http::{Response, StatusCode, header};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, WebviewUrl, WebviewWindowBuilder};

use crate::sidecar::{Runtime, Service};

const BRIDGE: &str = include_str!("bridge.js");

/// Service methods the screens may call, as the Electron shell allowed them.
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
];

const STEMS: &[&str] = &["drums", "bass", "vocals", "other"];

struct AppState {
    service: Service,
    /// Audio files the screens may play, by the id in their `autocue-audio` URL.
    media: Mutex<HashMap<String, PathBuf>>,
    next_media: AtomicU64,
}

impl AppState {
    fn media_url(&self, file: PathBuf) -> String {
        let id = format!("m{}", self.next_media.fetch_add(1, Ordering::Relaxed));
        self.media.lock().insert(id.clone(), file);
        // Windows webviews reach custom schemes as http://<scheme>.localhost/.
        format!("http://autocue-audio.localhost/{id}")
    }
}

/// `%AUTOCUE_HOME%`, else `%LOCALAPPDATA%\Autocue`, as Autocue resolves its workspace.
fn autocue_home() -> PathBuf {
    std::env::var_os("AUTOCUE_HOME").map_or_else(
        || {
            std::env::var_os("LOCALAPPDATA")
                .map_or_else(|| PathBuf::from("."), PathBuf::from)
                .join("Autocue")
        },
        PathBuf::from,
    )
}

#[tauri::command]
async fn autocue_call(
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
async fn autocue_pick(window: tauri::WebviewWindow, kind: String) -> Result<Vec<String>, String> {
    let dialog = rfd::AsyncFileDialog::new().set_parent(&window);
    let picked = if kind == "folder" {
        dialog.pick_folder().await.map(|f| vec![f])
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
async fn autocue_media(
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
async fn autocue_preview_media(
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
async fn autocue_export(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
    params: Value,
) -> Result<Value, String> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("autocue.json");
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

/// ChatGPT sign-in moves from the Electron main process to Rust in a later step; until then
/// the screens see no accounts and a reason.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri commands take owned arguments
fn autocue_account(action: String) -> Result<Value, String> {
    const NOT_YET: &str = "ChatGPT sign-in is not in the OnSet desktop build yet; \
                           use the Autocue app for AI requests";
    match action.as_str() {
        "status" => Ok(json!({
            "active": null,
            "selected_model": null,
            "accounts": [],
            "unavailable": NOT_YET,
        })),
        _ => Err(NOT_YET.into()),
    }
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

/// Serves `http://autocue-audio.localhost/<id>` for registered files, with byte ranges so
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
    let Ok(bytes) = std::fs::read(&file) else {
        return not_found("Audio is missing");
    };
    let len = bytes.len();
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
        Some("ogg" | "opus") => "audio/ogg",
        Some("aif" | "aiff") => "audio/aiff",
        _ => "application/octet-stream",
    };
    let range = request
        .headers()
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("bytes="))
        .and_then(|v| {
            let (a, b) = v.split_once('-')?;
            let start: usize = a.parse().ok()?;
            let end: usize = if b.is_empty() {
                len.checked_sub(1)?
            } else {
                b.parse::<usize>().ok()?.min(len.checked_sub(1)?)
            };
            (start <= end).then_some((start, end))
        });
    let builder = Response::builder()
        .header(header::CONTENT_TYPE, mime)
        .header(header::ACCEPT_RANGES, "bytes");
    match range {
        Some((start, end)) => builder
            .status(StatusCode::PARTIAL_CONTENT)
            .header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{len}"))
            .body(bytes[start..=end].to_vec()),
        None => builder.status(StatusCode::OK).body(bytes),
    }
    .expect("valid media response")
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
    let home = autocue_home();
    if let Err(e) = std::fs::create_dir_all(&home) {
        tracing::error!("cannot create {}: {e}", home.display());
    }

    let app = tauri::Builder::default()
        .register_asynchronous_uri_scheme_protocol("autocue-audio", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            std::thread::spawn(move || responder.respond(serve_media(&app, &request)));
        })
        .invoke_handler(tauri::generate_handler![
            autocue_call,
            autocue_pick,
            autocue_media,
            autocue_preview_media,
            autocue_export,
            autocue_account,
            open_live_window,
            set_live_on_top,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            let events = handle.clone();
            let service = Service::start(&runtime, &home, move |event| {
                let _ = events.emit("autocue:event", event);
            })?;
            tracing::info!(
                python = %runtime.python.display(),
                home = %home.display(),
                "Autocue service started"
            );
            app.manage(AppState {
                service,
                media: Mutex::default(),
                next_media: AtomicU64::new(1),
            });
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
            // Let the service checkpoint its jobs, as Quit did in the Electron shell.
            state.service.stop(Duration::from_secs(30));
        }
    });
}
