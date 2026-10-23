//! Replaying a recorded set through the headless renderer (`onset_render::replay`), so a
//! scene or mapping change can be compared on identical input.
//!
//! `tests/fixtures/synthetic.onsetrec` is four seconds of the gallery's synthetic set at
//! 30 Hz, written by `synthetic_fixture_matches_the_generator` when `ONSET_UPDATE_FIXTURES`
//! is set. A real set is recorded with `onset --record` and replayed with
//! `replay_recording` (see the test's doc comment).
mod common;

use std::path::PathBuf;

use common::{diff, shader_dir, state};
use onset_core::music_state::MusicState;
use onset_core::recording::{Encoder, header};
use onset_core::show::AutoChange;
use onset_render::headless::Headless;
use onset_render::renderer::{Quality, RenderSettings, Renderer};
use onset_render::replay::Recording;
use onset_render::scenes::builtin_scenes;
use onset_render::uniforms::FrameUniforms;

const FIXTURE_HZ: f32 = 30.0;
const FIXTURE_SECONDS: f32 = 4.0;
const DROP_AT: f32 = 1.6;

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/synthetic.onsetrec")
}

/// The synthetic set as the engine would have recorded it.
fn synthetic_states() -> Vec<MusicState> {
    (0..(FIXTURE_HZ * FIXTURE_SECONDS) as u32)
        .map(|f| state(f as f32 / FIXTURE_HZ, DROP_AT, 1.0, 1.0))
        .collect()
}

fn encode(states: &[MusicState]) -> Vec<u8> {
    let mut out = header().to_vec();
    let mut enc = Encoder::new();
    for ms in states {
        enc.push(ms, &mut out);
    }
    out
}

/// Whether two states would drive the shaders the same way, within float noise (the
/// generator's `sin` and `powf` may differ by an ulp between platforms).
fn same_uniforms(a: &MusicState, b: &MusicState) -> bool {
    let ua = FrameUniforms::from_state(a, (1, 1), 0.0);
    let ub = FrameUniforms::from_state(b, (1, 1), 0.0);
    let fa: &[f32] = bytemuck::cast_slice(bytemuck::bytes_of(&ua));
    let fb: &[f32] = bytemuck::cast_slice(bytemuck::bytes_of(&ub));
    fa.iter().zip(fb).all(|(x, y)| (x - y).abs() < 1e-4)
        && a.track.as_ref().map(|t| (&t.title, t.bpm)) == b.track.as_ref().map(|t| (&t.title, t.bpm))
        && a.beat_index == b.beat_index
        && (a.bpm_grid - b.bpm_grid).abs() < 1e-4
}

#[test]
fn synthetic_fixture_matches_the_generator() {
    let path = fixture_path();
    let states = synthetic_states();
    if std::env::var_os("ONSET_UPDATE_FIXTURES").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, encode(&states)).unwrap();
        println!("wrote {}", path.display());
    }
    let recording = Recording::read(&path).expect(
        "the fixture exists; regenerate it with ONSET_UPDATE_FIXTURES=1 cargo test -p onset-render --test replay",
    );
    assert!(!recording.truncated);
    assert_eq!(recording.len(), states.len());
    for (i, (got, want)) in recording.frames().zip(&states).enumerate() {
        assert!(
            same_uniforms(got, want),
            "state {i} differs from the generator; regenerate with ONSET_UPDATE_FIXTURES=1"
        );
    }
    assert!((recording.duration_s() - f64::from((FIXTURE_SECONDS * FIXTURE_HZ - 1.0) / FIXTURE_HZ)).abs() < 1e-6);
    // The drop is in the recording: the countdown reaches zero and the phrase flips.
    let before = recording.at(f64::from(DROP_AT) - 0.05).unwrap();
    let after = recording.at(f64::from(DROP_AT) + 0.05).unwrap();
    assert!(before.drop_countdown_beats.is_some() && after.drop_countdown_beats.is_none());
}

struct Stage {
    h: Headless,
    r: Renderer,
}

impl Stage {
    fn new(size: (u32, u32), software: bool, quality: Quality) -> Self {
        let h = Headless::new(size, software).expect("adapter");
        let mut r = Renderer::new(&h.gpu, size, h.format);
        let loaded = builtin_scenes(
            &h.gpu,
            r.scene_format(),
            &r.bindings().layout,
            &shader_dir(),
        );
        assert!(loaded.errors.is_empty(), "{:?}", loaded.errors);
        for s in loaded.scenes {
            r.add_scene(s);
        }
        r.set_show_card(false);
        r.set_settings(
            &h.gpu,
            RenderSettings {
                auto: false,
                auto_change: AutoChange::Off,
                adaptive: false,
                quality,
                ..RenderSettings::default()
            },
        );
        Self { h, r }
    }

    /// Renders one frame; reads the pixels back when `read`.
    fn frame(&mut self, ms: &MusicState, t: f32, read: bool) -> Option<Vec<u8>> {
        let r = &mut self.r;
        let view = self.h.view();
        let mut enc = self
            .h
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("replay") });
        r.render(&self.h.gpu, &mut enc, &view, ms, t);
        self.h.gpu.queue.submit([enc.finish()]);
        read.then(|| self.h.read_back())
    }

    fn save(&self, dir: &std::path::Path, name: &str, px: &[u8]) {
        let img = image::RgbaImage::from_raw(self.h.size.0, self.h.size.1, px.to_vec()).unwrap();
        img.save(dir.join(format!("{name}.png"))).unwrap();
    }
}

#[test]
fn the_fixture_replays_through_a_scene_and_shows_its_drop() {
    let recording = Recording::read(&fixture_path()).expect("fixture");
    let dump = std::env::var_os("ONSET_DUMP_DIR").map(PathBuf::from);
    let mut stage = Stage::new((192, 108), true, Quality::Low);
    let scene = stage
        .r
        .scene_names()
        .into_iter()
        .find(|n| n == "synthwave")
        .or_else(|| stage.r.scene_names().into_iter().next())
        .expect("a scene");
    assert!(stage.r.set_scene(&scene));
    // Let the crossfade into the scene finish on the recording's first state.
    let first = recording.frames().next().unwrap().clone();
    for f in 0..40 {
        stage.frame(&first, f as f32 / FIXTURE_HZ, false);
    }
    let (start, _) = recording.span().unwrap();
    let mut build = Vec::new();
    let mut drop = Vec::new();
    for (t, ms) in recording.sampled(f64::from(FIXTURE_HZ)) {
        let rel = (t - start) as f32;
        let want = |at: f32| (rel - at).abs() < 0.5 / FIXTURE_HZ;
        let read = want(1.4) || want(DROP_AT + 0.1);
        if let Some(px) = stage.frame(ms, 10.0 + rel, read) {
            if want(1.4) {
                build = px;
            } else {
                drop = px;
            }
        }
    }
    assert!(!build.is_empty() && !drop.is_empty(), "both moments were sampled");
    if let Some(dir) = &dump {
        stage.save(dir, &format!("replay_{scene}_build"), &build);
        stage.save(dir, &format!("replay_{scene}_drop"), &drop);
    }
    let change = diff(&build, &drop);
    assert!(
        change > 4.0,
        "{scene}: the recorded drop barely changes the picture ({change:.1})"
    );
}

/// Replays a recording through one or more scenes and measures them; with a dump folder it
/// also saves a frame every so often for looking at or diffing against another build.
///
/// ```text
/// ONSET_RECORDING=<file.onsetrec> cargo test --release -p onset-render --test replay replay_recording -- --ignored --nocapture
/// ```
/// `ONSET_REPLAY_SCENES=a,b` limits the scenes (default: all), `ONSET_REPLAY_FPS` the
/// frame rate (30), `ONSET_REPLAY_SIZE` the frame size (1920x1080), `ONSET_REPLAY_SECONDS`
/// how much of the recording to replay (all), and `ONSET_DUMP_DIR` saves
/// `replay_<scene>_<frame>.png` every `ONSET_REPLAY_EVERY` frames (30). Without
/// `ONSET_RECORDING` the synthetic fixture is replayed.
#[test]
#[ignore = "replay tool: run on demand"]
fn replay_recording() {
    let path = std::env::var_os("ONSET_RECORDING").map_or_else(fixture_path, PathBuf::from);
    let recording = Recording::read(&path).expect("recording");
    let env = |k: &str| std::env::var(k).ok();
    let fps: f64 = env("ONSET_REPLAY_FPS").and_then(|v| v.parse().ok()).unwrap_or(30.0);
    let size = env("ONSET_REPLAY_SIZE")
        .and_then(|v| {
            let (w, h) = v.split_once('x')?;
            Some((w.parse().ok()?, h.parse().ok()?))
        })
        .unwrap_or((1920, 1080));
    let seconds: f64 = env("ONSET_REPLAY_SECONDS")
        .and_then(|v| v.parse().ok())
        .unwrap_or(f64::INFINITY);
    let every: u64 = env("ONSET_REPLAY_EVERY").and_then(|v| v.parse().ok()).unwrap_or(30);
    let dump = std::env::var_os("ONSET_DUMP_DIR").map(PathBuf::from);
    let mut stage = Stage::new(size, false, Quality::High);
    let all = stage.r.scene_names();
    let scenes: Vec<String> = match env("ONSET_REPLAY_SCENES") {
        Some(list) => list.split(',').map(str::to_string).collect(),
        None => all.clone(),
    };
    println!(
        "{}: {} states over {:.1} s{}; adapter {}; {}x{} at {fps} fps",
        path.display(),
        recording.len(),
        recording.duration_s(),
        if recording.truncated {
            " (cut short)"
        } else {
            ""
        },
        stage.h.gpu.adapter_name(),
        size.0,
        size.1
    );
    let (start, _) = recording.span().unwrap();
    for scene in scenes {
        assert!(stage.r.set_scene(&scene), "no scene {scene:?}; have {all:?}");
        let first = recording.frames().next().unwrap().clone();
        for f in 0..30 {
            stage.frame(&first, f as f32 / fps as f32, false);
        }
        let t0 = std::time::Instant::now();
        let mut cpu = 0.0f64;
        let mut frames = 0u64;
        for (t, ms) in recording.sampled(fps) {
            let rel = t - start;
            if rel > seconds {
                break;
            }
            let read = dump.is_some() && frames.is_multiple_of(every);
            let c0 = std::time::Instant::now();
            let px = stage.frame(ms, 100.0 + rel as f32, read);
            cpu += c0.elapsed().as_secs_f64() * 1000.0;
            if let (Some(dir), Some(px)) = (&dump, px) {
                stage.save(dir, &format!("replay_{scene}_{frames:05}"), &px);
            }
            frames += 1;
        }
        let _ = stage.h.gpu.device.poll(wgpu::PollType::wait_indefinitely());
        let wall = t0.elapsed().as_secs_f64() * 1000.0 / frames.max(1) as f64;
        println!(
            "{scene:>10}: {frames} frames, {wall:6.2} ms each (renderer CPU {:5.2} ms)",
            cpu / frames.max(1) as f64
        );
    }
}
