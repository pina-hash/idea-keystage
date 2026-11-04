//! Tutorial capture: every mouse button press and release, double click,
//! drag start and end, wheel step, and the cursor path sampled at up to 30
//! per second, in physical screen pixels (virtual-screen coordinates).
//!
//! This is independent of what the overlay shows: a plain left click makes
//! no callout by default, but it is an entry here. It is still bound by
//! every privacy rule, because the [`crate::Processor`] feeds this log only
//! while nothing is suppressed (password field, excluded app, locked
//! session, unidentified foreground, paused) and forgets held buttons the
//! moment something is. The app turns it on only while a Session Log the
//! user started with "Tutorial capture" runs (docs/SESSION_FORMAT.md,
//! `pointer`), and the entries go nowhere but that Session Log: they are not
//! part of the wire protocol.

use serde::{Deserialize, Serialize};

use crate::event::{Mod, MouseButton};
use crate::modifiers::ModSet;
use crate::rules::DisplayRules;

/// Moves closer together than this are not logged (at most 30 per second).
pub const MOVE_SAMPLE_MS: u64 = 34;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PointerKind {
    Down,
    Up,
    /// The second press of a double click (right after its `down`).
    DoubleClick,
    /// A held button moved past the drag threshold; `x`, `y` are where it
    /// was pressed.
    DragStart,
    /// The release that ended a drag (right before its `up`).
    DragEnd,
    /// One wheel event; `dx`, `dy` in raw `WHEEL_DELTA` units (+120 is one
    /// notch up or right).
    Wheel,
    /// A cursor sample.
    Move,
}

fn is_zero(v: &i32) -> bool {
    *v == 0
}

/// One `pointer` entry of a session file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PointerEntry {
    /// Monotonic milliseconds, the clock of every `t_ms` in a session.
    pub t_ms: u64,
    pub kind: PointerKind,
    /// Button entries only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub button: Option<MouseButton>,
    /// Physical screen pixels, virtual-screen coordinates (the primary
    /// monitor's top left is 0, 0; monitors left of or above it are
    /// negative).
    pub x: i32,
    pub y: i32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub dx: i32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub dy: i32,
    /// Modifiers held at a press, release or wheel step.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mods: Vec<Mod>,
}

#[derive(Clone, Copy, Debug)]
struct Hold {
    x0: i32,
    y0: i32,
    t0: u64,
    dragging: bool,
}

#[derive(Clone, Copy, Debug)]
struct Click {
    button: MouseButton,
    x: i32,
    y: i32,
    t_down: u64,
}

/// The tutorial capture log, fed by the processor.
#[derive(Clone, Debug)]
pub struct PointerLog {
    holds: [Option<Hold>; 5],
    last_click: Option<Click>,
    last_move: Option<(u64, i32, i32)>,
    drag_px: f64,
    double_ms: u64,
    double_px: f64,
    out: Vec<PointerEntry>,
}

fn mods_of(m: ModSet) -> Vec<Mod> {
    m.iter().map(|k| k.wire()).collect()
}

fn dist(ax: i32, ay: i32, bx: i32, by: i32) -> f64 {
    let dx = f64::from(bx) - f64::from(ax);
    let dy = f64::from(by) - f64::from(ay);
    (dx * dx + dy * dy).sqrt()
}

impl PointerLog {
    /// Uses the drag and double click thresholds of `rules`, so the log and
    /// the overlay agree on what a drag or a double click is.
    pub fn new(rules: &DisplayRules) -> PointerLog {
        PointerLog {
            holds: [None; 5],
            last_click: None,
            last_move: None,
            drag_px: f64::from(rules.drag_threshold_px),
            double_ms: rules.double_click_ms,
            double_px: f64::from(rules.double_click_px),
            out: Vec::new(),
        }
    }

    fn entry(
        &mut self,
        t_ms: u64,
        kind: PointerKind,
        button: Option<MouseButton>,
        x: i32,
        y: i32,
        mods: &[Mod],
    ) {
        self.out.push(PointerEntry {
            t_ms,
            kind,
            button,
            x,
            y,
            dx: 0,
            dy: 0,
            mods: mods.to_vec(),
        });
    }

    /// Forgets held buttons and the last click (suppression began).
    pub fn reset(&mut self) {
        self.holds = [None; 5];
        self.last_click = None;
        self.last_move = None;
    }

    pub fn button(
        &mut self,
        button: MouseButton,
        down: bool,
        x: i32,
        y: i32,
        t_ms: u64,
        mods: ModSet,
    ) {
        let i = button.index();
        let m = mods_of(mods);
        if down {
            self.entry(t_ms, PointerKind::Down, Some(button), x, y, &m);
            let double = self.last_click.take().filter(|c| {
                c.button == button
                    && t_ms.saturating_sub(c.t_down) <= self.double_ms
                    && dist(c.x, c.y, x, y) <= self.double_px
            });
            if double.is_some() {
                self.entry(t_ms, PointerKind::DoubleClick, Some(button), x, y, &m);
            }
            self.holds[i] = Some(Hold {
                x0: x,
                y0: y,
                t0: t_ms,
                dragging: false,
            });
            // A double click ends a pair; the next press starts a new one.
            if double.is_some() {
                self.holds[i] = Some(Hold {
                    x0: x,
                    y0: y,
                    t0: u64::MAX,
                    dragging: false,
                });
            }
            return;
        }
        let hold = self.holds[i].take();
        if let Some(h) = hold {
            let dragged = h.dragging || dist(h.x0, h.y0, x, y) > self.drag_px;
            if dragged {
                if !h.dragging {
                    self.entry(t_ms, PointerKind::DragStart, Some(button), h.x0, h.y0, &m);
                }
                self.entry(t_ms, PointerKind::DragEnd, Some(button), x, y, &m);
                self.last_click = None;
            } else if h.t0 != u64::MAX {
                self.last_click = Some(Click {
                    button,
                    x: h.x0,
                    y: h.y0,
                    t_down: h.t0,
                });
            }
        }
        self.entry(t_ms, PointerKind::Up, Some(button), x, y, &m);
    }

    pub fn wheel(&mut self, dx: i32, dy: i32, x: i32, y: i32, t_ms: u64, mods: ModSet) {
        self.out.push(PointerEntry {
            t_ms,
            kind: PointerKind::Wheel,
            button: None,
            x,
            y,
            dx,
            dy,
            mods: mods_of(mods),
        });
    }

    pub fn moved(&mut self, x: i32, y: i32, t_ms: u64) {
        let sample = match self.last_move {
            None => true,
            Some((t, lx, ly)) => t_ms.saturating_sub(t) >= MOVE_SAMPLE_MS && (lx, ly) != (x, y),
        };
        if sample {
            self.last_move = Some((t_ms, x, y));
            self.entry(t_ms, PointerKind::Move, None, x, y, &[]);
        }
        for button in MouseButton::ALL {
            let i = button.index();
            let Some(h) = self.holds[i] else { continue };
            if !h.dragging && dist(h.x0, h.y0, x, y) > self.drag_px {
                self.entry(t_ms, PointerKind::DragStart, Some(button), h.x0, h.y0, &[]);
                if let Some(h) = self.holds[i].as_mut() {
                    h.dragging = true;
                }
                self.last_click = None;
            }
        }
    }

    /// The entries logged since the last call.
    pub fn take(&mut self) -> Vec<PointerEntry> {
        std::mem::take(&mut self.out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modifiers::ModKey;
    use PointerKind::*;

    fn log() -> PointerLog {
        PointerLog::new(&DisplayRules::default())
    }

    fn kinds(l: &mut PointerLog) -> Vec<(PointerKind, i32, i32)> {
        l.take().into_iter().map(|e| (e.kind, e.x, e.y)).collect()
    }

    #[test]
    fn a_plain_click_is_a_press_and_a_release_at_its_position() {
        let mut l = log();
        l.button(MouseButton::Left, true, -1200, 340, 100, ModSet::default());
        l.button(MouseButton::Left, false, -1199, 341, 180, ModSet::default());
        let e = l.take();
        assert_eq!(e.len(), 2);
        assert_eq!(
            (e[0].kind, e[0].button, e[0].x, e[0].y, e[0].t_ms),
            (Down, Some(MouseButton::Left), -1200, 340, 100)
        );
        assert_eq!(
            (e[1].kind, e[1].x, e[1].y, e[1].t_ms),
            (Up, -1199, 341, 180)
        );
        let json = serde_json::to_string(&e[0]).expect("json");
        assert_eq!(
            json,
            r#"{"t_ms":100,"kind":"down","button":"left","x":-1200,"y":340}"#
        );
    }

    #[test]
    fn double_clicks_and_drags_use_the_overlay_thresholds() {
        let mut l = log();
        let none = ModSet::default();
        l.button(MouseButton::Left, true, 10, 10, 0, none);
        l.button(MouseButton::Left, false, 10, 10, 50, none);
        l.button(MouseButton::Left, true, 12, 11, 300, none);
        l.button(MouseButton::Left, false, 12, 11, 350, none);
        // A third press right after is a new first click, not a second double.
        l.button(MouseButton::Left, true, 12, 11, 400, none);
        l.button(MouseButton::Left, false, 12, 11, 450, none);
        assert_eq!(
            kinds(&mut l).iter().map(|k| k.0).collect::<Vec<_>>(),
            vec![Down, Up, Down, DoubleClick, Up, Down, Up]
        );
        // Too slow or too far is two clicks.
        l.button(MouseButton::Right, true, 0, 0, 1000, none);
        l.button(MouseButton::Right, false, 0, 0, 1010, none);
        l.button(MouseButton::Right, true, 0, 0, 1600, none);
        l.button(MouseButton::Right, false, 0, 0, 1610, none);
        assert!(!kinds(&mut l).iter().any(|k| k.0 == DoubleClick));

        // A middle drag: start at the press point, end at the release.
        l.button(MouseButton::Middle, true, 100, 100, 2000, none);
        l.moved(102, 101, 2010);
        l.moved(140, 120, 2050);
        l.moved(160, 130, 2060);
        l.button(MouseButton::Middle, false, 170, 135, 2100, none);
        assert_eq!(
            kinds(&mut l),
            vec![
                (Down, 100, 100),
                (Move, 102, 101),
                (Move, 140, 120),
                (DragStart, 100, 100),
                (DragEnd, 170, 135),
                (Up, 170, 135)
            ]
        );
        // A fast drag with no move in between is still a drag.
        l.button(MouseButton::Left, true, 0, 0, 3000, none);
        l.button(MouseButton::Left, false, 50, 0, 3020, none);
        assert_eq!(
            kinds(&mut l).iter().map(|k| k.0).collect::<Vec<_>>(),
            vec![Down, DragStart, DragEnd, Up]
        );
    }

    #[test]
    fn moves_are_sampled_at_most_thirty_times_a_second() {
        let mut l = log();
        for i in 0..100u64 {
            // 125 per second for 800 ms.
            l.moved(i as i32, 0, i * 8);
        }
        let e = l.take();
        assert!(e.iter().all(|e| e.kind == Move));
        assert!(e.len() <= 25, "{} samples in 800 ms", e.len());
        for w in e.windows(2) {
            assert!(w[1].t_ms - w[0].t_ms >= MOVE_SAMPLE_MS);
        }
        // A cursor that does not move is not logged again.
        l.moved(500, 0, 10_000);
        l.moved(500, 0, 10_100);
        assert_eq!(l.take().len(), 1);
    }

    #[test]
    fn wheel_steps_and_modifiers() {
        let mut l = log();
        let ctrl = ModSet::default().with(ModKey::Ctrl);
        l.wheel(0, -120, 5, 6, 10, ctrl);
        l.button(MouseButton::Left, true, 5, 6, 20, ctrl);
        let e = l.take();
        assert_eq!((e[0].kind, e[0].dy, e[0].dx), (Wheel, -120, 0));
        assert_eq!(e[0].mods, vec![Mod::Ctrl]);
        assert_eq!(
            serde_json::to_string(&e[0]).expect("j"),
            r#"{"t_ms":10,"kind":"wheel","x":5,"y":6,"dy":-120,"mods":["ctrl"]}"#
        );
        assert_eq!(e[1].mods, vec![Mod::Ctrl]);
    }

    #[test]
    fn reset_forgets_held_buttons() {
        let mut l = log();
        l.button(MouseButton::Left, true, 0, 0, 0, ModSet::default());
        l.button(MouseButton::Left, false, 0, 0, 10, ModSet::default());
        l.button(MouseButton::Left, true, 0, 0, 100, ModSet::default());
        l.take();
        l.reset();
        // The release after suppression ended: no drag, no double click.
        l.button(MouseButton::Left, false, 80, 0, 200, ModSet::default());
        l.button(MouseButton::Left, true, 80, 0, 250, ModSet::default());
        assert_eq!(
            kinds(&mut l).iter().map(|k| k.0).collect::<Vec<_>>(),
            vec![Up, Down]
        );
    }
}
