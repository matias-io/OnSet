//! Onset's visuals: the show renderer, its window and the settings it reads. The desktop app
//! runs it as its projector window; `onset.exe` runs it on its own for development and the
//! bench.
pub mod app;
mod bench;
mod brand;
pub mod config;
mod launcher;
mod lyrics;
mod monitors;
mod overlay;
mod thumbs;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};

pub use onset_engine as engine;
use winit::event_loop::{EventLoop, EventLoopProxy};

use crate::app::{AppOptions, OnsetApp, ShowMessage};

/// The window loop shows run on. winit allows one per process, so it is built once and
/// reused for every show; on Windows it need not run on the main thread.
fn event_loop() -> anyhow::Result<EventLoop<ShowMessage>> {
    let mut builder = EventLoop::<ShowMessage>::with_user_event();
    #[cfg(windows)]
    {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        builder.with_any_thread(true);
    }
    Ok(builder.build()?)
}

/// Opens the show window and runs it on the calling thread until it closes.
pub fn run(opts: AppOptions) -> anyhow::Result<()> {
    // The window and the render loop run on this thread.
    if !onset_engine::priority::raise_current_thread() {
        tracing::debug!("render thread keeps normal priority");
    }
    let event_loop = event_loop()?;
    let mut app = OnsetApp::new(opts);
    event_loop.run_app(&mut app)?;
    Ok(())
}

/// The desktop app's projector: one thread holding the window loop, which runs a show when
/// asked and waits for the next one when it ends.
pub struct Projector {
    shows: mpsc::Sender<AppOptions>,
    proxy: Mutex<Option<EventLoopProxy<ShowMessage>>>,
    running: Arc<AtomicBool>,
}

impl Projector {
    /// Starts the projector thread. `ended` runs on that thread after each show closes,
    /// however it closed (Esc, the window's close button, `end`).
    pub fn spawn(ended: impl Fn() + Send + 'static) -> anyhow::Result<Self> {
        let (shows, wanted) = mpsc::channel::<AppOptions>();
        let (proxy_tx, proxy_rx) = mpsc::channel();
        let running = Arc::new(AtomicBool::new(false));
        let flag = running.clone();
        std::thread::Builder::new()
            .name("onset-projector".into())
            .spawn(move || {
                use winit::platform::run_on_demand::EventLoopExtRunOnDemand;
                if !onset_engine::priority::raise_current_thread() {
                    tracing::debug!("render thread keeps normal priority");
                }
                let mut event_loop = match event_loop() {
                    Ok(l) => l,
                    Err(e) => {
                        tracing::error!("the projector cannot open windows: {e:#}");
                        let _ = proxy_tx.send(None);
                        return;
                    }
                };
                let _ = proxy_tx.send(Some(event_loop.create_proxy()));
                while let Ok(opts) = wanted.recv() {
                    let mut app = OnsetApp::new(opts);
                    if let Err(e) = event_loop.run_app_on_demand(&mut app) {
                        tracing::error!("the show stopped: {e}");
                    }
                    // Closes the window and releases the GPU before the next show.
                    drop(app);
                    flag.store(false, Ordering::Release);
                    ended();
                }
            })?;
        let proxy = proxy_rx
            .recv()
            .map_err(|_| anyhow::anyhow!("the projector thread stopped"))?
            .ok_or_else(|| anyhow::anyhow!("the projector cannot open windows"))?;
        Ok(Self {
            shows,
            proxy: Mutex::new(Some(proxy)),
            running,
        })
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    /// Runs a show with `opts`; refused while one is running.
    pub fn start(&self, opts: AppOptions) -> Result<(), String> {
        if self.running.swap(true, Ordering::AcqRel) {
            return Err("The show is already running".into());
        }
        self.shows.send(opts).map_err(|_| {
            self.running.store(false, Ordering::Release);
            "The projector stopped; restart OnSet".to_string()
        })
    }

    /// Closes the running show, if any.
    pub fn end(&self) {
        self.send(ShowMessage::End);
    }

    fn send(&self, message: ShowMessage) {
        if let Ok(proxy) = self.proxy.lock()
            && let Some(p) = proxy.as_ref()
        {
            let _ = p.send_event(message);
        }
    }
}
