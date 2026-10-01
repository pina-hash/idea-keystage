//! Writes one sample JSON file per wire message type to
//! `tests/fixtures/protocol/`, so the TypeScript side tests against the same
//! samples the Rust side produces. Callout samples are real Processor output;
//! captions in them come from `data/profiles/solidworks.json`.
//!
//! A file is rewritten only when its JSON value changes, so formatting the
//! files with the repo's prettier config is stable. After a protocol change,
//! run this test, then `npx prettier --write crates/keystage-core/tests/fixtures/protocol`.

mod common;

use std::collections::BTreeMap;
use std::path::PathBuf;

use common::*;
use keystage_core::legends::{build_legends, us_scan_to_vk};
use keystage_core::protocol::{
    AppState, ClientHost, ClientMsg, ProfileRef, RecordingState, Route, ServerMsg, SessionState,
    SolidworksMode, Suppressed,
};
use keystage_core::{
    keys, CalloutKind, DisplayEvent, MonitorRect, MouseButton, OverlayConfig, PhysKey, UsLegends,
};

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/protocol")
}

/// Writes `msg` as tab-indented JSON in wire field order, unless the file
/// already holds the same JSON value.
fn write_sample<T: serde::Serialize>(name: &str, msg: &T) {
    let path = dir().join(name);
    let value = as_value(msg);
    if let Ok(existing) = std::fs::read_to_string(&path) {
        if serde_json::from_str::<serde_json::Value>(&existing).ok() == Some(value) {
            return;
        }
    }
    let mut buf = Vec::new();
    let fmt = serde_json::ser::PrettyFormatter::with_indent(b"\t");
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, fmt);
    msg.serialize(&mut ser).unwrap();
    buf.push(b'\n');
    std::fs::create_dir_all(dir()).unwrap();
    std::fs::write(&path, buf).unwrap();
}

/// The JSON value of a message as it goes on the wire (through its text, so
/// `f32` coordinates keep their short form).
fn as_value<T: serde::Serialize>(msg: &T) -> serde_json::Value {
    serde_json::from_str(&serde_json::to_string(msg).unwrap()).unwrap()
}

fn us_legends() -> keystage_core::Legends {
    let pairs = keys::CHARACTER_SCANS
        .iter()
        .filter_map(|s| us_scan_to_vk(*s).map(|v| (PhysKey::new(*s, false), v)));
    build_legends(UsLegends::LAYOUT_ID, pairs, &UsLegends)
}

fn busy_state() -> AppState {
    AppState {
        paused: false,
        suppressed: None,
        profile: Some(ProfileRef {
            id: "solidworks".into(),
            name: "SolidWorks".into(),
        }),
        solidworks_mode: SolidworksMode::Auto,
        keyboard_visible: true,
        native_visible: false,
        obs_clients: 2,
        recording: Some(RecordingState {
            active: true,
            paused: false,
            timecode: "00:12:34.567".into(),
        }),
        session: Some(SessionState {
            active: true,
            started_ms: 1000,
            markers: 2,
        }),
    }
}

fn first_of(out: &[DisplayEvent], pred: impl Fn(&DisplayEvent) -> bool) -> ServerMsg {
    out.iter().find(|e| pred(e)).expect("event").to_server_msg()
}

fn callout_msg(c: &keystage_core::Callout) -> ServerMsg {
    ServerMsg::Callout { callout: c.clone() }
}

/// Every server sample, by file name.
fn samples() -> BTreeMap<&'static str, ServerMsg> {
    let mut m: BTreeMap<&'static str, ServerMsg> = BTreeMap::new();
    m.insert(
        "hello.json",
        ServerMsg::hello(OverlayConfig::default(), busy_state(), Some(us_legends())),
    );
    m.insert(
        "hello_minimal.json",
        ServerMsg::hello(OverlayConfig::default(), AppState::default(), None),
    );
    m.insert(
        "config.json",
        ServerMsg::Config {
            config: OverlayConfig::default(),
        },
    );
    m.insert(
        "state.json",
        ServerMsg::State {
            state: busy_state(),
        },
    );
    let suppressed = AppState {
        suppressed: Some(Suppressed::Password),
        ..AppState::default()
    };
    m.insert(
        "state_suppressed.json",
        ServerMsg::State { state: suppressed },
    );
    m.insert("legends.json", ServerMsg::Legends(us_legends()));

    // Real processor output in the SolidWorks profile.
    let mut h = H::solidworks();
    h.p.set_monitor(Some(MonitorRect {
        x: 0,
        y: 0,
        width: 1000,
        height: 1000,
    }));
    let out = h.combo(&["Ctrl"], "B");
    m.insert("callout.json", callout_msg(&last(&out)));
    m.insert(
        "key.json",
        first_of(&out, |e| matches!(e, DisplayEvent::Key { down: true, .. })),
    );
    h.wait(2000);
    h.type_keys(&["2", "5", ".", "4", "Enter"]);
    m.insert(
        "callout_text.json",
        callout_msg(&h.callouts().pop().unwrap()),
    );
    h.wait(2000);
    h.tap("Alt");
    h.type_keys(&["F", "C"]);
    let chord = h.callouts().pop().unwrap();
    assert_eq!(chord.kind, CalloutKind::Chord);
    m.insert("callout_chord.json", callout_msg(&chord));
    h.wait(2000);
    h.down("Ctrl");
    let down = h.button(MouseButton::Middle, true, 520, 470);
    m.insert(
        "button.json",
        first_of(&down, |e| matches!(e, DisplayEvent::Button { .. })),
    );
    let moved = h.mv(560, 480);
    m.insert(
        "move.json",
        first_of(&moved, |e| matches!(e, DisplayEvent::Move { .. })),
    );
    m.insert("callout_mouse.json", callout_msg(&last(&moved)));
    h.button(MouseButton::Middle, false, 560, 480);
    h.up("Ctrl");
    h.wait(2000);
    h.wheel(0, 120);
    h.wheel(0, 120);
    let wheel = h.wheel(0, 120);
    m.insert(
        "wheel.json",
        first_of(&wheel, |e| matches!(e, DisplayEvent::Wheel { .. })),
    );
    m.insert("callout_wheel.json", callout_msg(&last(&wheel)));
    // Masked typing in the general profile.
    let mut g = H::general();
    g.type_keys(&["H", "I", "Enter"]);
    m.insert(
        "callout_masked.json",
        callout_msg(&g.callouts().pop().unwrap()),
    );
    g.wait(2000);
    g.type_keys(&["F5", "F5", "F5"]);
    m.insert(
        "callout_repeat.json",
        callout_msg(&g.callouts().pop().unwrap()),
    );
    let release = g.p.set_suppressed(Some(keystage_core::Suppress::Password));
    m.insert("release_all.json", release.last().unwrap().to_server_msg());
    m.insert("clear.json", ServerMsg::Clear);
    m.insert("pong.json", ServerMsg::Pong);

    m
}

fn client_samples() -> BTreeMap<&'static str, ClientMsg> {
    let mut m = BTreeMap::new();
    m.insert(
        "client_hello.json",
        ClientMsg::Hello {
            route: Route::Keys,
            host: ClientHost::Obs,
        },
    );
    m.insert("client_ping.json", ClientMsg::Ping);
    m
}

#[test]
fn write_protocol_samples() {
    let server = samples();
    let client = client_samples();
    let tags: Vec<&str> = server.values().map(ServerMsg::tag).collect();
    for t in [
        "hello",
        "config",
        "state",
        "legends",
        "callout",
        "key",
        "button",
        "wheel",
        "move",
        "release_all",
        "clear",
        "pong",
    ] {
        assert!(tags.contains(&t), "a sample for {t}");
    }
    for (name, msg) in &server {
        let back = ServerMsg::from_json(&msg.to_json()).unwrap();
        assert_eq!(&back, msg, "{name}");
        write_sample(name, msg);
    }
    for (name, msg) in &client {
        assert_eq!(&ClientMsg::from_json(&msg.to_json()).unwrap(), msg);
        write_sample(name, msg);
    }
    // The files on disk hold exactly these values and parse as messages.
    let on_disk = |name: &str| -> String { std::fs::read_to_string(dir().join(name)).unwrap() };
    for (name, msg) in &server {
        let text = on_disk(name);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&text).unwrap(),
            as_value(msg),
            "{name}"
        );
        assert_eq!(&ServerMsg::from_json(&text).unwrap(), msg, "{name}");
    }
    for (name, msg) in &client {
        assert_eq!(&ClientMsg::from_json(&on_disk(name)).unwrap(), msg);
    }
}

#[test]
fn sample_callouts_carry_sourced_captions() {
    let samples = samples();
    let cap = |name: &str| match &samples[name] {
        ServerMsg::Callout { callout } => callout.caption.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(
        cap("callout.json"),
        reference_caption(&["Ctrl", "B"]).map(|c| c.0)
    );
    assert_eq!(
        cap("callout_chord.json"),
        reference_caption(&["Alt", "F", "C"]).map(|c| c.0)
    );
    assert_eq!(
        cap("callout_mouse.json"),
        reference_mouse_caption("middle", "drag", &["Ctrl"], None).map(|c| c.0)
    );
    assert_eq!(
        cap("callout_wheel.json"),
        reference_mouse_caption("middle", "wheel", &[], Some("up")).map(|c| c.0)
    );
    assert_eq!(cap("callout_masked.json"), None);
}
