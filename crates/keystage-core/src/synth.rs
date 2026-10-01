//! Original procedural mechanical key sounds.
//!
//! Every sound is synthesized here from first principles (filtered noise
//! transients, damped resonant modes, exponential envelopes). No recorded
//! samples and no third-party sound assets are used. Output is deterministic,
//! 48 kHz mono `f32`, normalized to a per-sound peak, shorter than 120 ms,
//! and starts and ends at exactly zero with smooth ramps so playback never
//! clicks.
//!
//! Each pack has four sounds: key down, key up, and a wide variant of each
//! for the stabilized keys (Space, Enter, numpad Enter, Backspace), which
//! sound lower and longer with a small stabilizer rattle.

use serde::{Deserialize, Serialize};

/// Output sample rate.
pub const SAMPLE_RATE: u32 = 48_000;
/// Every sound is shorter than this.
pub const MAX_LEN_MS: u32 = 120;
/// Number of subtly different takes of each sound, so fast typing does not
/// sound like a machine gun. Variant numbers wrap around.
pub const VARIANTS: u32 = 4;

/// Which sound of a pack.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoundKind {
    Down,
    Up,
    /// Space, Enter, numpad Enter and Backspace going down.
    WideDown,
    /// The same keys coming up.
    WideUp,
}

impl SoundKind {
    pub const ALL: [SoundKind; 4] = [
        SoundKind::Down,
        SoundKind::Up,
        SoundKind::WideDown,
        SoundKind::WideUp,
    ];

    /// The sound for a canonical key name.
    pub fn for_key(key: &str, down: bool) -> SoundKind {
        let wide = matches!(key, "Space" | "Enter" | "NumEnter" | "Backspace");
        match (wide, down) {
            (true, true) => SoundKind::WideDown,
            (true, false) => SoundKind::WideUp,
            (false, true) => SoundKind::Down,
            (false, false) => SoundKind::Up,
        }
    }
}

/// A sound pack as listed in settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct PackInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
}

/// All packs, in the order settings lists them.
pub const PACKS: [PackInfo; 12] = [
    PackInfo {
        id: "soft-linear",
        name: "Soft linear",
        description: "Smooth, rounded and quiet.",
    },
    PackInfo {
        id: "tactile",
        name: "Tactile",
        description: "A small bump before each bottom out.",
    },
    PackInfo {
        id: "clicky",
        name: "Clicky",
        description: "Crisp double click on every press.",
    },
    PackInfo {
        id: "deep",
        name: "Deep",
        description: "Low, full and heavy.",
    },
    PackInfo {
        id: "typewriter",
        name: "Typewriter",
        description: "Metal strike over a wooden thud.",
    },
    PackInfo {
        id: "bubble",
        name: "Bubble",
        description: "Soft pitched pops.",
    },
    PackInfo {
        id: "ceramic",
        name: "Ceramic",
        description: "Bright, glassy ring.",
    },
    PackInfo {
        id: "muted",
        name: "Muted",
        description: "Barely there, for quiet rooms.",
    },
    PackInfo {
        id: "laptop",
        name: "Laptop",
        description: "Short, tight scissor switches.",
    },
    PackInfo {
        id: "studio",
        name: "Studio",
        description: "Balanced and warm, sits under a voice.",
    },
    PackInfo {
        id: "hollow",
        name: "Hollow",
        description: "Resonant case with a long tail.",
    },
    PackInfo {
        id: "pebble",
        name: "Pebble",
        description: "Dry stone clack.",
    },
];

/// The default pack id.
pub const DEFAULT_PACK: &str = "soft-linear";

/// Looks up a pack by id.
pub fn pack(id: &str) -> Option<&'static PackInfo> {
    PACKS.iter().find(|p| p.id == id)
}

#[derive(Clone, Copy, Debug)]
struct Mode {
    f: f64,
    decay_ms: f64,
    amp: f64,
}

const fn m(f: f64, decay_ms: f64, amp: f64) -> Mode {
    Mode { f, decay_ms, amp }
}

const NO_MODE: Mode = m(0.0, 0.0, 0.0);

#[derive(Clone, Copy, Debug)]
struct Tick {
    amp: f64,
    decay_ms: f64,
    bp_hz: f64,
    q: f64,
}

#[derive(Clone, Copy, Debug)]
struct Voice {
    len_ms: f64,
    peak: f64,
    tick: Tick,
    /// A second transient: the click jacket of clicky switches, the bump of
    /// tactile ones, or a stabilizer rattle. `(at_ms, amp, bp_hz)`.
    tick2: Option<(f64, f64, f64)>,
    modes: [Mode; 3],
    /// Pitch ratio reached by the end of the sound (1 = steady).
    sweep: f64,
    noise_amp: f64,
    noise_lp_hz: f64,
    noise_decay_ms: f64,
    /// The body (modes and noise) starts this late: switch travel.
    body_delay_ms: f64,
}

#[allow(clippy::too_many_arguments)]
const fn v(
    len_ms: f64,
    peak: f64,
    tick: (f64, f64, f64, f64),
    tick2: Option<(f64, f64, f64)>,
    modes: [Mode; 3],
    sweep: f64,
    noise: (f64, f64, f64),
    body_delay_ms: f64,
) -> Voice {
    Voice {
        len_ms,
        peak,
        tick: Tick {
            amp: tick.0,
            decay_ms: tick.1,
            bp_hz: tick.2,
            q: tick.3,
        },
        tick2,
        modes,
        sweep,
        noise_amp: noise.0,
        noise_lp_hz: noise.1,
        noise_decay_ms: noise.2,
        body_delay_ms,
    }
}

/// `(down, up)` voices per pack, in `PACKS` order.
const VOICES: [(Voice, Voice); 12] = [
    // Soft linear
    (
        v(
            78.0,
            0.85,
            (0.30, 2.2, 2400.0, 0.8),
            None,
            [
                m(230.0, 20.0, 1.0),
                m(520.0, 11.0, 0.45),
                m(1180.0, 5.0, 0.18),
            ],
            0.97,
            (0.28, 2800.0, 9.0),
            0.6,
        ),
        v(
            52.0,
            0.50,
            (0.25, 1.6, 3200.0, 0.9),
            None,
            [
                m(410.0, 10.0, 0.8),
                m(880.0, 6.0, 0.35),
                m(2000.0, 3.0, 0.12),
            ],
            1.0,
            (0.22, 3600.0, 5.0),
            0.3,
        ),
    ),
    // Tactile
    (
        v(
            88.0,
            0.88,
            (0.55, 1.6, 3000.0, 1.4),
            Some((7.0, 0.40, 2200.0)),
            [
                m(260.0, 17.0, 1.0),
                m(600.0, 9.0, 0.5),
                m(1500.0, 4.0, 0.25),
            ],
            0.96,
            (0.30, 3500.0, 7.0),
            7.0,
        ),
        v(
            58.0,
            0.55,
            (0.45, 1.4, 3600.0, 1.2),
            None,
            [
                m(470.0, 9.0, 0.7),
                m(1100.0, 5.0, 0.35),
                m(2600.0, 2.5, 0.15),
            ],
            1.0,
            (0.25, 4200.0, 4.0),
            0.3,
        ),
    ),
    // Clicky
    (
        v(
            85.0,
            0.90,
            (0.90, 1.0, 4800.0, 3.0),
            Some((5.0, 0.70, 4200.0)),
            [
                m(300.0, 14.0, 0.6),
                m(720.0, 8.0, 0.35),
                m(1900.0, 4.0, 0.2),
            ],
            1.0,
            (0.25, 5000.0, 5.0),
            5.5,
        ),
        v(
            60.0,
            0.65,
            (0.85, 0.9, 5200.0, 3.0),
            Some((4.0, 0.50, 4600.0)),
            [m(520.0, 7.0, 0.4), m(1300.0, 4.0, 0.2), m(3100.0, 2.0, 0.1)],
            1.0,
            (0.20, 6000.0, 3.0),
            0.5,
        ),
    ),
    // Deep
    (
        v(
            100.0,
            0.90,
            (0.20, 2.8, 1800.0, 0.7),
            None,
            [m(140.0, 32.0, 1.0), m(300.0, 18.0, 0.5), m(640.0, 8.0, 0.2)],
            0.94,
            (0.32, 1400.0, 12.0),
            0.8,
        ),
        v(
            65.0,
            0.50,
            (0.20, 2.2, 2400.0, 0.7),
            None,
            [
                m(260.0, 13.0, 0.8),
                m(560.0, 8.0, 0.35),
                m(1200.0, 4.0, 0.12),
            ],
            1.0,
            (0.25, 2200.0, 6.0),
            0.4,
        ),
    ),
    // Typewriter
    (
        v(
            105.0,
            0.90,
            (0.70, 1.4, 3800.0, 2.0),
            None,
            [
                m(1850.0, 26.0, 0.55),
                m(2930.0, 20.0, 0.4),
                m(4170.0, 12.0, 0.25),
            ],
            1.0,
            (0.45, 900.0, 10.0),
            0.3,
        ),
        v(
            70.0,
            0.60,
            (0.50, 1.2, 4400.0, 2.0),
            None,
            [
                m(2200.0, 10.0, 0.4),
                m(3400.0, 7.0, 0.25),
                m(160.0, 9.0, 0.5),
            ],
            1.0,
            (0.30, 1200.0, 6.0),
            0.2,
        ),
    ),
    // Bubble
    (
        v(
            75.0,
            0.85,
            (0.15, 1.5, 2000.0, 0.7),
            None,
            [m(880.0, 22.0, 1.0), m(1760.0, 10.0, 0.25), NO_MODE],
            0.55,
            (0.08, 2000.0, 4.0),
            0.3,
        ),
        v(
            50.0,
            0.55,
            (0.12, 1.2, 2600.0, 0.7),
            None,
            [m(1240.0, 14.0, 0.9), m(2480.0, 6.0, 0.2), NO_MODE],
            1.45,
            (0.06, 2600.0, 3.0),
            0.2,
        ),
    ),
    // Ceramic
    (
        v(
            95.0,
            0.85,
            (0.50, 1.0, 5200.0, 1.5),
            None,
            [
                m(2450.0, 30.0, 0.7),
                m(3720.0, 22.0, 0.45),
                m(5100.0, 12.0, 0.25),
            ],
            1.0,
            (0.15, 7000.0, 4.0),
            0.2,
        ),
        v(
            65.0,
            0.55,
            (0.45, 0.9, 6000.0, 1.5),
            None,
            [
                m(2950.0, 16.0, 0.6),
                m(4400.0, 11.0, 0.35),
                m(6200.0, 7.0, 0.2),
            ],
            1.0,
            (0.12, 8000.0, 3.0),
            0.2,
        ),
    ),
    // Muted
    (
        v(
            70.0,
            0.60,
            (0.12, 3.0, 1200.0, 0.6),
            None,
            [m(190.0, 14.0, 1.0), m(420.0, 8.0, 0.3), NO_MODE],
            1.0,
            (0.35, 900.0, 10.0),
            1.0,
        ),
        v(
            48.0,
            0.38,
            (0.10, 2.4, 1600.0, 0.6),
            None,
            [m(330.0, 8.0, 0.8), m(700.0, 5.0, 0.25), NO_MODE],
            1.0,
            (0.30, 1300.0, 6.0),
            0.5,
        ),
    ),
    // Laptop
    (
        v(
            48.0,
            0.80,
            (0.70, 0.8, 3200.0, 1.2),
            None,
            [
                m(640.0, 6.0, 0.6),
                m(1450.0, 4.0, 0.35),
                m(3100.0, 2.0, 0.2),
            ],
            1.0,
            (0.30, 6000.0, 3.0),
            0.3,
        ),
        v(
            36.0,
            0.50,
            (0.60, 0.7, 3900.0, 1.2),
            None,
            [m(900.0, 4.0, 0.5), m(2000.0, 3.0, 0.25), NO_MODE],
            1.0,
            (0.25, 7000.0, 2.0),
            0.2,
        ),
    ),
    // Studio
    (
        v(
            82.0,
            0.82,
            (0.35, 1.8, 2700.0, 1.0),
            None,
            [
                m(330.0, 16.0, 1.0),
                m(700.0, 10.0, 0.4),
                m(1600.0, 5.0, 0.15),
            ],
            0.98,
            (0.20, 3200.0, 6.0),
            0.5,
        ),
        v(
            55.0,
            0.52,
            (0.30, 1.5, 3300.0, 1.0),
            None,
            [m(560.0, 9.0, 0.7), m(1180.0, 6.0, 0.3), m(2500.0, 3.0, 0.1)],
            1.0,
            (0.18, 4000.0, 4.0),
            0.3,
        ),
    ),
    // Hollow
    (
        v(
            110.0,
            0.88,
            (0.30, 2.0, 2200.0, 0.9),
            None,
            [
                m(255.0, 40.0, 1.0),
                m(515.0, 26.0, 0.55),
                m(1030.0, 10.0, 0.2),
            ],
            1.0,
            (0.20, 2000.0, 8.0),
            0.6,
        ),
        v(
            72.0,
            0.55,
            (0.28, 1.6, 2900.0, 0.9),
            None,
            [
                m(380.0, 20.0, 0.8),
                m(770.0, 12.0, 0.4),
                m(1540.0, 6.0, 0.15),
            ],
            1.0,
            (0.18, 2600.0, 5.0),
            0.3,
        ),
    ),
    // Pebble
    (
        v(
            60.0,
            0.85,
            (0.90, 1.6, 1600.0, 1.8),
            None,
            [m(900.0, 8.0, 0.6), m(2100.0, 5.0, 0.3), NO_MODE],
            1.0,
            (0.40, 2500.0, 4.0),
            0.3,
        ),
        v(
            42.0,
            0.55,
            (0.80, 1.3, 2100.0, 1.8),
            None,
            [m(1250.0, 5.0, 0.5), m(2700.0, 3.0, 0.25), NO_MODE],
            1.0,
            (0.35, 3200.0, 3.0),
            0.2,
        ),
    ),
];

/// The wide (stabilized key) form of a voice: lower, longer, with a rattle.
fn widen(base: Voice, rattle_ms: f64) -> Voice {
    let mut w = base;
    w.len_ms = (base.len_ms * 1.3).min(f64::from(MAX_LEN_MS) - 5.0);
    for mode in w.modes.iter_mut() {
        mode.f *= 0.72;
        mode.decay_ms *= 1.5;
    }
    w.noise_lp_hz *= 0.75;
    w.noise_decay_ms *= 1.5;
    w.tick.bp_hz *= 0.8;
    w.peak = (base.peak * 1.04).min(0.93);
    if w.tick2.is_none() {
        w.tick2 = Some((rattle_ms, base.tick.amp * 0.6 + 0.1, base.tick.bp_hz * 0.7));
    } else if let Some((at, amp, bp)) = base.tick2 {
        w.tick2 = Some((at * 1.3, amp, bp * 0.8));
    }
    w
}

fn voice(pack_index: usize, kind: SoundKind) -> Voice {
    let (down, up) = VOICES[pack_index];
    match kind {
        SoundKind::Down => down,
        SoundKind::Up => up,
        SoundKind::WideDown => widen(down, 9.0),
        SoundKind::WideUp => widen(up, 6.0),
    }
}

/// xorshift64*: small, fast and deterministic.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform in [-1, 1).
    fn bipolar(&mut self) -> f64 {
        ((self.next_u64() >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0
    }
}

/// RBJ cookbook biquad, transposed direct form II.
#[derive(Clone, Copy)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Biquad {
    fn from_coeffs(b0: f64, b1: f64, b2: f64, a0: f64, a1: f64, a2: f64) -> Biquad {
        Biquad {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    fn omega(f: f64) -> f64 {
        let nyq = f64::from(SAMPLE_RATE) / 2.0;
        2.0 * std::f64::consts::PI * f.clamp(10.0, nyq * 0.95) / f64::from(SAMPLE_RATE)
    }

    fn lowpass(f: f64, q: f64) -> Biquad {
        let w = Self::omega(f);
        let (s, c) = w.sin_cos();
        let alpha = s / (2.0 * q);
        Self::from_coeffs(
            (1.0 - c) / 2.0,
            1.0 - c,
            (1.0 - c) / 2.0,
            1.0 + alpha,
            -2.0 * c,
            1.0 - alpha,
        )
    }

    fn highpass(f: f64, q: f64) -> Biquad {
        let w = Self::omega(f);
        let (s, c) = w.sin_cos();
        let alpha = s / (2.0 * q);
        Self::from_coeffs(
            (1.0 + c) / 2.0,
            -(1.0 + c),
            (1.0 + c) / 2.0,
            1.0 + alpha,
            -2.0 * c,
            1.0 - alpha,
        )
    }

    /// Band-pass with 0 dB peak gain.
    fn bandpass(f: f64, q: f64) -> Biquad {
        let w = Self::omega(f);
        let (s, c) = w.sin_cos();
        let alpha = s / (2.0 * q);
        Self::from_coeffs(alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * c, 1.0 - alpha)
    }

    fn process(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

fn ms_to_samples(ms: f64) -> usize {
    (ms * f64::from(SAMPLE_RATE) / 1000.0).round().max(0.0) as usize
}

/// Raised-cosine ramp from 0 to 1 over `len` samples.
fn ramp(i: usize, len: usize) -> f64 {
    if len == 0 || i >= len {
        1.0
    } else {
        0.5 - 0.5 * (std::f64::consts::PI * i as f64 / len as f64).cos()
    }
}

fn add_tick(
    out: &mut [f64],
    rng: &mut Rng,
    start: usize,
    amp: f64,
    decay_ms: f64,
    bp_hz: f64,
    q: f64,
) {
    let mut bp = Biquad::bandpass(bp_hz, q);
    let decay = decay_ms.max(0.05) * f64::from(SAMPLE_RATE) / 1000.0;
    let rise = ms_to_samples(0.15).max(2);
    let len = (decay * 7.0) as usize;
    for i in 0..len {
        let idx = start + i;
        if idx >= out.len() {
            break;
        }
        let env = (-(i as f64) / decay).exp() * ramp(i, rise);
        out[idx] += bp.process(rng.bipolar()) * env * amp * 2.5;
    }
}

fn synthesize(voice: &Voice, seed: u64) -> Vec<f32> {
    let mut rng = Rng::new(seed);
    let n = ms_to_samples(voice.len_ms).min(ms_to_samples(f64::from(MAX_LEN_MS)) - 1);
    let mut out = vec![0.0f64; n];
    let sr = f64::from(SAMPLE_RATE);
    // Small per-variant detune keeps repeated presses from sounding identical.
    let detune = 1.0 + 0.02 * rng.bipolar();

    add_tick(
        &mut out,
        &mut rng,
        0,
        voice.tick.amp,
        voice.tick.decay_ms,
        voice.tick.bp_hz * detune,
        voice.tick.q,
    );
    if let Some((at_ms, amp, bp_hz)) = voice.tick2 {
        let q = voice.tick.q.max(0.8);
        add_tick(
            &mut out,
            &mut rng,
            ms_to_samples(at_ms),
            amp,
            voice.tick.decay_ms * 0.8,
            bp_hz * detune,
            q,
        );
    }

    let body_start = ms_to_samples(voice.body_delay_ms);
    let body_rise = ms_to_samples(0.4).max(2);
    let body_len = n.saturating_sub(body_start).max(1);
    for mode in voice.modes.iter().filter(|m| m.amp > 0.0 && m.f > 0.0) {
        let decay = mode.decay_ms * sr / 1000.0;
        let f0 = mode.f * detune;
        let mut phase = 0.0f64;
        for i in 0..body_len {
            let idx = body_start + i;
            if idx >= n {
                break;
            }
            let progress = i as f64 / body_len as f64;
            let f = f0 * voice.sweep.powf(progress);
            let env = (-(i as f64) / decay).exp() * ramp(i, body_rise);
            out[idx] += phase.sin() * env * mode.amp;
            phase += 2.0 * std::f64::consts::PI * f / sr;
        }
    }
    if voice.noise_amp > 0.0 {
        let mut lp = Biquad::lowpass(voice.noise_lp_hz, 0.707);
        let decay = voice.noise_decay_ms * sr / 1000.0;
        for i in 0..body_len {
            let idx = body_start + i;
            if idx >= n {
                break;
            }
            let env = (-(i as f64) / decay).exp() * ramp(i, body_rise);
            out[idx] += lp.process(rng.bipolar()) * env * voice.noise_amp;
        }
    }

    // Remove any DC and sub-bass rumble, then shape the ends.
    let mut hp = Biquad::highpass(40.0, 0.707);
    for s in out.iter_mut() {
        *s = hp.process(*s);
    }
    let attack = ms_to_samples(0.5).max(4);
    let release = ms_to_samples(5.0).min(n / 2).max(4);
    for (i, s) in out.iter_mut().enumerate() {
        *s *= ramp(i, attack);
        let from_end = n - 1 - i;
        *s *= ramp(from_end, release);
    }

    let peak = out.iter().fold(0.0f64, |a, s| a.max(s.abs()));
    let gain = if peak > 1e-9 { voice.peak / peak } else { 0.0 };
    let mut result: Vec<f32> = out.iter().map(|s| (s * gain) as f32).collect();
    if let Some(first) = result.first_mut() {
        *first = 0.0;
    }
    if let Some(last) = result.last_mut() {
        *last = 0.0;
    }
    result
}

/// Renders one sound. `variant` picks one of [`VARIANTS`] takes. Returns
/// `None` for an unknown pack id.
pub fn render(pack_id: &str, kind: SoundKind, variant: u32) -> Option<Vec<f32>> {
    let index = PACKS.iter().position(|p| p.id == pack_id)?;
    let kind_index = SoundKind::ALL.iter().position(|k| *k == kind).unwrap_or(0) as u64;
    let seed = (index as u64 + 1) * 1_000_003
        + kind_index * 7919
        + u64::from(variant % VARIANTS) * 104_729;
    Some(synthesize(&voice(index, kind), seed))
}

/// Renders one sound as a 16-bit PCM WAV file.
pub fn render_wav(pack_id: &str, kind: SoundKind, variant: u32) -> Option<Vec<u8>> {
    render(pack_id, kind, variant).map(|s| encode_wav(&s, SAMPLE_RATE))
}

/// Encodes mono samples as a 16-bit PCM WAV byte buffer. Samples are
/// clamped to [-1, 1].
pub fn encode_wav(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + data_len as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVE");
    b.extend_from_slice(b"fmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&1u16.to_le_bytes()); // mono
    b.extend_from_slice(&sample_rate.to_le_bytes());
    b.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // byte rate
    b.extend_from_slice(&2u16.to_le_bytes()); // block align
    b.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        let v = if s.is_finite() {
            s.clamp(-1.0, 1.0)
        } else {
            0.0
        };
        let i = (v * 32767.0).round() as i16;
        b.extend_from_slice(&i.to_le_bytes());
    }
    b
}

/// Simple features used by the tests to check that sounds differ.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SoundFeatures {
    pub len_ms: f64,
    pub peak: f64,
    pub rms: f64,
    /// Energy-weighted mean time, in milliseconds.
    pub time_centroid_ms: f64,
    /// Energy-weighted mean frequency over 64 log-spaced bands, in Hz.
    pub spectral_centroid_hz: f64,
}

/// Measures a sound.
pub fn features(samples: &[f32]) -> SoundFeatures {
    let sr = f64::from(SAMPLE_RATE);
    let n = samples.len().max(1);
    let energy: f64 = samples.iter().map(|s| f64::from(*s) * f64::from(*s)).sum();
    let peak = samples
        .iter()
        .fold(0.0f64, |a, s| a.max(f64::from(s.abs())));
    let tc = if energy > 0.0 {
        samples
            .iter()
            .enumerate()
            .map(|(i, s)| i as f64 * f64::from(*s) * f64::from(*s))
            .sum::<f64>()
            / energy
            / sr
            * 1000.0
    } else {
        0.0
    };
    // Goertzel power at log-spaced frequencies from 80 Hz to 12 kHz.
    let mut num = 0.0;
    let mut den = 0.0;
    for b in 0..64 {
        let f = 80.0 * (12_000.0f64 / 80.0).powf(f64::from(b) / 63.0);
        let w = 2.0 * std::f64::consts::PI * f / sr;
        let coeff = 2.0 * w.cos();
        let (mut s1, mut s2) = (0.0f64, 0.0f64);
        for x in samples {
            let s0 = f64::from(*x) + coeff * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        let power = s1 * s1 + s2 * s2 - coeff * s1 * s2;
        num += power * f;
        den += power;
    }
    SoundFeatures {
        len_ms: n as f64 / sr * 1000.0,
        peak,
        rms: (energy / n as f64).sqrt(),
        time_centroid_ms: tc,
        spectral_centroid_hz: if den > 0.0 { num / den } else { 0.0 },
    }
}

/// A distance between two sounds' features: octaves of brightness plus
/// scaled differences in decay and length. Above about 0.25 they sound
/// clearly different.
pub fn feature_distance(a: &SoundFeatures, b: &SoundFeatures) -> f64 {
    let bright = (a.spectral_centroid_hz.max(1.0) / b.spectral_centroid_hz.max(1.0))
        .log2()
        .abs();
    let decay = (a.time_centroid_ms - b.time_centroid_ms).abs() / 5.0;
    let len = (a.len_ms - b.len_ms).abs() / 20.0;
    let level = (a.rms.max(1e-6) / b.rms.max(1e-6)).log2().abs() * 0.5;
    bright + decay + len + level
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_sounds() -> Vec<(&'static str, SoundKind, Vec<f32>)> {
        let mut v = Vec::new();
        for p in PACKS {
            for k in SoundKind::ALL {
                v.push((p.id, k, render(p.id, k, 0).unwrap()));
            }
        }
        v
    }

    #[test]
    fn at_least_ten_named_packs() {
        assert!(PACKS.len() >= 10);
        let mut ids: Vec<_> = PACKS.iter().map(|p| p.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), PACKS.len(), "ids are unique");
        for p in PACKS {
            assert!(!p.name.is_empty() && !p.description.is_empty());
            assert!(!p.name.contains('\u{2014}') && !p.description.contains('\u{2014}'));
            let mut chars = p.name.chars();
            let first = chars.next().unwrap();
            assert!(
                first.is_uppercase() && chars.all(|c| !c.is_uppercase()),
                "{} is sentence case",
                p.name
            );
        }
        assert!(pack(DEFAULT_PACK).is_some());
        assert!(pack("nope").is_none());
        assert!(render("nope", SoundKind::Down, 0).is_none());
    }

    #[test]
    fn sounds_are_short_loud_enough_and_bounded() {
        for (id, kind, s) in all_sounds() {
            let f = features(&s);
            assert!(
                s.len() < (MAX_LEN_MS * SAMPLE_RATE / 1000) as usize,
                "{id} {kind:?} too long"
            );
            assert!(f.len_ms >= 30.0, "{id} {kind:?} too short");
            assert!(f.peak <= 0.95, "{id} {kind:?} peak {}", f.peak);
            assert!(f.peak >= 0.3, "{id} {kind:?} peak {}", f.peak);
            assert!(
                f.rms > 0.01,
                "{id} {kind:?} is nearly silent (rms {})",
                f.rms
            );
            assert!(s.iter().all(|x| x.is_finite()));
        }
    }

    #[test]
    fn starts_and_ends_without_clicks() {
        for (id, kind, s) in all_sounds() {
            assert_eq!(s[0], 0.0, "{id} {kind:?}");
            assert_eq!(*s.last().unwrap(), 0.0, "{id} {kind:?}");
            let head = s[..4].iter().fold(0.0f32, |a, x| a.max(x.abs()));
            assert!(head < 0.05, "{id} {kind:?} starts abruptly ({head})");
            let tail = s[s.len() - 24..].iter().fold(0.0f32, |a, x| a.max(x.abs()));
            assert!(tail < 0.02, "{id} {kind:?} ends abruptly ({tail})");
        }
    }

    #[test]
    fn deterministic() {
        for p in PACKS {
            for k in SoundKind::ALL {
                assert_eq!(render(p.id, k, 1), render(p.id, k, 1));
            }
        }
        assert_eq!(
            render("clicky", SoundKind::Down, 2),
            render("clicky", SoundKind::Down, 2 + VARIANTS)
        );
        assert_ne!(
            render("clicky", SoundKind::Down, 0),
            render("clicky", SoundKind::Down, 1)
        );
    }

    #[test]
    fn sounds_are_distinct() {
        let sounds = all_sounds();
        let feats: Vec<_> = sounds
            .iter()
            .map(|(id, k, s)| (*id, *k, features(s)))
            .collect();
        // Every pack differs from every other pack for the same kind.
        for k in SoundKind::ALL {
            let same: Vec<_> = feats.iter().filter(|f| f.1 == k).collect();
            for i in 0..same.len() {
                for j in (i + 1)..same.len() {
                    let d = feature_distance(&same[i].2, &same[j].2);
                    assert!(
                        d > 0.25,
                        "{} and {} {:?} too similar ({d:.3})",
                        same[i].0,
                        same[j].0,
                        k
                    );
                }
            }
        }
        // Within a pack: down, up and the wide variants all differ.
        for p in PACKS {
            let mine: Vec<_> = feats.iter().filter(|f| f.0 == p.id).collect();
            for i in 0..mine.len() {
                for j in (i + 1)..mine.len() {
                    let d = feature_distance(&mine[i].2, &mine[j].2);
                    assert!(
                        d > 0.25,
                        "{} {:?} and {:?} too similar ({d:.3})",
                        p.id,
                        mine[i].1,
                        mine[j].1
                    );
                }
            }
        }
    }

    #[test]
    fn wide_keys() {
        assert_eq!(SoundKind::for_key("Space", true), SoundKind::WideDown);
        assert_eq!(SoundKind::for_key("Enter", false), SoundKind::WideUp);
        assert_eq!(SoundKind::for_key("NumEnter", true), SoundKind::WideDown);
        assert_eq!(SoundKind::for_key("Backspace", true), SoundKind::WideDown);
        assert_eq!(SoundKind::for_key("A", true), SoundKind::Down);
        assert_eq!(SoundKind::for_key("A", false), SoundKind::Up);
    }

    #[test]
    fn wav_encoding() {
        let s = render("studio", SoundKind::Down, 0).unwrap();
        let wav = encode_wav(&s, SAMPLE_RATE);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(u16::from_le_bytes([wav[20], wav[21]]), 1, "PCM");
        assert_eq!(u16::from_le_bytes([wav[22], wav[23]]), 1, "mono");
        assert_eq!(
            u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]),
            48_000
        );
        assert_eq!(u16::from_le_bytes([wav[34], wav[35]]), 16);
        assert_eq!(&wav[36..40], b"data");
        let data_len = u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]) as usize;
        assert_eq!(data_len, s.len() * 2);
        assert_eq!(wav.len(), 44 + data_len);
        let riff_len = u32::from_le_bytes([wav[4], wav[5], wav[6], wav[7]]) as usize;
        assert_eq!(riff_len, wav.len() - 8);
        let clipped = encode_wav(&[2.0, -2.0, f32::NAN], 8000);
        assert_eq!(i16::from_le_bytes([clipped[44], clipped[45]]), 32767);
        assert_eq!(i16::from_le_bytes([clipped[46], clipped[47]]), -32767);
        assert_eq!(i16::from_le_bytes([clipped[48], clipped[49]]), 0);
        assert!(render_wav("muted", SoundKind::Up, 3).is_some());
    }
}
