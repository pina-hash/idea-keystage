//! Human-like pointer motion (docs/ARCHITECTURE.md section 14).
//!
//! * [`move_path`]: from the current point to a target along a slightly
//!   curved quadratic Bezier (control point 5 to 12 percent of the distance
//!   off the straight line, on a random side), with the minimum-jerk speed
//!   profile `s(t) = 10t^3 - 15t^4 + 6t^5` (Flash and Hogan, 1985), lasting
//!   [`fitts_ms`] unless a duration is given, and, on moves longer than
//!   [`OVERSHOOT_MIN_PX`], a small overshoot that is corrected.
//! * [`drag_path`]: through a list of points on a Catmull-Rom spline, the
//!   same speed profile over its whole arc length.
//! * Samples are [`STEP_MS`] apart (125 per second); the last sample is the
//!   target exactly.
//! * [`to_absolute`] maps physical virtual-screen pixels to the 0 to 65535
//!   range `SendInput` takes with `MOUSEEVENTF_ABSOLUTE |
//!   MOUSEEVENTF_VIRTUALDESK`.
//!
//! Everything is deterministic for a seed ([`Rng`]).

use serde::{Deserialize, Serialize};

/// Between samples (125 per second).
pub const STEP_MS: u64 = 8;
/// Moves longer than this overshoot a little and correct.
pub const OVERSHOOT_MIN_PX: f64 = 600.0;
/// Longest single move or drag.
pub const MAX_DURATION_MS: u64 = 60_000;

/// SplitMix64: small, fast, deterministic.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)`.
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Uniform in `[lo, hi]`.
    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }

    /// Uniform integer in `[lo, hi]`.
    pub fn range_u64(&mut self, lo: u64, hi: u64) -> u64 {
        if hi <= lo {
            return lo;
        }
        lo + self.next_u64() % (hi - lo + 1)
    }

    pub fn coin(&mut self) -> bool {
        self.next_u64() & 1 == 1
    }
}

/// A point in physical virtual-screen pixels.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Point {
        Point { x, y }
    }

    fn dist(self, o: Point) -> f64 {
        ((o.x - self.x).powi(2) + (o.y - self.y).powi(2)).sqrt()
    }

    fn lerp(self, o: Point, s: f64) -> Point {
        Point::new(self.x + (o.x - self.x) * s, self.y + (o.y - self.y) * s)
    }
}

/// One pointer sample: milliseconds after the start, and the pixel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sample {
    pub t_ms: u64,
    pub x: i32,
    pub y: i32,
}

/// A rectangle in physical virtual-screen pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub fn right(&self) -> i64 {
        i64::from(self.x) + i64::from(self.width)
    }

    pub fn bottom(&self) -> i64 {
        i64::from(self.y) + i64::from(self.height)
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        let (x, y) = (i64::from(x), i64::from(y));
        x >= i64::from(self.x) && y >= i64::from(self.y) && x < self.right() && y < self.bottom()
    }

    /// True when the two overlap by at least one pixel.
    pub fn intersects(&self, o: &Rect) -> bool {
        i64::from(self.x) < o.right()
            && i64::from(o.x) < self.right()
            && i64::from(self.y) < o.bottom()
            && i64::from(o.y) < self.bottom()
    }
}

/// Fitts's law movement time: `230 ms + 166 ms * log2(D / 20 + 1)`.
pub fn fitts_ms(distance_px: f64) -> u64 {
    let d = distance_px.max(0.0);
    (230.0 + 166.0 * (d / 20.0 + 1.0).log2()).round() as u64
}

/// The minimum-jerk position profile on `t` in `[0, 1]`.
pub fn min_jerk(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    let t3 = t * t * t;
    t3 * (10.0 - 15.0 * t + 6.0 * t * t)
}

fn round(p: Point) -> (i32, i32) {
    (p.x.round() as i32, p.y.round() as i32)
}

/// Samples `f(s)` with `s = min_jerk(t / duration)` from `t = 0` to the
/// duration, every [`STEP_MS`], the last one exactly at the end.
fn sample_profile(
    duration_ms: u64,
    t0: u64,
    mut f: impl FnMut(f64) -> Point,
    out: &mut Vec<Sample>,
) {
    let d = duration_ms.max(STEP_MS);
    let mut t = 0;
    loop {
        let s = min_jerk(t as f64 / d as f64);
        let (x, y) = round(f(s));
        out.push(Sample { t_ms: t0 + t, x, y });
        if t >= d {
            break;
        }
        t = (t + STEP_MS).min(d);
    }
}

fn bezier(p0: Point, c: Point, p1: Point, s: f64) -> Point {
    let a = (1.0 - s) * (1.0 - s);
    let b = 2.0 * (1.0 - s) * s;
    let e = s * s;
    Point::new(a * p0.x + b * c.x + e * p1.x, a * p0.y + b * c.y + e * p1.y)
}

/// The curve's control point: off the midpoint, perpendicular to the line.
fn control(from: Point, to: Point, rng: &mut Rng) -> Point {
    let d = from.dist(to);
    let mid = from.lerp(to, 0.5);
    if d < 1.0 {
        return mid;
    }
    let (ux, uy) = ((to.x - from.x) / d, (to.y - from.y) / d);
    let side = if rng.coin() { 1.0 } else { -1.0 };
    let off = d * rng.range(0.05, 0.12) * side;
    Point::new(mid.x - uy * off, mid.y + ux * off)
}

/// A natural move from `from` to `to`. `duration_ms` defaults to
/// [`fitts_ms`]; it is capped at [`MAX_DURATION_MS`].
pub fn move_path(from: Point, to: Point, duration_ms: Option<u64>, seed: u64) -> Vec<Sample> {
    let mut rng = Rng::new(seed);
    let d = from.dist(to);
    let total = duration_ms
        .unwrap_or_else(|| fitts_ms(d))
        .min(MAX_DURATION_MS);
    let mut out = Vec::new();
    if d < 0.5 {
        let (x, y) = round(to);
        out.push(Sample { t_ms: 0, x, y });
        return out;
    }
    if d > OVERSHOOT_MIN_PX {
        // Past the target by 2 to 4 percent, then back.
        let k = rng.range(0.02, 0.04);
        let over = Point::new(to.x + (to.x - from.x) * k, to.y + (to.y - from.y) * k);
        let main = (total as f64 * 0.85).round() as u64;
        let c = control(from, over, &mut rng);
        sample_profile(main, 0, |s| bezier(from, c, over, s), &mut out);
        let start = out.last().map(|s| s.t_ms).unwrap_or(0);
        let mut tail = Vec::new();
        sample_profile(
            total.saturating_sub(main).max(STEP_MS * 4),
            start,
            |s| over.lerp(to, s),
            &mut tail,
        );
        out.extend(tail.into_iter().skip(1));
    } else {
        let c = control(from, to, &mut rng);
        sample_profile(total, 0, |s| bezier(from, c, to, s), &mut out);
    }
    if let Some(last) = out.last_mut() {
        let (x, y) = round(to);
        last.x = x;
        last.y = y;
    }
    out
}

fn catmull_rom(p0: Point, p1: Point, p2: Point, p3: Point, t: f64) -> Point {
    let t2 = t * t;
    let t3 = t2 * t;
    let f = |a: f64, b: f64, c: f64, d: f64| {
        0.5 * (2.0 * b
            + (-a + c) * t
            + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2
            + (-a + 3.0 * b - 3.0 * c + d) * t3)
    };
    Point::new(f(p0.x, p1.x, p2.x, p3.x), f(p0.y, p1.y, p2.y, p3.y))
}

/// A dense polyline through `points` (Catmull-Rom, end points repeated).
fn spline(points: &[Point]) -> Vec<Point> {
    if points.len() < 2 {
        return points.to_vec();
    }
    let n = points.len();
    let at = |i: isize| points[i.clamp(0, n as isize - 1) as usize];
    let mut out = vec![points[0]];
    for i in 0..n - 1 {
        let (p0, p1, p2, p3) = (
            at(i as isize - 1),
            at(i as isize),
            at(i as isize + 1),
            at(i as isize + 2),
        );
        let steps = ((p1.dist(p2) / 4.0).ceil() as usize).clamp(4, 400);
        for k in 1..=steps {
            out.push(catmull_rom(p0, p1, p2, p3, k as f64 / steps as f64));
        }
    }
    out
}

/// The default drag duration: Fitts's time for the whole length, plus
/// 120 ms for each bend.
pub fn drag_ms(points: &[Point]) -> u64 {
    let len: f64 = points.windows(2).map(|w| w[0].dist(w[1])).sum();
    (fitts_ms(len) + 120 * points.len().saturating_sub(2) as u64).max(300)
}

/// A drag through `points` (the first is where the button goes down).
pub fn drag_path(points: &[Point], duration_ms: Option<u64>) -> Vec<Sample> {
    let mut out = Vec::new();
    let Some(first) = points.first().copied() else {
        return out;
    };
    let last = *points.last().unwrap_or(&first);
    let dense = spline(points);
    let mut cum = vec![0.0];
    for w in dense.windows(2) {
        let prev = *cum.last().unwrap_or(&0.0);
        cum.push(prev + w[0].dist(w[1]));
    }
    let len = *cum.last().unwrap_or(&0.0);
    let total = duration_ms
        .unwrap_or_else(|| drag_ms(points))
        .min(MAX_DURATION_MS);
    if len < 0.5 {
        let (x, y) = round(first);
        out.push(Sample { t_ms: 0, x, y });
        return out;
    }
    sample_profile(
        total,
        0,
        |s| {
            let target = s * len;
            let i = cum
                .partition_point(|&c| c < target)
                .clamp(1, dense.len() - 1);
            let (a, b) = (cum[i - 1], cum[i]);
            let f = if b > a { (target - a) / (b - a) } else { 0.0 };
            dense[i - 1].lerp(dense[i], f)
        },
        &mut out,
    );
    if let Some(l) = out.last_mut() {
        let (x, y) = round(last);
        l.x = x;
        l.y = y;
    }
    if let Some(f) = out.first_mut() {
        let (x, y) = round(first);
        f.x = x;
        f.y = y;
    }
    out
}

/// Physical virtual-screen pixels to `SendInput` absolute units over the
/// virtual desktop: the left and top edges are 0, the right and bottom
/// edges 65535. Outside points are clamped.
pub fn to_absolute(x: i32, y: i32, screen: &Rect) -> (i32, i32) {
    let map = |v: i32, origin: i32, size: u32| -> i32 {
        let span = i64::from(size.max(2)) - 1;
        let off = (i64::from(v) - i64::from(origin)).clamp(0, span);
        ((off * 65535 + span / 2) / span) as i32
    };
    (
        map(x, screen.x, screen.width),
        map(y, screen.y, screen.height),
    )
}

/// A point given as fractions of a monitor (0 is the left or top edge, 1
/// the right or bottom pixel), in physical pixels.
pub fn on_monitor(nx: f64, ny: f64, monitor: &Rect) -> (i32, i32) {
    let w = f64::from(monitor.width.max(1) - 1);
    let h = f64::from(monitor.height.max(1) - 1);
    (
        monitor.x + (nx.clamp(0.0, 1.0) * w).round() as i32,
        monitor.y + (ny.clamp(0.0, 1.0) * h).round() as i32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn fitts_and_min_jerk_follow_their_formulas() {
        assert_eq!(fitts_ms(0.0), 230);
        // 230 + 166 * log2(500/20 + 1) = 230 + 166 * 4.7004 = 1010.3
        assert_eq!(fitts_ms(500.0), 1010);
        assert_eq!(fitts_ms(20.0), 396);
        assert_eq!(min_jerk(0.0), 0.0);
        assert_eq!(min_jerk(1.0), 1.0);
        assert!((min_jerk(0.5) - 0.5).abs() < 1e-12);
        let mut last = 0.0;
        for i in 0..=100 {
            let v = min_jerk(i as f64 / 100.0);
            assert!(v >= last);
            last = v;
        }
    }

    #[test]
    fn a_move_is_deterministic_ends_exactly_and_progresses() {
        let a = move_path(p(100.0, 100.0), p(500.0, 300.0), None, 7);
        let b = move_path(p(100.0, 100.0), p(500.0, 300.0), None, 7);
        assert_eq!(a, b, "same seed, same path");
        let c = move_path(p(100.0, 100.0), p(500.0, 300.0), None, 8);
        assert_ne!(a, c, "another seed, another curve");
        let last = a.last().expect("samples");
        assert_eq!((last.x, last.y), (500, 300));
        assert_eq!((a[0].x, a[0].y), (100, 100));
        // Duration from Fitts's law: distance 447 px.
        assert_eq!(last.t_ms, fitts_ms(447.213_595_5));
        for w in a.windows(2) {
            assert!(w[1].t_ms > w[0].t_ms);
            assert!(w[1].t_ms - w[0].t_ms <= STEP_MS);
        }
        // Progress along the line from start to target never goes back.
        let (dx, dy) = (400.0, 200.0);
        let proj = |s: &Sample| {
            ((f64::from(s.x) - 100.0) * dx + (f64::from(s.y) - 100.0) * dy) / (dx * dx + dy * dy)
        };
        for w in a.windows(2) {
            assert!(proj(&w[1]) >= proj(&w[0]) - 0.01, "{:?}", w);
        }
        // Curved, but only a little: off the line by 1 to 7 percent at most.
        let d = (dx * dx + dy * dy).sqrt();
        let off = a
            .iter()
            .map(|s| ((f64::from(s.x) - 100.0) * dy - (f64::from(s.y) - 100.0) * dx).abs() / d)
            .fold(0.0, f64::max);
        assert!(off > d * 0.01 && off < d * 0.07, "{off}");
    }

    #[test]
    fn long_moves_overshoot_and_correct_and_short_ones_do_not() {
        let a = move_path(p(0.0, 500.0), p(1500.0, 500.0), Some(1200), 3);
        let max_x = a.iter().map(|s| s.x).max().unwrap_or(0);
        assert!(max_x > 1500 + 20 && max_x < 1500 + 70, "{max_x}");
        let last = a.last().expect("s");
        assert_eq!((last.x, last.y, last.t_ms), (1500, 500, 1200));
        let b = move_path(p(0.0, 500.0), p(500.0, 500.0), Some(600), 3);
        assert!(b.iter().all(|s| s.x <= 500));
        assert_eq!(b.last().map(|s| s.t_ms), Some(600));
        // No move at all is one sample.
        assert_eq!(move_path(p(5.0, 5.0), p(5.0, 5.0), None, 1).len(), 1);
    }

    #[test]
    fn drags_pass_through_their_points() {
        let pts = [p(100.0, 100.0), p(300.0, 120.0), p(320.0, 400.0)];
        let s = drag_path(&pts, Some(900));
        assert_eq!((s[0].x, s[0].y), (100, 100));
        let last = s.last().expect("s");
        assert_eq!((last.x, last.y, last.t_ms), (320, 400, 900));
        // The middle point is visited (within a few pixels).
        let near = s
            .iter()
            .map(|q| ((f64::from(q.x) - 300.0).powi(2) + (f64::from(q.y) - 120.0).powi(2)).sqrt())
            .fold(f64::MAX, f64::min);
        assert!(near < 6.0, "{near}");
        assert_eq!(drag_path(&[], None), vec![]);
        assert!(drag_ms(&pts) >= fitts_ms(200.0));
    }

    #[test]
    fn absolute_coordinates_cover_the_virtual_desktop() {
        // A 2560 wide monitor left of a 1920 primary: x from -2560 to 1919.
        let vs = Rect {
            x: -2560,
            y: -200,
            width: 4480,
            height: 1440,
        };
        assert_eq!(to_absolute(-2560, -200, &vs), (0, 0));
        assert_eq!(to_absolute(1919, 1239, &vs), (65535, 65535));
        assert_eq!(to_absolute(-9999, 9999, &vs), (0, 65535), "clamped");
        let (mx, _) = to_absolute(0, 0, &vs);
        assert!((mx - 37456).abs() <= 1, "{mx}");
        let mon = Rect {
            x: -2560,
            y: -200,
            width: 2560,
            height: 1440,
        };
        assert_eq!(on_monitor(0.0, 0.0, &mon), (-2560, -200));
        assert_eq!(on_monitor(1.0, 1.0, &mon), (-1, 1239));
        assert_eq!(on_monitor(0.5, 0.5, &mon), (-1280, 520));
    }

    #[test]
    fn rects() {
        let a = Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 100,
        };
        assert!(a.contains(0, 0) && a.contains(99, 99) && !a.contains(100, 0));
        assert!(a.intersects(&Rect {
            x: 99,
            y: 99,
            width: 5,
            height: 5
        }));
        assert!(!a.intersects(&Rect {
            x: 100,
            y: 0,
            width: 5,
            height: 5
        }));
    }
}
