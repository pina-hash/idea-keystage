//! Recommended OBS Browser Source sizes for `server_urls` (ARCHITECTURE.md
//! section 6), the same numbers as the renderer's `obsSourceSize` in
//! `src/lib/renderer/sizes.ts` and `metrics.ts`: `cursor` and `all` are the
//! chosen monitor's pixel size; `keys`, `keyboard` and `mouse` are their
//! content box at the chosen scale, rounded up to even numbers. The settings
//! window computes these itself and uses ours only as a fallback; the tests
//! pin the shared constants.

use keystage_core::protocol::{CalloutSize, KeyboardLayout, KeyboardSize, OverlayConfig, Route};

/// OBS accepts 1 to 8192 for both fields.
pub const OBS_MAX: u32 = 8192;

fn callout_unit_px(size: CalloutSize) -> f64 {
    match size {
        CalloutSize::S => 34.0,
        CalloutSize::M => 46.0,
        CalloutSize::L => 58.0,
        CalloutSize::Xl => 76.0,
    }
}

const MAX_WIDTH_U: f64 = 18.0;
const PAD_Y_U: f64 = 0.24;
const CAP_U: f64 = 1.0;
const CAPTION_GAP_U: f64 = 0.14;
const CAPTION_LINE_U: f64 = 0.54;
const STACK_GAP_U: f64 = 0.22;
const BLEED_U: f64 = 0.6;
const INDICATOR_U: f64 = 0.9;

fn callout_height_u(captions: bool) -> f64 {
    let caption = if captions {
        CAPTION_GAP_U + CAPTION_LINE_U
    } else {
        0.0
    };
    PAD_Y_U * 2.0 + CAP_U + caption
}

fn keyboard_unit_px(size: KeyboardSize) -> f64 {
    match size {
        KeyboardSize::S => 28.0,
        KeyboardSize::M => 38.0,
        KeyboardSize::L => 50.0,
    }
}

fn footprint(layout: KeyboardLayout) -> (f64, f64) {
    match layout {
        KeyboardLayout::Full => (23.5, 7.5),
        KeyboardLayout::Tkl => (19.25, 7.5),
        KeyboardLayout::Seventy5 => (17.0, 7.25),
        KeyboardLayout::Sixty => (16.0, 6.0),
        KeyboardLayout::Laptop => (16.0, 7.0),
        KeyboardLayout::Left => (11.75, 7.5),
    }
}

const KEYBOARD_MOUSE_W: f64 = 4.0;
const MOUSE_HEIGHT_PX: f64 = 160.0;
const MOUSE_ASPECT: f64 = 0.64;
const MOUSE_BLEED_PX: f64 = 16.0;

/// Rounds up to an even number in 2..=8192.
pub fn even_ceil(n: f64) -> u32 {
    let c = (n - 1e-9).ceil();
    let c = if c.is_finite() { c.max(0.0) } else { 2.0 };
    let c = c as u64;
    let even = if c.is_multiple_of(2) { c } else { c + 1 };
    even.clamp(2, u64::from(OBS_MAX)) as u32
}

fn scale_of(config: &OverlayConfig) -> f64 {
    if config.scale.is_finite() && config.scale > 0.0 {
        config.scale
    } else {
        1.0
    }
}

/// The recommended source size for a route.
pub fn obs_source_size(route: Route, config: &OverlayConfig, monitor: (u32, u32)) -> (u32, u32) {
    let s = scale_of(config);
    match route {
        Route::Cursor | Route::All => (monitor.0.clamp(1, OBS_MAX), monitor.1.clamp(1, OBS_MAX)),
        Route::Keys => {
            let c = &config.callouts;
            let u = callout_unit_px(c.size) * s;
            let n = f64::from(c.max_stack.max(1));
            let rows = n * callout_height_u(c.captions) + (n - 1.0) * STACK_GAP_U;
            let w = (MAX_WIDTH_U + 2.0 * BLEED_U) * u + c.offset_x.abs() * s;
            let h = (rows + STACK_GAP_U + INDICATOR_U + 2.0 * BLEED_U) * u + c.offset_y.abs() * s;
            (even_ceil(w), even_ceil(h))
        }
        Route::Keyboard => {
            let k = &config.keyboard;
            let unit = keyboard_unit_px(k.size) * s;
            let (fw, fh) = footprint(k.layout);
            let w = fw + if k.mouse { KEYBOARD_MOUSE_W } else { 0.0 };
            (
                even_ceil(w * unit + k.offset_x.abs() * s),
                even_ceil(fh * unit + k.offset_y.abs() * s),
            )
        }
        Route::Mouse => {
            let m = &config.mouse;
            let h = MOUSE_HEIGHT_PX * s;
            let bleed = MOUSE_BLEED_PX * s;
            (
                even_ceil(h * MOUSE_ASPECT + 2.0 * bleed + m.offset_x.abs() * s),
                even_ceil(h + 2.0 * bleed + m.offset_y.abs() * s),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn even_rounding() {
        assert_eq!(even_ceil(1045.0), 1046);
        assert_eq!(even_ceil(1046.0), 1046);
        assert_eq!(even_ceil(1046.0000000001), 1046);
        assert_eq!(even_ceil(0.2), 2);
        assert_eq!(even_ceil(99999.0), OBS_MAX);
        assert_eq!(even_ceil(f64::NAN), 2);
    }

    #[test]
    fn keyboard_matches_the_visual_box() {
        // visuals notes: full layout plus the mouse at size m is 1046 x 286.
        let mut c = OverlayConfig::default();
        c.keyboard.offset_x = 0.0;
        c.keyboard.offset_y = 0.0;
        c.keyboard.layout = KeyboardLayout::Full;
        c.keyboard.mouse = true;
        c.keyboard.size = KeyboardSize::M;
        assert_eq!(
            obs_source_size(Route::Keyboard, &c, (1920, 1080)),
            (1046, 286)
        );
        c.scale = 2.0;
        assert_eq!(
            obs_source_size(Route::Keyboard, &c, (1920, 1080)),
            (2090, 570)
        );
    }

    #[test]
    fn full_screen_routes_use_the_monitor() {
        let c = OverlayConfig::default();
        assert_eq!(obs_source_size(Route::All, &c, (3840, 2160)), (3840, 2160));
        assert_eq!(obs_source_size(Route::Cursor, &c, (0, 9000)), (1, OBS_MAX));
    }

    #[test]
    fn keys_and_mouse_are_even_and_grow_with_scale() {
        let mut c = OverlayConfig::default();
        let (w1, h1) = obs_source_size(Route::Keys, &c, (1920, 1080));
        assert_eq!((w1 % 2, h1 % 2), (0, 0));
        c.scale = 2.0;
        let (w2, h2) = obs_source_size(Route::Keys, &c, (1920, 1080));
        assert!(w2 > w1 && h2 > h1);
        let mut m = OverlayConfig::default();
        m.mouse.offset_x = 0.0;
        m.mouse.offset_y = 0.0;
        // 160 * 0.64 + 32 = 134.4 -> 136; 160 + 32 = 192.
        assert_eq!(obs_source_size(Route::Mouse, &m, (1920, 1080)), (136, 192));
    }

    #[test]
    fn callout_box_by_hand() {
        let mut c = OverlayConfig {
            scale: 1.0,
            ..OverlayConfig::default()
        };
        c.callouts.size = CalloutSize::M;
        c.callouts.max_stack = 1;
        c.callouts.captions = false;
        c.callouts.offset_x = 0.0;
        c.callouts.offset_y = 0.0;
        // width (18 + 1.2) * 46 = 883.2 -> 884; height (1.48 + 0.22 + 0.9 + 1.2) * 46 = 174.8 -> 176.
        assert_eq!(obs_source_size(Route::Keys, &c, (1920, 1080)), (884, 176));
    }
}
