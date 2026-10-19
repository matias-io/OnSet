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
    next: AtomicU64,
    pending: Pending,
}

/// Where the Python service's source and interpreter are.
pub struct Runtime {
    pub python: PathBuf,
    pub root: PathBuf,
}

impl Runtime {
    /// `ONSET_SERVICE_ROOT` (default: the repository's `service/` folder) and `ONSET_PYTHON`
    /// (default: the root's `.venv` or `.runtime` interpreter).
    pub fn locate() -> Result<Self, String> {
        let root = std::env::var_os("ONSET_SERVICE_ROOT").map_or_else(
            || PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../service")),
            PathBuf::from,
        );
        let candidates = [
            std::env::var_os("ONSET_PYTHON").map(PathBuf::from),
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

        let replies = pending.clone();
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
                // The service stopped: fail every call still waiting.
                for (_, tx) in replies.lock().drain() {
                    let _ = tx.send(Err("Analysis service stopped".into()));
                }
            })?;
        // Library logs stay out of the screens; keep them at debug.
        std::thread::Builder::new()
            .name("service-err".into())
            .spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    tracing::debug!(target: "onset_service", "{line}");
                }
            })?;

        Ok(Self {
            child: Mutex::new(child),
            stdin: Mutex::new(stdin),
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
            return Err("Analysis service stopped".into());
        }
        rx.await
            .unwrap_or_else(|_| Err("Analysis service stopped".into()))
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
