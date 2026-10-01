//! Pushes 1,000,000 synthetic input events through the Processor and prints
//! events per second and nanoseconds per event.
//!
//! Run: `cargo run --release -p keystage-core --example throughput`

use std::sync::Arc;
use std::time::Instant;

use keystage_core::{
    builtin, DisplayEvent, DisplayRules, InputEvent, KeyInput, MonitorRect, MouseButton, PhysKey,
    Processor, UsLegends,
};

const EVENTS: usize = 1_000_000;

fn key(vk: u16, scan: u16, ext: bool, down: bool, t: u64) -> InputEvent {
    InputEvent::Key(KeyInput {
        vk,
        phys: PhysKey::new(scan, ext),
        down,
        injected: false,
        t_ms: t,
    })
}

/// A repeating mix: typing, combos, special keys, mouse moves, drags and
/// wheel, at realistic spacing (about 10 ms per event).
fn workload() -> Vec<InputEvent> {
    let letters: [(u16, u16); 8] = [
        (0x41, 0x1E),
        (0x53, 0x1F),
        (0x44, 0x20),
        (0x46, 0x21),
        (0x4A, 0x24),
        (0x4B, 0x25),
        (0x4C, 0x26),
        (0x31, 0x02),
    ];
    let mut evs = Vec::with_capacity(EVENTS + 16);
    let mut t: u64 = 1_000;
    let mut i = 0usize;
    while evs.len() < EVENTS {
        match i % 6 {
            0 | 1 => {
                let (vk, sc) = letters[i % letters.len()];
                evs.push(key(vk, sc, false, true, t));
                evs.push(key(vk, sc, false, false, t + 8));
            }
            2 => {
                evs.push(key(0xA2, 0x1D, false, true, t));
                evs.push(key(0x5A, 0x2C, false, true, t + 5));
                evs.push(key(0x5A, 0x2C, false, false, t + 9));
                evs.push(key(0xA2, 0x1D, false, false, t + 12));
            }
            3 => {
                evs.push(key(0x74, 0x3F, false, true, t));
                evs.push(key(0x74, 0x3F, false, false, t + 8));
            }
            4 => {
                for k in 0..8 {
                    evs.push(InputEvent::Move {
                        x: 500 + k * 7,
                        y: 400 + k * 3,
                        t_ms: t + k as u64,
                    });
                }
                evs.push(InputEvent::Wheel {
                    dx: 0,
                    dy: 120,
                    x: 556,
                    y: 424,
                    t_ms: t + 9,
                });
            }
            _ => {
                evs.push(InputEvent::Button {
                    button: MouseButton::Middle,
                    down: true,
                    x: 600,
                    y: 400,
                    injected: false,
                    t_ms: t,
                });
                evs.push(InputEvent::Move {
                    x: 640,
                    y: 410,
                    t_ms: t + 4,
                });
                evs.push(InputEvent::Button {
                    button: MouseButton::Middle,
                    down: false,
                    x: 640,
                    y: 410,
                    injected: false,
                    t_ms: t + 8,
                });
            }
        }
        t += 40;
        i += 1;
    }
    evs.truncate(EVENTS);
    evs
}

fn measure(name: &str, mut p: Processor, evs: &[InputEvent]) {
    p.set_monitor(Some(MonitorRect {
        x: 0,
        y: 0,
        width: 1920,
        height: 1080,
    }));
    let legends = UsLegends;
    let mut out: Vec<DisplayEvent> = Vec::with_capacity(16);
    let mut emitted = 0usize;
    let start = Instant::now();
    for (n, ev) in evs.iter().enumerate() {
        out.clear();
        p.handle_into(ev, &legends, &mut out);
        emitted += out.len();
        if n % 64 == 0 {
            emitted += p.tick(ev.t_ms()).len();
        }
    }
    let elapsed = start.elapsed();
    let secs = elapsed.as_secs_f64();
    println!(
        "{name}: {} events in {:.3} s = {:.0} events/s, {:.0} ns/event ({} display events)",
        evs.len(),
        secs,
        evs.len() as f64 / secs,
        elapsed.as_nanos() as f64 / evs.len() as f64,
        emitted
    );
}

fn main() {
    let evs = workload();
    measure(
        "general profile",
        Processor::new(DisplayRules::default()),
        &evs,
    );
    let mut sw = Processor::new(DisplayRules::default());
    match builtin::solidworks() {
        Ok(profile) => {
            sw.set_profile(Arc::new(profile));
            measure("SolidWorks profile", sw, &evs);
        }
        Err(e) => eprintln!("the SolidWorks profile could not be read: {e}"),
    }
}
