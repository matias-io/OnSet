//! Synced lyrics over the show. The line being sung is laid out word by word so each word
//! can take its own colour and motion: Karaoke lights words as they are sung, and at drops
//! and build-ups the words split into colour, sweep through a rainbow and ride a gentle wave.
//! Nothing blinks: opacity only ever eases in and out, so it stays safe for photosensitive
//! viewers.
//!
//! Timing: a line slides in ahead of its stamp so it is readable the moment its first word
//! is sung, and the playhead is pulled back by the audio lag the beat lock measured, so
//! the words follow what the room hears rather than what the deck reports. Readability:
//! every word carries an outline and a shadow, and a dark band sits behind the line.
use std::sync::Arc;

use onset_core::lyrics::{LyricAt, Lyrics};
use onset_core::music_state::MusicState;
use onset_core::show::Fx;
use onset_core::track::TrackId;

use crate::gpu::Gpu;
use crate::overlay_options::{LyricPlace, LyricStyle, LyricsOptions};
use crate::quad::{QuadPipeline, QuadTexture};
use crate::text::{TextItem, TextLayer, linear_to_srgb8};

/// Design size of the sung line at 1080 lines.
const LINE_PX: f32 = 64.0;
/// How long a new line takes to slide in, in seconds. The slide starts this long before
/// the line's stamp, so the line is in place when its first word is sung.
const APPEAR_S: f32 = 0.28;
/// Seconds of the beat lock's measured audio lag that nobody hears: the analyser's window
/// centre and hop (2048 / 2 + 256 samples at 48 kHz). The rest is the sound reaching the
/// room after the deck's position says it should, and the lyrics wait for it.
const ANALYSIS_S: f32 = 0.024;
/// Kicks the beat lock must have measured before its lag is trusted.
const LAG_MIN_SAMPLES: u32 = 20;
/// Outline thickness as a share of the font size.
const OUTLINE: f32 = 0.035;
/// The band's alpha at its middle when `backdrop` is 1.
const BAND_ALPHA: f32 = 0.85;
/// Rows of the band image: a soft edge needs a few.
const BAND_STEPS: u32 = 48;

/// Per-frame inputs to [`LyricsLayer::items`].
pub struct LyricsFrame<'a> {
    pub size: (u32, u32),
    pub ms: &'a MusicState,
    pub fx: &'a Fx,
    pub theme: &'a [[f32; 3]; 5],
    pub options: &'a LyricsOptions,
    pub time_s: f32,
}

pub struct LyricsLayer {
    track: Option<TrackId>,
    lyrics: Option<Arc<Lyrics>>,
    line: Option<usize>,
    previous: Option<usize>,
    changed_at: f32,
    quads: QuadPipeline,
    band: QuadTexture,
}

/// When the lyrics stand this frame: the playhead the words follow (after the DJ's offset
/// and the measured audio lag), the line to show (ahead of its stamp by the slide-in) and
/// what is being sung on it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Timing {
    pub playhead: f32,
    pub line: Option<usize>,
    /// The word being sung on `line`; at the start of a line, before its stamp, word 0 at
    /// progress 0.
    pub sung: Option<LyricAt>,
}

/// The playhead the lyrics follow: the deck's, shifted by the DJ's offset and pulled back
/// by the part of the measured audio lag a listener hears.
pub fn lyric_playhead(ms: &MusicState, opts: &LyricsOptions) -> f32 {
    let heard_lag = if ms.hits.lag_samples >= LAG_MIN_SAMPLES {
        (ms.hits.lag_s - ANALYSIS_S).max(0.0)
    } else {
        0.0
    };
    ms.playhead_s as f32 + opts.offset_s - heard_lag
}

pub fn timing(lyrics: &Lyrics, ms: &MusicState, opts: &LyricsOptions) -> Timing {
    let playhead = lyric_playhead(ms, opts);
    let now = lyrics.at(playhead);
    // The line slides in ahead of its stamp.
    let line = lyrics.at(playhead + APPEAR_S).or(now).map(|a| a.line);
    let sung = line.map(|l| match now {
        Some(a) if a.line == l => a,
        _ => LyricAt {
            line: l,
            line_progress: 0.0,
            since_s: 0.0,
            word: 0,
            word_progress: 0.0,
        },
    });
    Timing {
        playhead,
        line,
        sung,
    }
}

/// How lit word `index` is, 0..1: Karaoke sweeps through the words, Glow lights the words
/// sung so far, and both light the whole line at once when the source did not time the
/// words, rather than guessing. Clean is always lit.
pub fn lit(style: LyricStyle, word_timed: bool, index: usize, sung: &LyricAt) -> f32 {
    match style {
        LyricStyle::Clean => 1.0,
        LyricStyle::Glow => {
            if !word_timed || index <= sung.word {
                1.0
            } else {
                0.0
            }
        }
        LyricStyle::Karaoke => {
            if !word_timed {
                // A sweep across the line over its time, without pretending to know words.
                return sung.line_progress.clamp(0.0, 1.0);
            }
            match index.cmp(&sung.word) {
                std::cmp::Ordering::Less => 1.0,
                std::cmp::Ordering::Equal => sung.word_progress,
                std::cmp::Ordering::Greater => 0.0,
            }
        }
    }
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// HSV (all 0..1) to linear RGB, for the rainbow sweep.
fn hsv(hue: f32, sat: f32, val: f32) -> [f32; 3] {
    let sector = hue.rem_euclid(1.0) * 6.0;
    let chroma = val * sat;
    let second = chroma * (1.0 - ((sector % 2.0) - 1.0).abs());
    let (red, green, blue) = match sector as u32 {
        0 => (chroma, second, 0.0),
        1 => (second, chroma, 0.0),
        2 => (0.0, chroma, second),
        3 => (0.0, second, chroma),
        4 => (second, 0.0, chroma),
        _ => (chroma, 0.0, second),
    };
    let floor = val - chroma;
    [red + floor, green + floor, blue + floor]
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// The track's accent, lifted so it reads on any scene.
fn accent(theme: &[[f32; 3]; 5]) -> [f32; 3] {
    let c = theme[2];
    let m = c[0].max(c[1]).max(c[2]).max(0.05);
    mix([c[0] / m, c[1] / m, c[2] / m], [1.0; 3], 0.25)
}

/// A black band with soft top and bottom edges, stretched behind the line.
fn band_image(steps: u32) -> image::RgbaImage {
    image::RgbaImage::from_fn(1, steps, |_, y| {
        let t = y as f32 / (steps - 1).max(1) as f32;
        let edge = smooth(t / 0.35).min(smooth((1.0 - t) / 0.35));
        image::Rgba([0, 0, 0, (edge * 255.0).round() as u8])
    })
}

/// A word placed on screen.
struct Placed {
    text: String,
    x: f32,
    y: f32,
    /// Word index within the line, for Karaoke and the rainbow.
    index: usize,
}

impl LyricsLayer {
    pub fn new(gpu: &Gpu, format: wgpu::TextureFormat) -> Self {
        let quads = QuadPipeline::new(gpu, format);
        let band = quads.upload(gpu, &band_image(BAND_STEPS));
        Self {
            track: None,
            lyrics: None,
            line: None,
            previous: None,
            changed_at: 0.0,
            quads,
            band,
        }
    }

    /// The lyrics for `track`, or `None` when it has none. Replaces what was shown.
    pub fn set(&mut self, track: TrackId, lyrics: Option<Arc<Lyrics>>) {
        self.track = Some(track);
        self.lyrics = lyrics;
        self.line = None;
        self.previous = None;
    }

    /// Lays out one line's words centred at `y`, wrapping to a second row if too wide.
    fn layout(text: &mut TextLayer, words: &[String], px: f32, width: f32, y: f32) -> Vec<Placed> {
        let space = px * 0.3;
        let max_w = width * 0.84;
        let sizes: Vec<(f32, f32)> = words
            .iter()
            .map(|w| text.measure(&TextItem::new(w.as_str(), px, (0.0, 0.0)).weight(700)))
            .collect();
        // Split into rows that fit.
        let mut rows: Vec<Vec<usize>> = vec![Vec::new()];
        let mut row_w = 0.0;
        for (i, (w, _)) in sizes.iter().enumerate() {
            let add = if rows.last().is_some_and(Vec::is_empty) {
                *w
            } else {
                space + w
            };
            if row_w + add > max_w && !rows.last().is_some_and(Vec::is_empty) {
                rows.push(Vec::new());
                row_w = 0.0;
            }
            let add = if rows.last().is_some_and(Vec::is_empty) {
                *w
            } else {
                space + w
            };
            rows.last_mut().expect("a row").push(i);
            row_w += add;
        }
        let line_h = sizes.iter().map(|s| s.1).fold(px, f32::max) * 1.08;
        let top = y - line_h * rows.len() as f32 * 0.5;
        let mut out = Vec::with_capacity(words.len());
        for (r, row) in rows.iter().enumerate() {
            let total: f32 = row.iter().map(|&i| sizes[i].0).sum::<f32>()
                + space * row.len().saturating_sub(1) as f32;
            let mut x = (width - total) * 0.5;
            for &i in row {
                out.push(Placed {
                    text: words[i].clone(),
                    x,
                    y: top + r as f32 * line_h,
                    index: i,
                });
                x += sizes[i].0 + space;
            }
        }
        out
    }

    /// Draws the band behind the words over `span` (top and bottom pixels), `strength` 0..1.
    fn draw_band(
        &self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: (u32, u32),
        span: (f32, f32),
        strength: f32,
    ) {
        let (top, bottom) = span;
        if strength <= 0.001 || bottom <= top {
            return;
        }
        self.band.place(
            gpu,
            [0.0, top, size.0 as f32, bottom - top],
            size,
            strength * BAND_ALPHA,
        );
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("lyrics band"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        self.quads.draw(&mut pass, &self.band);
    }

    /// Draws the band and returns the text to draw this frame; empty when there is
    /// nothing to sing.
    #[allow(clippy::too_many_lines)] // one frame of the lyrics, read top to bottom
    pub fn items(
        &mut self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        text: &mut TextLayer,
        frame: &LyricsFrame<'_>,
    ) -> Vec<TextItem> {
        let opts = frame.options;
        let Some(lyrics) = self.lyrics.clone() else {
            return Vec::new();
        };
        let same_track = frame.ms.track.as_ref().map(|t| t.id) == self.track;
        if !opts.enabled || !same_track || lyrics.lines.is_empty() {
            return Vec::new();
        }
        let Timing {
            line: now_line,
            sung: at,
            ..
        } = timing(&lyrics, frame.ms, opts);
        if now_line != self.line {
            self.previous = self.line;
            self.line = now_line;
            self.changed_at = frame.time_s;
        }
        let since = frame.time_s - self.changed_at;
        let appear = smooth(since / APPEAR_S);

        let (width, height) = (frame.size.0 as f32, frame.size.1 as f32);
        let px = LINE_PX * height / 1080.0 * opts.size.clamp(0.5, 2.5);
        let centre_y = height
            * match opts.place {
                LyricPlace::Top => 0.2,
                LyricPlace::Centre => 0.5,
                LyricPlace::Lower => 0.72,
            };
        let accent = accent(frame.theme);
        // How much the lyrics move: the drop and, more gently, the build-up towards it.
        let amount = opts.drop_fx.clamp(0.0, 1.0) * frame.fx.drop_hit.max(frame.fx.build * 0.6);
        let kick = frame.ms.hits.kick.clamp(0.0, 1.0);
        let mut items = Vec::new();
        // The band's extent, grown around whatever is drawn.
        let mut band_top = f32::MAX;
        let mut band_bottom = f32::MIN;
        let mut cover = |y: f32, h: f32| {
            band_top = band_top.min(y);
            band_bottom = band_bottom.max(y + h);
        };

        // The previous line drifts up and fades as the new one arrives.
        if let Some(prev) = self.previous.filter(|_| appear < 1.0)
            && let Some(line) = lyrics.lines.get(prev)
        {
            let words: Vec<String> = line.words.iter().map(|w| w.text.clone()).collect();
            let fade = 1.0 - appear;
            for word in Self::layout(text, &words, px, width, centre_y - px * 0.8 * appear) {
                cover(word.y, px * 1.08);
                items.push(
                    TextItem::new(word.text, px, (word.x, word.y))
                        .weight(700)
                        .color(linear_to_srgb8([1.0; 3], 0.55 * fade)),
                );
            }
        }

        let (Some(sung_at), Some(line)) = (at, now_line.and_then(|n| lyrics.lines.get(n))) else {
            return items;
        };
        let words: Vec<String> = line.words.iter().map(|w| w.text.clone()).collect();
        let lift = (1.0 - appear) * px * 0.5;
        for word in Self::layout(text, &words, px, width, centre_y + lift) {
            let index = word.index;
            cover(word.y, px * 1.08);
            let sung = lit(opts.style, line.word_timed, index, &sung_at);
            let (base, lit_col) = match opts.style {
                LyricStyle::Clean => ([1.0; 3], [1.0; 3]),
                LyricStyle::Glow => (mix([1.0; 3], accent, 0.35), accent),
                LyricStyle::Karaoke => ([1.0; 3], accent),
            };
            let mut col = mix(base, lit_col, sung);
            let mut alpha = appear
                * if opts.style == LyricStyle::Karaoke {
                    0.6 + 0.4 * sung
                } else {
                    1.0
                };
            // Drop motion: a rainbow sweep across the words, a wave, a lift on the kick.
            let hue = (index as f32 * 0.09 + frame.time_s * 0.2).fract();
            col = mix(col, hsv(hue, 0.75, 1.0), amount * 0.8);
            let wave = (frame.time_s * 2.4 + index as f32 * 0.7).sin() * px * 0.12 * amount;
            let bounce = kick * amount * px * 0.1;
            let (wx, wy) = (word.x, word.y + wave - bounce);
            alpha = alpha.clamp(0.0, 1.0);

            // Shadow, then an outline all round: the word reads on any scene, bright or
            // busy, whatever colour it takes.
            items.push(
                TextItem::new(word.text.as_str(), px, (wx + px * 0.035, wy + px * 0.045))
                    .weight(700)
                    .color([0, 0, 0, (170.0 * alpha) as u8]),
            );
            let o = px * OUTLINE;
            for (dx, dy) in [
                (-1.0f32, 0.0f32),
                (1.0, 0.0),
                (0.0, -1.0),
                (0.0, 1.0),
                (-0.7, -0.7),
                (0.7, -0.7),
                (-0.7, 0.7),
                (0.7, 0.7),
            ] {
                items.push(
                    TextItem::new(word.text.as_str(), px, (wx + dx * o, wy + dy * o))
                        .weight(700)
                        .color([0, 0, 0, (220.0 * alpha) as u8]),
                );
            }
            // Glow: soft copies in the accent around the lit words.
            if opts.style == LyricStyle::Glow && sung > 0.0 {
                for (dx, dy) in [(-1.0f32, 0.0f32), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
                    items.push(
                        TextItem::new(
                            word.text.as_str(),
                            px,
                            (wx + dx * px * 0.03, wy + dy * px * 0.03),
                        )
                        .weight(700)
                        .color(linear_to_srgb8(accent, 0.22 * alpha)),
                    );
                }
            }
            // Colour split at drops: red one way, cyan the other.
            if amount > 0.05 {
                let split = px * 0.06 * amount;
                items.push(
                    TextItem::new(word.text.as_str(), px, (wx + split, wy))
                        .weight(700)
                        .color(linear_to_srgb8([1.0, 0.1, 0.2], 0.45 * amount * alpha)),
                );
                items.push(
                    TextItem::new(word.text.as_str(), px, (wx - split, wy))
                        .weight(700)
                        .color(linear_to_srgb8([0.1, 0.8, 1.0], 0.45 * amount * alpha)),
                );
            }
            items.push(
                TextItem::new(word.text, px, (wx, wy))
                    .weight(700)
                    .color(linear_to_srgb8(col, alpha)),
            );
        }

        // The next line waits below, small and dim (Glow and Karaoke).
        if opts.style != LyricStyle::Clean
            && let Some(next) = lyrics.lines.get(sung_at.line + 1)
        {
            let small = px * 0.55;
            let words: Vec<String> = next.words.iter().map(|w| w.text.clone()).collect();
            let rows = Self::layout(text, &words, small, width, 0.0);
            let below = centre_y
                + px * 1.25
                + if rows.iter().any(|p| p.y > rows[0].y) {
                    small * 0.6
                } else {
                    0.0
                };
            for word in Self::layout(text, &words, small, width, below) {
                cover(word.y, small * 1.08);
                items.push(
                    TextItem::new(word.text, small, (word.x, word.y))
                        .weight(500)
                        .color(linear_to_srgb8([1.0; 3], 0.4 * appear)),
                );
            }
        }

        if band_top < band_bottom {
            let pad = px * 0.6;
            self.draw_band(
                gpu,
                encoder,
                target,
                frame.size,
                (band_top - pad, band_bottom + pad),
                opts.backdrop.clamp(0.0, 1.0) * appear.max(0.5),
            );
        }
        items
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LRC: &str = "[00:10.00] first line here\n[00:14.00] second line\n[00:20.00] <00:20.00>timed <00:20.50>words\n";

    fn state(playhead: f64, lag_s: f32, lag_samples: u32) -> MusicState {
        let mut ms = MusicState {
            playhead_s: playhead,
            ..MusicState::default()
        };
        ms.hits.lag_s = lag_s;
        ms.hits.lag_samples = lag_samples;
        ms
    }

    #[test]
    fn hsv_hits_the_primaries() {
        let close = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4);
        assert!(close(hsv(0.0, 1.0, 1.0), [1.0, 0.0, 0.0]));
        assert!(close(hsv(1.0 / 3.0, 1.0, 1.0), [0.0, 1.0, 0.0]));
        assert!(close(hsv(2.0 / 3.0, 1.0, 1.0), [0.0, 0.0, 1.0]));
    }

    #[test]
    fn the_accent_is_never_dark() {
        let mut theme = [[0.0; 3]; 5];
        theme[2] = [0.02, 0.0, 0.1];
        let a = accent(&theme);
        assert!(a[2] > 0.9 && a[0] > 0.2, "{a:?}");
    }

    #[test]
    fn a_line_slides_in_before_its_stamp_and_is_sung_from_it() {
        let lyrics = Lyrics::parse_lrc(LRC, "");
        let opts = LyricsOptions::default();
        // 0.2 s before the second line's stamp: it is the line to show, nothing sung yet.
        let t = timing(&lyrics, &state(13.8, 0.0, 0), &opts);
        assert_eq!(t.line, Some(1), "{t:?}");
        let sung = t.sung.unwrap();
        assert_eq!((sung.line, sung.word), (1, 0));
        assert!(sung.word_progress.abs() < f32::EPSILON);
        // Before the slide-in window, the first line is still the one.
        assert_eq!(timing(&lyrics, &state(13.5, 0.0, 0), &opts).line, Some(0));
        // A second into the line, the words are under way.
        let t = timing(&lyrics, &state(15.0, 0.0, 0), &opts);
        assert_eq!(t.line, Some(1));
        assert!(t.sung.unwrap().since_s > 0.9);
    }

    #[test]
    fn the_measured_audio_lag_delays_the_lyrics_once_trusted() {
        let lyrics = Lyrics::parse_lrc(LRC, "");
        let opts = LyricsOptions::default();
        // 84 ms of lag, 24 ms of it analysis: the words wait 60 ms for the room.
        let lagged = timing(&lyrics, &state(15.0, 0.084, 40), &opts);
        assert!((lagged.playhead - 14.94).abs() < 1e-3, "{lagged:?}");
        // Too few kicks measured: no correction.
        let fresh = timing(&lyrics, &state(15.0, 0.084, 5), &opts);
        assert!((fresh.playhead - 15.0).abs() < 1e-3, "{fresh:?}");
        // A lag smaller than the analysis alone is not the room's.
        let tiny = timing(&lyrics, &state(15.0, 0.01, 40), &opts);
        assert!((tiny.playhead - 15.0).abs() < 1e-3, "{tiny:?}");
        // The DJ's offset still applies on top.
        let shifted = LyricsOptions {
            offset_s: 0.5,
            ..LyricsOptions::default()
        };
        let both = timing(&lyrics, &state(15.0, 0.084, 40), &shifted);
        assert!((both.playhead - 15.44).abs() < 1e-3, "{both:?}");
    }

    #[test]
    fn untimed_words_are_not_lit_one_by_one() {
        let lyrics = Lyrics::parse_lrc(LRC, "");
        let opts = LyricsOptions::default();
        // Line-synced: halfway through "first line here", Glow lights all three words.
        let t = timing(&lyrics, &state(12.0, 0.0, 0), &opts);
        let sung = t.sung.unwrap();
        assert!(!lyrics.lines[0].word_timed);
        assert!((0..3).all(|i| lit(LyricStyle::Glow, false, i, &sung) > 0.99));
        // Karaoke sweeps by line progress instead of guessing words.
        let k = lit(LyricStyle::Karaoke, false, 2, &sung);
        assert!(
            (k - sung.line_progress).abs() < 1e-6 && k > 0.4 && k < 0.6,
            "{k}"
        );
        // Word-timed: the second word lights only once it is sung.
        let t = timing(&lyrics, &state(20.2, 0.0, 0), &opts);
        let sung = t.sung.unwrap();
        assert!(lyrics.lines[2].word_timed);
        assert!(lit(LyricStyle::Glow, true, 0, &sung) > 0.99);
        assert!(lit(LyricStyle::Glow, true, 1, &sung) < 0.01);
        assert!(lit(LyricStyle::Clean, false, 1, &sung) > 0.99);
    }

    #[test]
    fn the_band_is_soft_edged_and_solid_in_the_middle() {
        let img = band_image(BAND_STEPS);
        let a = |y: u32| img.get_pixel(0, y)[3];
        assert_eq!(a(0), 0);
        assert_eq!(a(BAND_STEPS - 1), 0);
        assert_eq!(a(BAND_STEPS / 2), 255);
        assert!(a(4) > 0 && a(4) < a(8), "a soft ramp");
    }
}
