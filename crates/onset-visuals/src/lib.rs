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

pub use onset_engine as engine;

use crate::app::{AppOptions, OnsetApp};

/// Opens the show window and runs it on the calling thread until it closes. The thread need
/// not be the process's main thread, so the desktop app can run it beside its own windows.
pub fn run(opts: AppOptions) -> anyhow::Result<()> {
    use winit::event_loop::EventLoop;
    // The window and the render loop run on this thread.
    if !onset_engine::priority::raise_current_thread() {
        tracing::debug!("render thread keeps normal priority");
    }
    let mut builder = EventLoop::builder();
    #[cfg(windows)]
    {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        builder.with_any_thread(true);
    }
    let event_loop = builder.build()?;
    let mut app = OnsetApp::new(opts);
    event_loop.run_app(&mut app)?;
    Ok(())
}
