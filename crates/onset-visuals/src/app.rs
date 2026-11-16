//! The show window: borderless fullscreen on the chosen monitor (or a window, as a preview),
//! one wgpu surface and the frame loop that renders the engine's latest `MusicState` through
//! the active scene. Nothing but the show is drawn; the desktop app holds the controls.
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use onset_core::music_state::MusicState;
use onset_core::show::ShowEvent;
use onset_render::assets::shader_dir;
use onset_render::gpu::Gpu;
use onset_render::headless::read_texture;
use onset_render::hot_reload::ShaderWatcher;
use onset_render::renderer::Renderer;
use onset_render::scenes::builtin_scenes;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::monitor::MonitorHandle;
use winit::window::{Fullscreen, Window, WindowAttributes, WindowId, WindowLevel};

use crate::bench::BenchStats;
use crate::config::{Config, ConfigExt, MonitorChoice, PresentModeChoice};
use crate::engine::{Engine, EngineCommand};
use crate::monitors::{MonitorInfo, choose, covers};

/// How a bench presents frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum BenchPace {
    /// No `VSync`: the GPU's own pace.
    Uncapped,
    /// The configured present mode, as a show runs.
    Show,
}

pub struct AppOptions {
    pub config: Config,
    /// Command-line override of the configured monitor.
    pub monitor: Option<MonitorChoice>,
    /// Command-line override of the starting scene; not written to the config.
    pub scene: Option<String>,
    /// Close automatically after this long (for unattended checks).
    pub exit_after: Option<Duration>,
    /// Print frame statistics on exit, presenting as `BenchPace` says.
    pub bench: Option<BenchPace>,
    /// Frames the GPU may queue ahead of the display.
    pub frame_latency: u32,
    /// The desktop app hosts the show: it owns the settings file and the controls, so the
    /// show saves nothing (key toggles last for that show).
    pub hosted: bool,
    /// The screen OnSet's window is on. A show never goes fullscreen there: it would cover
    /// the app and, usually, rekordbox beside it. It opens as a window instead.
    pub keep_clear: Option<String>,
    /// Save one frame here shortly before exit.
    pub screenshot: Option<PathBuf>,
    /// Onset's cache folder (decrypted library, lyrics).
    pub cache_dir: PathBuf,
    /// Shared with the desktop app's live view when it hosts the show.
    pub engine: Option<std::sync::Arc<Engine>>,
}

struct Surface {
    window: Arc<Window>,
    target: wgpu::Surface<'static>,
    gpu: Gpu,
    config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
    watcher: Option<ShaderWatcher>,
}

pub struct OnsetApp {
    opts: AppOptions,
    surface: Option<Surface>,
    started: Instant,
    frames: u64,
    fullscreen: bool,
    paused: bool,
    rate: f32,
    monitor: Option<MonitorHandle>,
    last_scene_log: Instant,
    last_frame: Option<Instant>,
    /// Frame intervals in ms, kept only in bench mode.
    frame_log: Vec<f32>,
    /// How old the engine's state was when each frame was presented, in ms (bench only).
    age_log: Vec<f32>,
    /// `exit()` was requested; `about_to_wait` can run again before the loop stops.
    exiting: bool,
    /// Where the one requested screenshot goes; taken out once it is written.
    screenshot_pending: Option<PathBuf>,
    lyrics: crate::lyrics::LyricsWorker,
}

fn monitor_infos(event_loop: &ActiveEventLoop) -> (Vec<MonitorHandle>, Vec<MonitorInfo>) {
    let primary = event_loop.primary_monitor();
    let handles: Vec<MonitorHandle> = event_loop.available_monitors().collect();
    let infos = handles
        .iter()
        .map(|m| MonitorInfo {
            name: m.name().unwrap_or_default(),
            is_primary: primary.as_ref() == Some(m),
            size: (m.size().width, m.size().height),
            scale: m.scale_factor(),
        })
        .collect();
    (handles, infos)
}

/// Loads the built-in scenes; a shader that fails to compile is reported and skipped so a
/// broken file cannot take the whole output down.
fn load_scenes(gpu: &Gpu, renderer: &mut Renderer) {
    let dir = shader_dir();
    let loaded = builtin_scenes(
        gpu,
        renderer.scene_format(),
        &renderer.bindings().layout,
        &dir,
    );
    for scene in loaded.scenes {
        renderer.add_scene(scene);
    }
    if let Some(e) = loaded.errors.first() {
        renderer.set_last_error(e.to_string());
    }
    tracing::info!(scenes = ?renderer.scene_names(), dir = %dir.display(), "scenes loaded");
}

/// The configured mode; the bench wants the GPU's real pace, so it asks for no `VSync`.
fn wanted_present_mode(bench: bool, choice: PresentModeChoice) -> wgpu::PresentMode {
    if bench {
        return wgpu::PresentMode::Immediate;
    }
    match choice {
        PresentModeChoice::Fifo => wgpu::PresentMode::Fifo,
        PresentModeChoice::Mailbox => wgpu::PresentMode::Mailbox,
    }
}

/// Picks a supported mode, falling back towards `VSync`.
fn pick_present_mode(
    bench: bool,
    choice: PresentModeChoice,
    caps: &wgpu::SurfaceCapabilities,
) -> wgpu::PresentMode {
    let wanted = wanted_present_mode(bench, choice);
    if caps.present_modes.contains(&wanted) {
        return wanted;
    }
    if bench && caps.present_modes.contains(&wgpu::PresentMode::Mailbox) {
        return wgpu::PresentMode::Mailbox;
    }
    wgpu::PresentMode::Fifo
}

/// The next swapchain texture, reconfiguring the surface when it says so; `None` means skip
/// this frame.
///
/// A suboptimal frame is still shown, and the surface is reconfigured only after it is
/// presented: wgpu refuses `configure` while a frame is held, and that refusal leaves the
/// surface unconfigured, so every later frame failed until the window happened to resize.
fn acquire_frame(s: &mut Surface) -> Option<(wgpu::SurfaceTexture, bool)> {
    match s.target.get_current_texture() {
        wgpu::CurrentSurfaceTexture::Success(f) => Some((f, false)),
        wgpu::CurrentSurfaceTexture::Suboptimal(f) => Some((f, true)),
        wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => None,
        wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
            s.target.configure(&s.gpu.device, &s.config);
            None
        }
        wgpu::CurrentSurfaceTexture::Validation => {
            // Configure again so the next frame can recover instead of failing forever.
            tracing::warn!("surface acquisition failed validation; reconfiguring");
            s.target.configure(&s.gpu.device, &s.config);
            None
        }
    }
}

/// Reads the frame back and writes it as PNG; failures are logged, never fatal.
fn save_frame(s: &Surface, texture: &wgpu::Texture, path: &std::path::Path) {
    if !s.config.usage.contains(wgpu::TextureUsages::COPY_SRC) {
        tracing::warn!("screenshot skipped: the surface cannot be read back on this adapter");
        return;
    }
    let size = (s.config.width, s.config.height);
    let mut pixels = read_texture(&s.gpu, texture, size);
    let bgra = matches!(
        s.config.format,
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
    );
    for px in pixels.as_chunks_mut::<4>().0 {
        if bgra {
            px.swap(0, 2);
        }
        px[3] = 255;
    }
    let Some(img) = image::RgbaImage::from_raw(size.0, size.1, pixels) else {
        tracing::warn!("screenshot buffer had the wrong size");
        return;
    };
    match img.save(path) {
        Ok(()) => tracing::info!(path = %path.display(), "screenshot saved"),
        Err(e) => tracing::warn!("screenshot not saved: {e}"),
    }
}

/// The key a press stands for. The physical scancode is the rule; input injected by tools
/// (macro pads, remote control) often carries no scancode, so the logical key stands in.
fn key_code(event: &KeyEvent) -> Option<KeyCode> {
    use winit::keyboard::{Key, NamedKey};
    if let PhysicalKey::Code(code) = event.physical_key {
        return Some(code);
    }
    match &event.logical_key {
        Key::Named(NamedKey::Escape) => Some(KeyCode::Escape),
        Key::Named(NamedKey::Space) => Some(KeyCode::Space),
        Key::Named(NamedKey::Home) => Some(KeyCode::Home),
        Key::Named(NamedKey::ArrowLeft) => Some(KeyCode::ArrowLeft),
        Key::Named(NamedKey::ArrowRight) => Some(KeyCode::ArrowRight),
        Key::Character(c) => match c.to_ascii_lowercase().as_str() {
            "f" => Some(KeyCode::KeyF),
            "h" => Some(KeyCode::KeyH),
            "c" => Some(KeyCode::KeyC),
            "b" => Some(KeyCode::KeyB),
            "r" => Some(KeyCode::KeyR),
            "[" => Some(KeyCode::BracketLeft),
            "]" => Some(KeyCode::BracketRight),
            _ => None,
        },
        _ => None,
    }
}

/// Borderless fullscreen on `monitor` (always on top, so nothing covers the projector), or a
/// 1280x720 window when the show is a preview on the DJ's own screen.
fn window_attributes(fullscreen: bool, monitor: Option<&MonitorHandle>) -> WindowAttributes {
    // A windowed show is a preview on the DJ's own screen, with a title bar to move it.
    let attrs = WindowAttributes::default()
        .with_title("OnSet")
        .with_window_icon(crate::brand::window_icon())
        .with_decorations(!fullscreen);
    if fullscreen {
        attrs
            .with_fullscreen(Some(Fullscreen::Borderless(monitor.cloned())))
            .with_window_level(WindowLevel::AlwaysOnTop)
    } else {
        attrs.with_inner_size(PhysicalSize::new(1280u32, 720u32))
    }
}

/// What the desktop app tells a running show.
#[derive(Debug, Clone)]
pub enum ShowMessage {
    /// Close the show window; the run returns.
    End,
    /// The settings changed in the app: apply what differs, now.
    Settings(Box<Config>),
    /// Play a show moment without music: the effects of a drop, a phrase change and so on.
    Preview(ShowEvent),
    /// Play a four-second build-up ending in a drop.
    PreviewBuild,
}

/// A change to the running show, from a key or from the app.
enum Action {
    Scene(String),
    PresentMode,
    InternalScale(f32),
    ShowHud(bool),
    ShowCard(bool),
    /// The look settings changed (style, effects, quality, Auto, rotation, overlay options).
    Look,
}

/// What to redo when the settings change under a running show. The output screen is not
/// in it: moving a running show between screens waits for the next show.
#[derive(Debug, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // independent things to redo
struct SettingsChange {
    scene: bool,
    present_mode: bool,
    internal_scale: bool,
    hud: bool,
    card: bool,
    look: bool,
    logo: bool,
    fullscreen: bool,
}

#[allow(clippy::float_cmp)] // a setting changed or it did not
fn settings_change(old: &Config, new: &Config) -> SettingsChange {
    SettingsChange {
        scene: !new.auto && old.scene != new.scene,
        present_mode: old.present_mode != new.present_mode,
        internal_scale: old.internal_scale != new.internal_scale,
        hud: old.show_hud != new.show_hud,
        card: old.show_card != new.show_card,
        look: old.render_settings() != new.render_settings()
            || old.hud != new.hud
            || old.card != new.card
            || old.lyrics != new.lyrics
            || old.blackout_logo_size != new.blackout_logo_size,
        logo: old.blackout_logo != new.blackout_logo,
        fullscreen: old.fullscreen != new.fullscreen,
    }
}

impl OnsetApp {
    fn save_config(&self) {
        if !self.opts.hosted
            && let Err(e) = self.opts.config.save()
        {
            tracing::warn!("could not save config: {e:#}");
        }
    }

    pub fn new(opts: AppOptions) -> Self {
        let screenshot_pending = opts.screenshot.clone();
        let opts_cache_dir = opts.cache_dir.clone();
        Self {
            fullscreen: opts.config.fullscreen,
            opts,
            surface: None,
            started: Instant::now(),
            frames: 0,
            paused: false,
            rate: 1.0,
            monitor: None,
            last_scene_log: Instant::now(),
            last_frame: None,
            frame_log: Vec::new(),
            age_log: Vec::new(),
            exiting: false,
            screenshot_pending,
            lyrics: crate::lyrics::LyricsWorker::spawn(&opts_cache_dir.join("lyrics")),
        }
    }

    /// Picks the output monitor and opens the show's window. Falls back to a window on the
    /// primary when the chosen monitor is missing.
    fn create_window(&mut self, event_loop: &ActiveEventLoop) -> anyhow::Result<Arc<Window>> {
        let (handles, infos) = monitor_infos(event_loop);
        let choice = self
            .opts
            .monitor
            .clone()
            .unwrap_or_else(|| self.opts.config.output_monitor.clone());
        let (index, fell_back) = choose(&infos, &choice);
        if fell_back {
            // Without the projector, a fullscreen always-on-top window would cover rekordbox
            // on the laptop screen; open a plain window instead and let F go fullscreen.
            tracing::warn!(
                ?choice,
                "requested monitor not found, opening a window on the primary"
            );
            self.fullscreen = false;
        }
        if self.fullscreen
            && infos
                .get(index)
                .is_some_and(|i| covers(i, self.opts.keep_clear.as_deref()))
        {
            tracing::warn!(
                "the show's screen is the one OnSet is on; opening a window instead of covering it"
            );
            self.fullscreen = false;
        }
        let monitor = handles.get(index).cloned();
        if let Some(info) = infos.get(index) {
            tracing::info!(
                monitor = %info.name,
                width = info.size.0,
                height = info.size.1,
                scale = info.scale,
                "output monitor"
            );
        }

        let window = Arc::new(
            event_loop.create_window(window_attributes(self.fullscreen, monitor.as_ref()))?,
        );
        window.set_cursor_visible(false);
        self.monitor = monitor;
        Ok(window)
    }

    fn create_surface(&mut self, event_loop: &ActiveEventLoop) -> anyhow::Result<()> {
        let window = self.create_window(event_loop)?;
        let instance = Gpu::new_instance();
        let surface = instance.create_surface(window.clone())?;
        let gpu =
            Gpu::new_for_surface_preferring(instance, &surface, self.opts.config.gpu.as_deref())?;
        let caps = surface.get_capabilities(&gpu.adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .unwrap_or(caps.formats[0]);
        let size = window.inner_size();
        let present_mode = pick_present_mode(
            self.opts.bench == Some(BenchPace::Uncapped),
            self.opts.config.present_mode,
            &caps,
        );
        // Screenshots copy the presented frame; ask for COPY_SRC only where it is offered.
        let mut usage = wgpu::TextureUsages::RENDER_ATTACHMENT;
        if self.opts.screenshot.is_some() && caps.usages.contains(wgpu::TextureUsages::COPY_SRC) {
            usage |= wgpu::TextureUsages::COPY_SRC;
        }
        let config = wgpu::SurfaceConfiguration {
            usage,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            alpha_mode: caps.alpha_modes[0],
            view_formats: Vec::new(),
            desired_maximum_frame_latency: self.opts.frame_latency,
        };
        surface.configure(&gpu.device, &config);
        tracing::info!(
            ?format,
            ?present_mode,
            width = config.width,
            height = config.height,
            "surface ready"
        );

        let mut renderer = Renderer::new(&gpu, (config.width, config.height), format);
        renderer.set_scale(window.scale_factor() as f32);
        renderer.set_show_card(self.opts.config.show_card);
        renderer.set_show_hud(self.opts.config.show_hud);
        renderer.set_settings(&gpu, self.render_settings());
        renderer.set_overlay_options(self.opts.config.hud.clone(), self.opts.config.card.clone());
        renderer.set_lyrics_options(self.opts.config.lyrics.clone());
        renderer.set_adapter_name(gpu.adapter_name());
        renderer.set_internal_scale(&gpu, self.opts.config.internal_scale);
        load_scenes(&gpu, &mut renderer);
        let wanted = self
            .opts
            .scene
            .clone()
            .unwrap_or_else(|| self.opts.config.scene.clone());
        if !renderer.set_scene(&wanted) {
            tracing::info!(requested = %wanted, "scene not found, using the first");
        }

        let watcher = match ShaderWatcher::new(&shader_dir()) {
            Ok(w) => Some(w),
            Err(e) => {
                tracing::warn!("shader hot reload disabled: {e:#}");
                None
            }
        };

        self.surface = Some(Surface {
            window,
            target: surface,
            gpu,
            config,
            renderer,
            watcher,
        });
        self.apply_logo();
        Ok(())
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
        if let Some(s) = self.surface.as_mut()
            && size.width > 0
            && size.height > 0
        {
            s.config.width = size.width;
            s.config.height = size.height;
            s.target.configure(&s.gpu.device, &s.config);
            s.renderer.resize(&s.gpu, (size.width, size.height));
            tracing::info!(
                width = size.width,
                height = size.height,
                "surface reconfigured"
            );
        }
    }

    /// Bench bookkeeping after a present: the frame interval and how old the music state was.
    fn record_frame(&mut self, ms: &MusicState) {
        let now = Instant::now();
        if self.opts.bench.is_some() {
            if let Some(engine) = self.opts.engine.as_ref() {
                self.age_log.push(engine.age(ms).as_secs_f32() * 1000.0);
            }
            if let Some(last) = self.last_frame {
                self.frame_log
                    .push(now.duration_since(last).as_secs_f32() * 1000.0);
            }
        }
        self.last_frame = Some(now);
    }

    fn render(&mut self) {
        let Some(s) = self.surface.as_mut() else {
            return;
        };
        if let Some(w) = s.watcher.as_mut() {
            s.renderer.poll_hot_reload(&s.gpu, w, &shader_dir());
        }
        let Some((frame, reconfigure)) = acquire_frame(s) else {
            return;
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let ms: Arc<MusicState> = self
            .opts
            .engine
            .as_ref()
            .map_or_else(|| Arc::new(MusicState::default()), |e| e.state());
        self.lyrics.tend(
            &mut s.renderer,
            ms.track.as_ref(),
            self.opts.config.lyrics.online,
        );
        let mut enc = s
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        let time_s = self.started.elapsed().as_secs_f32();
        if let Some(e) = &self.opts.engine {
            s.renderer.set_source_line(e.status().line());
        }
        let stats = s.renderer.render(&s.gpu, &mut enc, &view, &ms, time_s);
        s.gpu.queue.submit(std::iter::once(enc.finish()));

        if self.screenshot_due()
            && let Some(path) = self.screenshot_pending.take()
        {
            let s = self.surface.as_ref().expect("surface exists");
            save_frame(s, &frame.texture, &path);
        }
        let s = self.surface.as_mut().expect("surface exists");
        s.window.pre_present_notify();
        s.gpu.queue.present(frame);
        if reconfigure {
            s.target.configure(&s.gpu.device, &s.config);
        }
        self.frames += 1;
        tracing::trace!(cpu_ms = stats.cpu_ms, "frame recorded");

        // A once-a-second line so an unattended run leaves a trace of what it showed.
        if self.last_scene_log.elapsed() >= Duration::from_secs(1) {
            self.last_scene_log = Instant::now();
            tracing::debug!(
                scene = s.renderer.active_scene().unwrap_or("-"),
                playhead = ms.playhead_s,
                phrase = ?ms.phrase,
                drop = ?ms.drop_countdown_beats,
                intensity = ms.intensity,
                "frame"
            );
        }
        self.record_frame(&ms);
    }

    /// A second before exit, or three seconds in when the run is open-ended.
    fn screenshot_due(&self) -> bool {
        if self.screenshot_pending.is_none() {
            return false;
        }
        let at = self
            .opts
            .exit_after
            .map_or(3.0, |d| (d.as_secs_f32() - 1.0).max(0.5));
        self.started.elapsed().as_secs_f32() >= at
    }

    /// Applies one change to the running show.
    fn apply(&mut self, action: Action) {
        let Some(s) = self.surface.as_mut() else {
            return;
        };
        match action {
            Action::Scene(name) => {
                if s.renderer.set_scene(&name) {
                    tracing::info!(scene = %name, "switched");
                }
            }
            Action::PresentMode => {
                let bench = self.opts.bench == Some(BenchPace::Uncapped);
                let caps = s.target.get_capabilities(&s.gpu.adapter);
                s.config.present_mode =
                    pick_present_mode(bench, self.opts.config.present_mode, &caps);
                s.target.configure(&s.gpu.device, &s.config);
                tracing::info!(mode = ?s.config.present_mode, "present mode");
            }
            Action::InternalScale(scale) => s.renderer.set_internal_scale(&s.gpu, scale),
            Action::ShowHud(on) => s.renderer.set_show_hud(on),
            Action::ShowCard(on) => s.renderer.set_show_card(on),
            Action::Look => {
                let mut settings = self.opts.config.render_settings();
                if self.opts.scene.is_some() {
                    settings.auto = false;
                }
                s.renderer.set_settings(&s.gpu, settings);
                s.renderer.set_overlay_options(
                    self.opts.config.hud.clone(),
                    self.opts.config.card.clone(),
                );
                s.renderer
                    .set_lyrics_options(self.opts.config.lyrics.clone());
                s.renderer
                    .set_blackout_logo_size(self.opts.config.blackout_logo_size);
            }
        }
    }

    /// Takes the app's new settings and applies what changed to the running show.
    fn adopt_settings(&mut self, config: Config) {
        let change = settings_change(&self.opts.config, &config);
        let scene = config.scene.clone();
        let (hud, card, scale) = (config.show_hud, config.show_card, config.internal_scale);
        self.opts.config = config;
        tracing::info!(?change, "settings from the app");
        if change.scene {
            self.apply(Action::Scene(scene));
        }
        if change.present_mode {
            self.apply(Action::PresentMode);
        }
        if change.internal_scale {
            self.apply(Action::InternalScale(scale));
        }
        if change.hud {
            self.apply(Action::ShowHud(hud));
        }
        if change.card {
            self.apply(Action::ShowCard(card));
        }
        if change.look {
            self.apply(Action::Look);
        }
        if change.logo {
            self.apply_logo();
        }
        if change.fullscreen && self.fullscreen != self.opts.config.fullscreen {
            let on_onsets_screen = self.surface.as_ref().is_some_and(|s| {
                s.window
                    .current_monitor()
                    .and_then(|m| m.name())
                    .is_some_and(|name| {
                        self.opts
                            .keep_clear
                            .as_deref()
                            .is_some_and(|keep| keep.eq_ignore_ascii_case(&name))
                    })
            });
            if self.opts.config.fullscreen && on_onsets_screen {
                tracing::warn!("not going fullscreen on the screen OnSet is on");
            } else {
                self.toggle_fullscreen();
            }
        }
    }

    /// Loads the configured blackout logo into the renderer.
    fn apply_logo(&mut self) {
        let image = self
            .opts
            .config
            .blackout_logo
            .as_ref()
            .and_then(|p| image::open(p).ok())
            .map(|i| i.to_rgba8());
        let size = self.opts.config.blackout_logo_size;
        if let Some(s) = self.surface.as_mut() {
            s.renderer.set_blackout_logo(&s.gpu, image.as_ref(), size);
        }
    }

    /// The renderer's settings from the config; a scene named on the command line turns
    /// Auto mode off so that scene stays on.
    fn render_settings(&self) -> onset_render::renderer::RenderSettings {
        let mut settings = self.opts.config.render_settings();
        if self.opts.scene.is_some() {
            settings.auto = false;
        }
        settings
    }

    fn toggle_fullscreen(&mut self) {
        self.fullscreen = !self.fullscreen;
        if let Some(s) = &self.surface {
            // The monitor the window is on right now, so F never throws it onto another.
            let monitor = s.window.current_monitor().or_else(|| self.monitor.clone());
            let mode = self.fullscreen.then_some(Fullscreen::Borderless(monitor));
            s.window.set_fullscreen(mode);
            // A windowed show is a preview on the DJ's own screen: let it be moved.
            s.window.set_decorations(!self.fullscreen);
            s.window.set_window_level(if self.fullscreen {
                WindowLevel::AlwaysOnTop
            } else {
                WindowLevel::Normal
            });
        }
    }

    fn on_key(&mut self, event_loop: &ActiveEventLoop, key: KeyCode) {
        match key {
            // Ends the show: the window closes and the app hears about it.
            KeyCode::Escape => event_loop.exit(),
            KeyCode::KeyF => self.toggle_fullscreen(),
            KeyCode::KeyB => {
                if let Some(s) = self.surface.as_mut() {
                    let on = !s.renderer.blackout();
                    s.renderer.set_blackout(on);
                    tracing::info!(blackout = on, "toggle");
                }
            }
            KeyCode::KeyH => {
                self.opts.config.show_hud = !self.opts.config.show_hud;
                if let Some(s) = self.surface.as_mut() {
                    s.renderer.set_show_hud(self.opts.config.show_hud);
                }
                tracing::info!(hud = self.opts.config.show_hud, "toggle");
            }
            KeyCode::KeyC => {
                self.opts.config.show_card = !self.opts.config.show_card;
                if let Some(s) = self.surface.as_mut() {
                    s.renderer.set_show_card(self.opts.config.show_card);
                }
                tracing::info!(card = self.opts.config.show_card, "toggle");
            }
            KeyCode::Space => {
                self.paused = !self.paused;
                if let Some(e) = &self.opts.engine {
                    e.command(if self.paused {
                        EngineCommand::Pause
                    } else {
                        EngineCommand::Resume
                    });
                }
            }
            KeyCode::Home => {
                if let Some(e) = &self.opts.engine {
                    e.command(EngineCommand::Seek(0.0));
                }
            }
            KeyCode::BracketLeft | KeyCode::BracketRight => {
                self.rate += if key == KeyCode::BracketRight {
                    0.01
                } else {
                    -0.01
                };
                self.rate = self.rate.clamp(0.5, 1.5);
                if let Some(e) = &self.opts.engine {
                    e.command(EngineCommand::SetRate(self.rate));
                }
                tracing::info!(rate = self.rate, "sim rate");
            }
            KeyCode::KeyR => {
                if let (Some(e), Some(title)) = (&self.opts.engine, &self.opts.config.sim_track) {
                    e.command(EngineCommand::LoadTrack(title.clone()));
                }
            }
            KeyCode::ArrowRight | KeyCode::ArrowLeft => {
                if let Some(s) = self.surface.as_mut() {
                    if key == KeyCode::ArrowRight {
                        s.renderer.next_scene();
                    } else {
                        s.renderer.prev_scene();
                    }
                    if let Some(name) = s.renderer.active_scene() {
                        self.opts.config.scene = name.to_string();
                        tracing::info!(scene = name, "switched");
                    }
                }
            }
            _ => {}
        }
    }
}

impl ApplicationHandler<ShowMessage> for OnsetApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.surface.is_none()
            && let Err(e) = self.create_surface(event_loop)
        {
            tracing::error!("cannot create the output window: {e:#}");
            event_loop.exit();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let WindowEvent::KeyboardInput { event: key, .. } = &event {
            tracing::debug!(physical = ?key.physical_key, logical = ?key.logical_key, state = ?key.state, repeat = key.repeat, "key event");
        }
        let pressed_key = match &event {
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        state: ElementState::Pressed,
                        repeat: false,
                        ..
                    },
                ..
            } => match &event {
                WindowEvent::KeyboardInput { event, .. } => key_code(event),
                _ => None,
            },
            _ => None,
        };
        if let Some(code) = pressed_key {
            self.on_key(event_loop, code);
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => self.resize(size),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let Some(s) = self.surface.as_mut() {
                    s.renderer.set_scale(scale_factor as f32);
                    let size = s.window.inner_size();
                    self.resize(size);
                }
            }
            WindowEvent::RedrawRequested => {
                self.render();
                if let Some(s) = &self.surface {
                    s.window.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.save_config();
        // One line per show, so a real show leaves a trace of how smoothly it ran.
        let seconds = self.started.elapsed().as_secs_f64();
        tracing::info!(
            frames = self.frames,
            seconds = format!("{seconds:.1}"),
            fps = format!("{:.1}", self.frames as f64 / seconds.max(0.001)),
            "show frames"
        );
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, message: ShowMessage) {
        match message {
            ShowMessage::End => self.exiting = true,
            ShowMessage::Settings(config) => self.adopt_settings(*config),
            ShowMessage::Preview(event) => {
                if let Some(s) = self.surface.as_mut() {
                    s.renderer.preview_event(event);
                }
            }
            ShowMessage::PreviewBuild => {
                if let Some(s) = self.surface.as_mut() {
                    s.renderer.preview_build();
                }
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.exiting {
            // "End show" from the app, or an exit already under way.
            event_loop.exit();
            return;
        }
        if let Some(limit) = self.opts.exit_after
            && !self.exiting
            && self.started.elapsed() >= limit
        {
            self.exiting = true;
            let secs = self.started.elapsed().as_secs_f64();
            tracing::info!(
                frames = self.frames,
                fps = self.frames as f64 / secs,
                "exit-after reached"
            );
            if self.opts.bench.is_some() {
                let age = BenchStats::from_intervals(&self.age_log);
                println!(
                    "bench-age frame_latency={} vsync={} mean_ms={:.2} p99_ms={:.2}",
                    self.opts.frame_latency,
                    self.opts.bench == Some(BenchPace::Show),
                    age.mean_ms,
                    age.p99_ms
                );
                let stats = BenchStats::from_intervals(&self.frame_log);
                let (scene, size) = self
                    .surface
                    .as_ref()
                    .map_or(("-".to_string(), (0, 0)), |s| {
                        (
                            s.renderer.active_scene().unwrap_or("-").to_string(),
                            (s.config.width, s.config.height),
                        )
                    });
                println!("{}", stats.line(&scene, size));
            }
            event_loop.exit();
        }
        event_loop.set_control_flow(ControlFlow::Poll);
    }
}

#[cfg(test)]
mod settings_change_tests {
    use super::*;
    use crate::config::{Config, PresentModeChoice, Preset, Quality};

    #[test]
    fn nothing_changed_redoes_nothing() {
        let c = Config::default();
        assert_eq!(settings_change(&c, &c.clone()), SettingsChange::default());
    }

    #[test]
    fn each_setting_redoes_only_its_own_part() {
        let old = Config::default();
        let look = Config {
            preset: Preset::Chill,
            quality: Quality::Low,
            ..old.clone()
        };
        assert_eq!(
            settings_change(&old, &look),
            SettingsChange {
                look: true,
                ..SettingsChange::default()
            }
        );
        let toggles = Config {
            show_hud: !old.show_hud,
            show_card: !old.show_card,
            present_mode: PresentModeChoice::Mailbox,
            fullscreen: !old.fullscreen,
            ..old.clone()
        };
        let change = settings_change(&old, &toggles);
        assert!(change.hud && change.card && change.present_mode && change.fullscreen);
        assert!(!change.look && !change.scene);
    }

    #[test]
    fn a_new_scene_is_shown_only_when_auto_is_off() {
        let auto = Config {
            auto: true,
            ..Config::default()
        };
        let other = Config {
            scene: "warp".into(),
            ..auto.clone()
        };
        assert!(
            !settings_change(&auto, &other).scene,
            "Auto picks scenes itself"
        );
        let fixed = Config {
            auto: false,
            ..other
        };
        assert!(settings_change(&auto, &fixed).scene);
    }
}
