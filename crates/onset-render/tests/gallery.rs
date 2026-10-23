//! Every scene through the whole stage (bloom, effects, transitions), driven by a synthetic
//! build-up and drop. Each scene must show structure at idle, respond to the drop, and
//! respond to the bass separately from the treble.
//!
//! Set `ONSET_DUMP_DIR` to save 960x540 frames of every scene (idle, build-up, drop,
//! chorus) for looking at; `ONSET_GALLERY_ART` points at a cover to use for the palette.
mod common;

use common::{diff, shader_dir, state, track};
use onset_core::music_state::MusicState;
use onset_core::show::AutoChange;
use onset_render::headless::Headless;
use onset_render::renderer::{Quality, RenderSettings, Renderer};
use onset_render::scenes::{builtin_scenes, scene_info};

const FPS: f32 = 30.0;

fn luma_stats(px: &[u8]) -> (f64, f64) {
    let lum: Vec<f64> = px
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| 0.2126 * f64::from(p[0]) + 0.7152 * f64::from(p[1]) + 0.0722 * f64::from(p[2]))
        .collect();
    let mean = lum.iter().sum::<f64>() / lum.len() as f64;
    let spread = (lum.iter().map(|l| (l - mean).powi(2)).sum::<f64>() / lum.len() as f64).sqrt();
    (mean, spread)
}

struct Gallery {
    h: Headless,
    r: Renderer,
    dump: Option<std::path::PathBuf>,
}

impl Gallery {
    fn new() -> Self {
        let dump = std::env::var_os("ONSET_DUMP_DIR").map(std::path::PathBuf::from);
        // ONSET_GALLERY_SIZE=1920x1080 renders full-size frames for screenshots.
        let wanted = std::env::var("ONSET_GALLERY_SIZE").ok().and_then(|v| {
            let (w, h) = v.split_once('x')?;
            Some((w.parse().ok()?, h.parse().ok()?))
        });
        let size = match (&dump, wanted) {
            (Some(_), Some(s)) => s,
            (Some(_), None) => (960, 540),
            _ => (192, 108),
        };
        let h = Headless::new(size, dump.is_none()).expect("adapter");
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
                // Readbacks make test frames slow; a fixed resolution keeps them comparable.
                adaptive: false,
                quality: if dump.is_some() {
                    Quality::High
                } else {
                    Quality::Low
                },
                ..RenderSettings::default()
            },
        );
        Self { h, r, dump }
    }

    fn frame(&mut self, ms: &MusicState, t: f32, read: bool) -> Option<Vec<u8>> {
        let r = &mut self.r;
        let mut draw = |gpu: &onset_render::gpu::Gpu, view: &wgpu::TextureView| {
            let mut enc = gpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("g") });
            r.render(gpu, &mut enc, view, ms, t);
            gpu.queue.submit([enc.finish()]);
        };
        if read {
            Some(self.h.render_with(draw))
        } else {
            let view = self.h.view();
            draw(&self.h.gpu, &view);
            None
        }
    }

    /// Idle, build-up, drop and chorus as one 2x2 sheet at the frame size.
    fn sheet(&self, name: &str, frames: [&[u8]; 4]) {
        let Some(dir) = &self.dump else { return };
        let (w, h) = self.h.size;
        let mut out = image::RgbaImage::new(w, h);
        for (i, px) in frames.iter().enumerate() {
            let img = image::RgbaImage::from_raw(w, h, px.to_vec()).unwrap();
            let small =
                image::imageops::resize(&img, w / 2, h / 2, image::imageops::FilterType::Triangle);
            let (x, y) = ((i as u32 % 2) * w / 2, (i as u32 / 2) * h / 2);
            image::imageops::overlay(&mut out, &small, i64::from(x), i64::from(y));
        }
        out.save(dir.join(format!("{name}_sheet.png"))).unwrap();
    }

    fn save(&self, name: &str, px: &[u8]) {
        if let Some(dir) = &self.dump {
            let img =
                image::RgbaImage::from_raw(self.h.size.0, self.h.size.1, px.to_vec()).unwrap();
            img.save(dir.join(format!("{name}.png"))).unwrap();
        }
    }
}

#[test]
fn every_scene_reacts_to_the_music_and_the_drop() {
    let mut gallery = Gallery::new();
    let names = gallery.r.scene_names();
    let drop_at = 1.6;
    let only = std::env::var("ONSET_GALLERY_ONLY").ok();
    for name in names {
        if only
            .as_deref()
            .is_some_and(|o| !o.split(',').any(|x| x == name))
        {
            continue;
        }
        assert!(gallery.r.set_scene(&name));
        // Let the crossfade into this scene finish while idle.
        let mut idle = MusicState::default();
        let mut px_idle = Vec::new();
        for f in 0..40 {
            let t = f as f32 / FPS;
            idle.time_s = f64::from(t);
            if let Some(px) = gallery.frame(&idle, 100.0 + t, f == 39) {
                px_idle = px;
            }
        }
        gallery.save(&format!("{name}_0_idle"), &px_idle);
        let (_, spread) = luma_stats(&px_idle);
        assert!(
            spread > 3.0,
            "{name}: idle frame is flat (spread {spread:.1})"
        );

        let mut build = Vec::new();
        let mut drop = Vec::new();
        let mut chorus = Vec::new();
        for f in 0..(3.0 * FPS) as u32 {
            let t = f as f32 / FPS;
            let ms = state(t, drop_at, 1.0, 1.0);
            let want = |at: f32| (t - at).abs() < 0.5 / FPS;
            let read = want(1.4) || want(drop_at + 0.1) || want(2.8);
            if let Some(px) = gallery.frame(&ms, 200.0 + t, read) {
                if want(1.4) {
                    build = px;
                } else if want(drop_at + 0.1) {
                    drop = px;
                } else {
                    chorus = px;
                }
            }
        }
        gallery.save(&format!("{name}_1_build"), &build);
        gallery.save(&format!("{name}_2_drop"), &drop);
        gallery.save(&format!("{name}_3_chorus"), &chorus);
        gallery.sheet(&name, [&px_idle, &build, &drop, &chorus]);
        let change = diff(&build, &drop);
        assert!(
            change > 4.0,
            "{name}: the drop barely changes the picture ({change:.1})"
        );

        // Same moment, bass only versus treble only: the picture must differ. The classic
        // scenes predate the frequency split and stay out of Auto mode.
        if !scene_info(&name).is_some_and(|i| i.auto) {
            continue;
        }
        let t = 2.0;
        let bass_only = state(t, drop_at, 1.0, 0.0);
        let treble_only = state(t, drop_at, 0.0, 1.0);
        let a = gallery.frame(&bass_only, 300.0, true).unwrap();
        let b = gallery.frame(&treble_only, 300.0, true).unwrap();
        let change = diff(&a, &b);
        assert!(
            change > 1.0,
            "{name}: bass and treble look the same ({change:.2})"
        );
    }
}

#[test]
fn lyrics_follow_the_playhead_in_every_style() {
    use onset_core::lyrics::Lyrics;
    use onset_render::overlay_options::{LyricStyle, LyricsOptions};
    let mut gallery = Gallery::new();
    assert!(gallery.r.set_scene("synthwave"));
    // The state's playhead runs at t + 60 s: the first line is sung from 60.2 s.
    let lrc = "[01:00.20] You lift my heart up\n[01:01.60] When the rest of me is down\n[01:03.00] You enchant me\n";
    let lyrics = std::sync::Arc::new(Lyrics::parse_lrc(lrc, "test"));
    let drop_at = 1.6;
    let mut shots: Vec<Vec<u8>> = Vec::new();
    for (style, at) in [
        (LyricStyle::Clean, 0.9),
        (LyricStyle::Glow, 0.9),
        (LyricStyle::Karaoke, 1.0),
        (LyricStyle::Glow, drop_at + 0.15),
    ] {
        let mut plain = None;
        for show in [false, true] {
            gallery
                .r
                .set_lyrics(track().id, Some(lyrics.clone()), String::new());
            gallery.r.set_lyrics_options(LyricsOptions {
                enabled: show,
                style,
                drop_fx: 1.0,
                ..LyricsOptions::default()
            });
            // Run up to the moment so the line has slid in.
            let mut px = Vec::new();
            let steps = 30;
            for k in 0..=steps {
                let t = at - 0.5 + 0.5 * k as f32 / steps as f32;
                let ms = state(t, drop_at, 1.0, 1.0);
                if let Some(p) = gallery.frame(&ms, 500.0 + t, k == steps) {
                    px = p;
                }
            }
            if show {
                let d = plain.as_ref().map_or(0.0, |p: &Vec<u8>| diff(p, &px));
                assert!(
                    d > 0.3,
                    "{style:?} at {at}: the lyrics barely show ({d:.2})"
                );
                shots.push(px);
            } else {
                plain = Some(px);
            }
        }
    }
    gallery.sheet("lyrics", [&shots[0], &shots[1], &shots[2], &shots[3]]);
}

/// Frame cost of each scene at 1920x1080 on the fastest adapter, without a window:
/// `cargo test --release -p onset-render --test gallery bench -- --ignored --nocapture`.
#[test]
#[ignore = "benchmark: run on demand"]
fn bench_scenes_headless() {
    let size = (1920, 1080);
    let h = Headless::new(size, false).expect("adapter");
    let mut r = Renderer::new(&h.gpu, size, h.format);
    let loaded = builtin_scenes(
        &h.gpu,
        r.scene_format(),
        &r.bindings().layout,
        &shader_dir(),
    );
    for s in loaded.scenes {
        r.add_scene(s);
    }
    r.set_show_card(false);
    r.set_settings(
        &h.gpu,
        RenderSettings {
            auto: false,
            auto_change: AutoChange::Off,
            // Readbacks make test frames slow; a fixed resolution keeps them comparable.
            adaptive: false,
            quality: Quality::High,
            ..RenderSettings::default()
        },
    );
    println!("adapter: {}", h.gpu.adapter_name());
    for name in r.scene_names() {
        assert!(r.set_scene(&name));
        let view = h.view();
        let mut run = |frames: u32, start: f32| {
            let t0 = std::time::Instant::now();
            let mut cpu = 0.0f64;
            for f in 0..frames {
                let t = start + f as f32 / 60.0;
                let ms = state(t % 3.0, 1.6, 1.0, 1.0);
                let mut enc = h
                    .gpu
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("b") });
                let c0 = std::time::Instant::now();
                r.render(&h.gpu, &mut enc, &view, &ms, 1000.0 + t);
                cpu += c0.elapsed().as_secs_f64() * 1000.0;
                h.gpu.queue.submit([enc.finish()]);
                let _ = h.gpu.device.poll(wgpu::PollType::wait_indefinitely());
            }
            (
                t0.elapsed().as_secs_f64() * 1000.0 / f64::from(frames),
                cpu / f64::from(frames),
            )
        };
        run(30, 0.0); // warm up and finish the scene crossfade
        let (ms, cpu) = run(120, 1.0);
        println!("{name:>10}: {ms:6.2} ms  (renderer CPU {cpu:5.2} ms)");
    }
}
