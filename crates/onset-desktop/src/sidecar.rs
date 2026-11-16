//! The Python service as a child process: one JSON-RPC message per line on stdin and
//! stdout, as `onset_service/rpc.py` speaks it. Replies resolve the matching call; messages
//! with a `method` are events, forwarded to the screens.
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde_json::{Value, json};
use tokio::sync::oneshot;

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>;

pub struct Service {
    child: Mutex<Child>,
    stdin: Mutex<Option<ChildStdin>>,
    /// The service's last line of error output: why it stopped, when it did.
    last_error: Arc<Mutex<String>>,
    next: AtomicU64,
    pending: Pending,
}

/// Where the Python service's source and interpreter are.
pub struct Runtime {
    pub python: PathBuf,
    pub root: PathBuf,
}

impl Runtime {
    /// `ONSET_SERVICE_ROOT` and `ONSET_PYTHON` if set; else an installed layout (`service/`
    /// and `runtime/python.exe` beside the executable); else the repository's `service/`
    /// with its `.venv` or `.runtime` interpreter, for development.
    pub fn locate() -> Result<Self, String> {
        let installed = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf));
        Self::locate_in(installed.as_deref())
    }

    fn locate_in(installed: Option<&Path>) -> Result<Self, String> {
        let bundled = installed.filter(|dir| dir.join("service").join("onset_service").is_dir());
        let root = std::env::var_os("ONSET_SERVICE_ROOT").map_or_else(
            || {
                bundled.map_or_else(
                    || PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../service")),
                    |dir| dir.join("service"),
                )
            },
            PathBuf::from,
        );
        let candidates = [
            std::env::var_os("ONSET_PYTHON").map(PathBuf::from),
            bundled.map(|dir| dir.join("runtime").join("python.exe")),
            Some(root.join(".venv").join("Scripts").join("python.exe")),
            Some(root.join(".runtime").join("Scripts").join("python.exe")),
        ];
        let python = candidates
            .into_iter()
            .flatten()
            .find(|p| p.exists())
            .ok_or_else(|| {
                format!(
                    "The Python service's runtime is missing. Set ONSET_PYTHON, or run \
                     scripts/bootstrap.ps1 in {}",
                    root.display()
                )
            })?;
        Ok(Self { python, root })
    }
}

impl Service {
    pub fn start(
        runtime: &Runtime,
        home: &Path,
        on_event: impl Fn(Value) + Send + 'static,
    ) -> std::io::Result<Self> {
        let mut cmd = Command::new(&runtime.python);
        cmd.args(["-u", "-m", "onset_service", "--home"])
            .arg(home)
            .arg("rpc")
            .current_dir(&runtime.root)
            .env("PYTHONPATH", &runtime.root)
            .env("PYTHONUTF8", "1")
            .env("NUMBA_CACHE_DIR", home.join("numba"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = cmd.spawn()?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let pending: Pending = Arc::default();
        let last_error: Arc<Mutex<String>> = Arc::default();
        // The service writes why it stops and then exits; the reply thread waits for that
        // line before failing the calls still waiting.
        let (err_done, err_finished) = std::sync::mpsc::channel::<()>();

        let replies = pending.clone();
        let reason = last_error.clone();
        std::thread::Builder::new()
            .name("service-out".into())
            .spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    let Ok(value) = serde_json::from_str::<Value>(&line) else {
                        tracing::warn!("invalid service response");
                        continue;
                    };
                    if value.get("method").is_some() {
                        on_event(value);
                        continue;
                    }
                    let Some(id) = value.get("id").and_then(Value::as_u64) else {
                        continue;
                    };
                    if let Some(tx) = replies.lock().remove(&id) {
                        let outcome = match value.get("error") {
                            Some(e) => Err(e
                                .get("message")
                                .and_then(Value::as_str)
                                .unwrap_or("Service error")
                                .to_owned()),
                            None => Ok(value.get("result").cloned().unwrap_or(Value::Null)),
                        };
                        let _ = tx.send(outcome);
                    }
                }
                // The service stopped: fail every call still waiting, with its reason.
                let _ = err_finished.recv_timeout(Duration::from_millis(500));
                let message = stopped_message(&reason.lock());
                for (_, tx) in replies.lock().drain() {
                    let _ = tx.send(Err(message.clone()));
                }
            })?;
        // Library logs stay out of the screens; keep them at debug.
        let latest = last_error.clone();
        std::thread::Builder::new()
            .name("service-err".into())
            .spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    tracing::debug!(target: "onset_service", "{line}");
                    if !line.trim().is_empty() {
                        line.trim().clone_into(&mut latest.lock());
                    }
                }
                let _ = err_done.send(());
            })?;

        Ok(Self {
            child: Mutex::new(child),
            stdin: Mutex::new(stdin),
            last_error,
            next: AtomicU64::new(1),
            pending,
        })
    }

    pub async fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().insert(id, tx);
        let line = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        let sent = {
            let mut stdin = self.stdin.lock();
            match stdin.as_mut() {
                Some(w) => writeln!(w, "{line}").and_then(|()| w.flush()).is_ok(),
                None => false,
            }
        };
        if !sent {
            self.pending.lock().remove(&id);
            return Err(stopped_message(&self.last_error.lock()));
        }
        rx.await
            .unwrap_or_else(|_| Err("Analysis service stopped".into()))
    }

    /// While a show runs the service yields the processor to the visuals: below-normal
    /// priority and Windows' efficiency mode (`EcoQoS`). `false` restores both.
    pub fn set_background(&self, on: bool) -> Result<(), String> {
        set_background(&self.child.lock(), on)
    }

    /// Closes the service's input so it checkpoints and exits, then kills it after `grace`.
    pub fn stop(&self, grace: Duration) {
        self.stdin.lock().take();
        let deadline = Instant::now() + grace;
        let mut child = self.child.lock();
        while Instant::now() < deadline {
            if matches!(child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = child.kill();
    }
}

/// "Analysis service stopped", with the service's own last words when it left any.
fn stopped_message(last_error: &str) -> String {
    let reason = last_error
        .rsplit_once(": ")
        .filter(|(kind, _)| kind.ends_with("Error") || kind.ends_with("Exception"))
        .map_or(last_error, |(_, message)| message);
    if reason.is_empty() {
        "Analysis service stopped".into()
    } else {
        format!("Analysis service stopped: {reason}")
    }
}

/// Below-normal priority and efficiency mode for `child`, or back to normal.
pub fn set_background(child: &Child, on: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::Threading::{
            BELOW_NORMAL_PRIORITY_CLASS, NORMAL_PRIORITY_CLASS,
            PROCESS_POWER_THROTTLING_CURRENT_VERSION, PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
            PROCESS_POWER_THROTTLING_STATE, ProcessPowerThrottling, SetPriorityClass,
            SetProcessInformation,
        };
        let handle = child.as_raw_handle();
        let class = if on {
            BELOW_NORMAL_PRIORITY_CLASS
        } else {
            NORMAL_PRIORITY_CLASS
        };
        let throttling = PROCESS_POWER_THROTTLING_STATE {
            Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
            ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
            StateMask: if on {
                PROCESS_POWER_THROTTLING_EXECUTION_SPEED
            } else {
                0
            },
        };
        // SAFETY: `handle` belongs to the live child held by the lock above, and
        // `throttling` is a correctly sized PROCESS_POWER_THROTTLING_STATE.
        unsafe {
            if SetPriorityClass(handle, class) == 0 {
                return Err(format!(
                    "Cannot change the service's priority: {}",
                    std::io::Error::last_os_error()
                ));
            }
            // Efficiency mode is a hint; older Windows versions lack it.
            SetProcessInformation(
                handle,
                ProcessPowerThrottling,
                std::ptr::from_ref(&throttling).cast(),
                u32::try_from(std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>())
                    .expect("small struct"),
            );
        }
    }
    #[cfg(not(windows))]
    let _ = on;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Runtime;

    #[test]
    fn a_stopped_service_says_why() {
        assert_eq!(super::stopped_message(""), "Analysis service stopped");
        assert_eq!(
            super::stopped_message("RuntimeError: OnSet is already running on this workspace"),
            "Analysis service stopped: OnSet is already running on this workspace"
        );
        assert_eq!(
            super::stopped_message("some library note: details"),
            "Analysis service stopped: some library note: details"
        );
    }

    #[cfg(windows)]
    #[test]
    fn background_lowers_and_restores_the_priority_class() {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::Threading::{
            BELOW_NORMAL_PRIORITY_CLASS, GetPriorityClass, NORMAL_PRIORITY_CLASS,
        };
        let mut child = std::process::Command::new("ping")
            .args(["-n", "5", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        // SAFETY: the handle belongs to `child`, alive until killed below.
        let class = |c: &std::process::Child| unsafe { GetPriorityClass(c.as_raw_handle()) };
        super::set_background(&child, true).unwrap();
        assert_eq!(class(&child), BELOW_NORMAL_PRIORITY_CLASS);
        super::set_background(&child, false).unwrap();
        assert_eq!(class(&child), NORMAL_PRIORITY_CLASS);
        let _ = child.kill();
        let _ = child.wait();
    }

    #[test]
    fn an_installed_layout_beside_the_executable_wins_over_the_repository() {
        if std::env::var_os("ONSET_SERVICE_ROOT").is_some()
            || std::env::var_os("ONSET_PYTHON").is_some()
        {
            return; // explicit overrides take precedence by design
        }
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("service").join("onset_service")).unwrap();
        std::fs::create_dir_all(dir.path().join("runtime")).unwrap();
        std::fs::write(dir.path().join("runtime").join("python.exe"), b"").unwrap();
        let runtime = Runtime::locate_in(Some(dir.path())).unwrap();
        assert_eq!(runtime.root, dir.path().join("service"));
        assert_eq!(
            runtime.python,
            dir.path().join("runtime").join("python.exe")
        );
    }

    #[test]
    fn without_a_bundled_service_the_repository_layout_is_used() {
        if std::env::var_os("ONSET_SERVICE_ROOT").is_some() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        if let Ok(runtime) = Runtime::locate_in(Some(dir.path())) {
            assert!(runtime.root.ends_with("service"));
            assert!(!runtime.root.starts_with(dir.path()));
        }
    }
}
