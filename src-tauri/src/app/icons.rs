//! The tray icon glyphs, drawn in code at the exact pixel size Windows uses
//! for the notification area (16 px at 100 percent scaling, 20 at 125, 24 at
//! 150, 32 at 200), so they stay crisp instead of being scaled from one
//! bitmap. Every edge is placed on whole pixels per size and the shapes are
//! antialiased with 4 by 4 supersampling.
//!
//! The glyph is an original keycap (no third-party artwork):
//! * normal: a keycap with a lit green face;
//! * paused: the same keycap with an amber rim and a pause sign (two bars)
//!   instead of the lit face, so it differs by shape, not only by color;
//! * recording: the lit keycap with a round red record badge.

use super::state::TrayGlyph;

/// Pixel sizes the tray may ask for.
pub const SIZES: [u32; 6] = [16, 20, 24, 32, 40, 48];

/// The tray icon size for a monitor scale (1.0 = 100 percent).
pub fn size_for_scale(scale: f64) -> u32 {
    let want = if scale.is_finite() && scale > 0.0 {
        16.0 * scale
    } else {
        16.0
    };
    SIZES
        .iter()
        .copied()
        .find(|s| f64::from(*s) + 0.01 >= want)
        .unwrap_or(48)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Rgba(f32, f32, f32, f32);

const fn hex(rgb: u32) -> Rgba {
    Rgba(
        ((rgb >> 16) & 0xff) as f32 / 255.0,
        ((rgb >> 8) & 0xff) as f32 / 255.0,
        (rgb & 0xff) as f32 / 255.0,
        1.0,
    )
}

// IDEA palette (src/lib/styles/tokens.css): green #78b870, amber, crimson.
const BODY: Rgba = hex(0x1b201d);
const RIM_GREEN: Rgba = hex(0x78b870);
const RIM_AMBER: Rgba = hex(0xe0a35a);
const FACE_DARK: Rgba = hex(0x323a35);
const FACE_LIT: Rgba = hex(0x8fd086);
const PAUSE_BARS: Rgba = hex(0xf0b46a);
const REC_RED: Rgba = hex(0xe5484d);

#[derive(Clone, Copy, Debug)]
enum Shape {
    /// x0, y0, x1, y1 (pixel edges), corner radius.
    RoundRect(f32, f32, f32, f32, f32),
    /// Center and radius.
    Circle(f32, f32, f32),
}

impl Shape {
    fn contains(&self, x: f32, y: f32) -> bool {
        match *self {
            Shape::RoundRect(x0, y0, x1, y1, r) => {
                if x < x0 || x > x1 || y < y0 || y > y1 {
                    return false;
                }
                let r = r.min((x1 - x0) / 2.0).min((y1 - y0) / 2.0).max(0.0);
                let cx = x.clamp(x0 + r, x1 - r);
                let cy = y.clamp(y0 + r, y1 - r);
                let (dx, dy) = (x - cx, y - cy);
                dx * dx + dy * dy <= r * r
            }
            Shape::Circle(cx, cy, r) => {
                let (dx, dy) = (x - cx, y - cy);
                dx * dx + dy * dy <= r * r
            }
        }
    }
}

/// Integer pixel metrics for one size.
#[derive(Clone, Copy, Debug)]
struct Metrics {
    s: f32,
    margin: f32,
    rim: f32,
    radius: f32,
    gap: f32,
    skirt: f32,
    face_radius: f32,
}

fn metrics(size: u32) -> Metrics {
    let s = size as f32;
    let u = s / 16.0;
    let px = |v: f32| v.round().max(1.0);
    Metrics {
        s,
        margin: px(u),
        rim: px(u),
        radius: (3.0 * u).round(),
        gap: px(u),
        skirt: px(1.5 * u),
        face_radius: (1.5 * u).round(),
    }
}

/// The face rectangle (x0, y0, x1, y1).
fn face_rect(m: &Metrics) -> (f32, f32, f32, f32) {
    let inset = m.margin + m.rim + m.gap;
    (inset, inset, m.s - inset, m.s - inset - m.skirt)
}

fn layers(glyph: TrayGlyph, size: u32) -> Vec<(Shape, Rgba)> {
    let m = metrics(size);
    let (x0, y0, x1, y1) = (m.margin, m.margin, m.s - m.margin, m.s - m.margin);
    let rim = if glyph == TrayGlyph::Paused {
        RIM_AMBER
    } else {
        RIM_GREEN
    };
    let mut out = vec![
        (Shape::RoundRect(x0, y0, x1, y1, m.radius), rim),
        (
            Shape::RoundRect(
                x0 + m.rim,
                y0 + m.rim,
                x1 - m.rim,
                y1 - m.rim,
                (m.radius - m.rim).max(0.0),
            ),
            BODY,
        ),
    ];
    let (fx0, fy0, fx1, fy1) = face_rect(&m);
    match glyph {
        TrayGlyph::Normal | TrayGlyph::Recording => {
            out.push((
                Shape::RoundRect(fx0, fy0, fx1, fy1, m.face_radius),
                FACE_LIT,
            ));
        }
        TrayGlyph::Paused => {
            out.push((
                Shape::RoundRect(fx0, fy0, fx1, fy1, m.face_radius),
                FACE_DARK,
            ));
            let face_w = fx1 - fx0;
            let bar_w = (2.0 * m.s / 16.0).round().max(2.0);
            let total = bar_w * 3.0;
            let left = fx0 + ((face_w - total) / 2.0).round();
            let inset = (m.s / 16.0).round().max(1.0);
            let (by0, by1) = (fy0 + inset, fy1 - inset);
            out.push((
                Shape::RoundRect(left, by0, left + bar_w, by1, 0.0),
                PAUSE_BARS,
            ));
            out.push((
                Shape::RoundRect(left + 2.0 * bar_w, by0, left + 3.0 * bar_w, by1, 0.0),
                PAUSE_BARS,
            ));
        }
    }
    if glyph == TrayGlyph::Recording {
        let r = (3.0 * m.s / 16.0).round().max(3.0);
        let ring = (m.s / 16.0).round().max(1.0);
        let c = m.s - r - ring;
        out.push((Shape::Circle(c, c, r + ring), BODY));
        out.push((Shape::Circle(c, c, r), REC_RED));
    }
    out
}

/// Renders a glyph as straight (not premultiplied) RGBA, row by row.
pub fn render(glyph: TrayGlyph, size: u32) -> Vec<u8> {
    let size = size.clamp(8, 256);
    let layers = layers(glyph, size);
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    const N: u32 = 4;
    for py in 0..size {
        for px in 0..size {
            // Premultiplied accumulation over the supersamples.
            let mut acc = [0.0f32; 4];
            for sy in 0..N {
                for sx in 0..N {
                    let x = px as f32 + (sx as f32 + 0.5) / N as f32;
                    let y = py as f32 + (sy as f32 + 0.5) / N as f32;
                    let mut c = [0.0f32; 4];
                    for (shape, color) in &layers {
                        if shape.contains(x, y) {
                            let a = color.3;
                            c[0] = color.0 * a + c[0] * (1.0 - a);
                            c[1] = color.1 * a + c[1] * (1.0 - a);
                            c[2] = color.2 * a + c[2] * (1.0 - a);
                            c[3] = a + c[3] * (1.0 - a);
                        }
                    }
                    for i in 0..4 {
                        acc[i] += c[i];
                    }
                }
            }
            let n = (N * N) as f32;
            let a = acc[3] / n;
            let to_u8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            if a <= 0.0 {
                out.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                out.push(to_u8(acc[0] / n / a));
                out.push(to_u8(acc[1] / n / a));
                out.push(to_u8(acc[2] / n / a));
                out.push(to_u8(a));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(img: &[u8], size: u32, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * size + x) * 4) as usize;
        [img[i], img[i + 1], img[i + 2], img[i + 3]]
    }

    fn close(a: [u8; 4], c: Rgba) -> bool {
        let e = |v: u8, f: f32| (f32::from(v) - f * 255.0).abs() <= 3.0;
        a[3] == 255 && e(a[0], c.0) && e(a[1], c.1) && e(a[2], c.2)
    }

    #[test]
    fn sizes_follow_scaling() {
        assert_eq!(size_for_scale(1.0), 16);
        assert_eq!(size_for_scale(1.25), 20);
        assert_eq!(size_for_scale(1.5), 24);
        assert_eq!(size_for_scale(1.75), 32);
        assert_eq!(size_for_scale(2.0), 32);
        assert_eq!(size_for_scale(2.5), 40);
        assert_eq!(size_for_scale(3.0), 48);
        assert_eq!(size_for_scale(5.0), 48);
        assert_eq!(size_for_scale(f64::NAN), 16);
    }

    #[test]
    fn every_glyph_at_every_size_is_well_formed() {
        for size in SIZES {
            for g in [TrayGlyph::Normal, TrayGlyph::Paused, TrayGlyph::Recording] {
                let img = render(g, size);
                assert_eq!(img.len(), (size * size * 4) as usize);
                assert_eq!(px(&img, size, 0, 0)[3], 0, "corners are clear");
                let mid = px(&img, size, size / 2, size / 2);
                assert_eq!(mid[3], 255, "the cap is opaque at {size}");
            }
        }
    }

    #[test]
    fn normal_has_a_lit_face() {
        for size in [16u32, 20, 24, 32] {
            let img = render(TrayGlyph::Normal, size);
            let m = metrics(size);
            let (fx0, fy0, fx1, fy1) = face_rect(&m);
            let cx = ((fx0 + fx1) / 2.0) as u32;
            let cy = ((fy0 + fy1) / 2.0) as u32;
            assert!(close(px(&img, size, cx, cy), FACE_LIT), "size {size}");
            // The rim is drawn on whole pixels: the middle of the left edge is
            // fully rim colored.
            assert!(close(px(&img, size, m.margin as u32, size / 2), RIM_GREEN));
        }
    }

    #[test]
    fn paused_differs_by_shape() {
        for size in [16u32, 20, 24, 32] {
            let normal = render(TrayGlyph::Normal, size);
            let paused = render(TrayGlyph::Paused, size);
            let m = metrics(size);
            let (fx0, fy0, fx1, fy1) = face_rect(&m);
            let cy = ((fy0 + fy1) / 2.0) as u32;
            // Walk across the face: the lit face is one run; the pause sign is
            // bar, gap, bar.
            let row = |img: &[u8], c: Rgba| {
                (fx0 as u32..fx1 as u32)
                    .map(|x| close(px(img, size, x, cy), c))
                    .collect::<Vec<bool>>()
            };
            let lit = row(&normal, FACE_LIT);
            assert!(lit.iter().all(|b| *b), "size {size}: {lit:?}");
            let bars = row(&paused, PAUSE_BARS);
            let runs = bars.windows(2).filter(|w| w[0] != w[1]).count();
            assert_eq!(runs, 4, "size {size}: two separate bars {bars:?}");
            let on: usize = bars.iter().filter(|b| **b).count();
            assert!(on >= 4, "bars are at least 2 px wide at {size}");
        }
    }

    #[test]
    fn recording_has_a_badge() {
        for size in [16u32, 20, 24, 32] {
            let img = render(TrayGlyph::Recording, size);
            let normal = render(TrayGlyph::Normal, size);
            let r = (3.0 * size as f32 / 16.0).round().max(3.0);
            let ring = (size as f32 / 16.0).round().max(1.0);
            let c = (size as f32 - r - ring) as u32;
            assert!(close(px(&img, size, c, c), REC_RED), "size {size}");
            assert!(!close(px(&normal, size, c, c), REC_RED));
            assert_ne!(img, normal);
        }
    }
}
