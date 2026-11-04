//! The connector's tools: names, descriptions and JSON schemas. The app
//! serves this list (`GET /api/control/tools`) and runs the calls; the MCP
//! server (`idea-keystage-mcp`) only relays, so a new tool arrives with an
//! app update and the connector never has to be installed again.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> ToolDef {
    ToolDef {
        name: name.into(),
        description: description.into(),
        input_schema: json!({
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false
        }),
    }
}

fn target() -> Value {
    json!({
        "x": {"type": "number", "description": "Physical pixels of the virtual screen, or with monitor a fraction from 0 to 1."},
        "y": {"type": "number", "description": "Physical pixels of the virtual screen, or with monitor a fraction from 0 to 1."},
        "monitor": {"type": "string", "description": "A monitor device name from status (\\\\.\\DISPLAY1). With it, x and y are fractions of that monitor."}
    })
}

fn button() -> Value {
    json!({"type": "string", "enum": ["left", "right", "middle"], "default": "left"})
}

fn rect() -> Value {
    json!({
        "type": "object",
        "properties": {
            "x": {"type": "integer"}, "y": {"type": "integer"},
            "width": {"type": "integer", "minimum": 1}, "height": {"type": "integer", "minimum": 1}
        },
        "required": ["x", "y", "width", "height"],
        "additionalProperties": false
    })
}

fn merge(a: Value, b: Value) -> Value {
    let mut a = a;
    if let (Some(m), Some(n)) = (a.as_object_mut(), b.as_object()) {
        for (k, v) in n {
            m.insert(k.clone(), v.clone());
        }
    }
    a
}

/// Every tool the app answers.
pub fn catalog() -> Vec<ToolDef> {
    let none = json!({});
    vec![
        tool("status", "IDEA Keystage's state: version, paused or active, input hooks, profile, monitors (device name, rectangle in physical pixels, DPI scale), OBS link and recording, the Session Log, updates, and whether automation is allowed.", none.clone(), &[]),
        tool("session_start", "Starts a Session Log (the record of shortcuts, clicks and chapter markers a tutorial builder reads). capture: true also logs every click, drag, wheel step and the cursor path (tutorial capture) for this session.", json!({
            "name": {"type": "string", "description": "A first chapter marker name, optional."},
            "capture": {"type": "boolean", "description": "Tutorial capture for this session. Default: the app setting."}
        }), &[]),
        tool("session_stop", "Stops the running Session Log, saves it and returns the whole session JSON (keystage.session/1.x).", none.clone(), &[]),
        tool("session_marker", "Adds a chapter marker to the running Session Log (and to the OBS recording when it can take chapters). step is an optional tutorial step id kept with the marker.", json!({
            "name": {"type": "string", "maxLength": 100},
            "step": {"type": "string", "maxLength": 64}
        }), &["name"]),
        tool("session_list", "The saved and in-memory Session Logs, newest first.", json!({
            "limit": {"type": "integer", "minimum": 1, "maximum": 200, "default": 20}
        }), &[]),
        tool("session_get", "One Session Log's JSON by id.", json!({"id": {"type": "string"}}), &["id"]),
        tool("obs_status", "The OBS link: whether Connect to OBS is on, the connection state, recording state and timecode, and the recording folder. If the link is off it says exactly what to turn on.", none.clone(), &[]),
        tool("obs_start_recording", "Starts recording in OBS through the OBS link (never with hotkeys).", none.clone(), &[]),
        tool("obs_stop_recording", "Stops recording in OBS through the OBS link; returns the output file path and the recording length.", none.clone(), &[]),
        tool("overlay_set", "Shows or hides IDEA Keystage's on-screen overlay (Host A), so a recording can be made with or without it.", json!({"visible": {"type": "boolean"}}), &["visible"]),
        tool("show_set", "Which pieces the overlay draws: all, only keys (callouts), only the keyboard, only the mouse diagram, only cursor effects, or auto (back to the user's settings).", json!({
            "mode": {"type": "string", "enum": ["auto", "all", "keys", "keyboard", "mouse", "cursor"]}
        }), &["mode"]),
        tool("profile_set", "Which profile captions come from: solidworks (on), general (SolidWorks off), auto (follow the app in front), or a user profile id.", json!({"id": {"type": "string"}}), &["id"]),
        tool("pause", "Pauses IDEA Keystage (removes the input hooks; nothing is shown or logged).", none.clone(), &[]),
        tool("resume", "Resumes IDEA Keystage.", none.clone(), &[]),
        tool("recording_info", "Size and SHA-256 of a file in the OBS recording folder or the Session Log folder.", json!({"path": {"type": "string"}}), &["path"]),
        tool("recording_read_chunk", "Reads up to 4 MiB of a file in the OBS recording folder or the Session Log folder, as base64. Read a whole recording chunk by chunk and check it against recording_info.", json!({
            "path": {"type": "string"},
            "offset": {"type": "integer", "minimum": 0},
            "length": {"type": "integer", "minimum": 1, "maximum": 4194304}
        }), &["path", "offset", "length"]),
        tool("check_for_updates_now", "Checks GitHub for a new IDEA Keystage version now; it installs itself when nothing records.", none.clone(), &[]),
        tool("foreground_window", "The app in front: process name, window rectangle in physical pixels, and the monitor it is on. Never the window title.", none.clone(), &[]),
        tool("occluders", "Windows in front of the app being recorded that cover the capture monitor: process name, z-order, rectangle, topmost. Never titles. A clean take needs this to be empty.", json!({
            "monitor": {"type": "string", "description": "Capture monitor device name; default the overlay's monitor or the primary."}
        }), &[]),
        tool("pointer_move", "Moves the real cursor like a person: a slightly curved path, minimum-jerk speed, Fitts's law timing unless duration_ms is given, a small overshoot on long moves. Needs \"Allow Claude to move the mouse and type\".", merge(target(), json!({
            "duration_ms": {"type": "integer", "minimum": 0, "maximum": 60000}
        })), &["x", "y"]),
        tool("pointer_click", "Clicks like a person: moves there first when x and y are given, settles 120 ms, holds 60 to 110 ms. wait_idle first waits for the screen there to stop changing.", merge(target(), json!({
            "button": button(),
            "double": {"type": "boolean", "default": false},
            "wait_idle": {"type": "object", "properties": {"rect": rect(), "max_ms": {"type": "integer", "minimum": 0, "maximum": 30000}}, "additionalProperties": false}
        })), &[]),
        tool("pointer_drag", "Presses a button, follows the path smoothly (Catmull-Rom through the points, minimum-jerk overall) and releases. The cursor first moves to the first point.", json!({
            "path": {"type": "array", "minItems": 2, "maxItems": 200, "items": {"type": "array", "items": {"type": "number"}, "minItems": 2, "maxItems": 2}},
            "button": button(),
            "duration_ms": {"type": "integer", "minimum": 0, "maximum": 60000}
        }), &["path"]),
        tool("pointer_scroll", "Scrolls the wheel at a point, one notch at a time with a human rhythm.", merge(target(), json!({
            "clicks": {"type": "integer", "minimum": 1, "maximum": 100},
            "direction": {"type": "string", "enum": ["up", "down", "left", "right"]}
        })), &["x", "y", "clicks", "direction"]),
        tool("key_chord", "Presses a shortcut like ctrl+1, ctrl+shift+s, f5, enter or up with realistic timing.", json!({"keys": {"type": "string"}}), &["keys"]),
        tool("type_text", "Types text with a human cadence (60 to 140 ms a character); line breaks are Enter, tabs are Tab. Refused in a password field.", json!({"text": {"type": "string", "maxLength": 10000}}), &["text"]),
        tool("wait_idle", "Waits until a small screen rectangle stops changing for 150 ms (a dialog finished opening), at most max_ms. Returns how long it took.", json!({"rect": rect(), "max_ms": {"type": "integer", "minimum": 0, "maximum": 30000}}), &["rect"]),
        tool("perform", "Runs a whole take in one call with exact timing: actions move, click, drag, scroll, chord, type, wait, wait_idle, marker (name, step), obs_start, obs_stop. Refuses to start while another window covers the capture monitor (require_clean, default true), checks again at each marker and reports dirty spans. Any mouse or key use by a person stops it at once. Returns each action's start and end on the session clock and, with OBS recording, the video clock.", json!({
            "actions": {"type": "array", "minItems": 1, "maxItems": 500, "items": {"type": "object", "properties": {"op": {"type": "string", "enum": ["move", "click", "drag", "scroll", "chord", "type", "wait", "wait_idle", "marker", "obs_start", "obs_stop"]}}, "required": ["op"]}},
            "require_clean": {"type": "boolean", "default": true},
            "capture_monitor": {"type": "string"},
            "seed": {"type": "integer", "minimum": 0}
        }), &["actions"]),
    ]
}

/// The tool names that send input (they need "Allow Claude to move the
/// mouse and type").
pub const INPUT_TOOLS: &[&str] = &[
    "pointer_move",
    "pointer_click",
    "pointer_drag",
    "pointer_scroll",
    "key_chord",
    "type_text",
    "perform",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalog_is_well_formed() {
        let c = catalog();
        let mut names: Vec<&str> = c.iter().map(|t| t.name.as_str()).collect();
        let n = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), n, "names are unique");
        for t in &c {
            assert!(
                t.name
                    .chars()
                    .all(|ch| ch.is_ascii_lowercase() || ch == '_'),
                "{}",
                t.name
            );
            assert!(t.description.ends_with('.'), "{}", t.name);
            assert!(!t.description.contains('\u{2014}') && !t.description.contains('\u{2013}'));
            assert_eq!(t.input_schema["type"], "object");
            let props = t.input_schema["properties"]
                .as_object()
                .expect("properties");
            for r in t.input_schema["required"].as_array().expect("required") {
                assert!(
                    props.contains_key(r.as_str().expect("s")),
                    "{} requires {r}",
                    t.name
                );
            }
        }
        for name in INPUT_TOOLS {
            assert!(c.iter().any(|t| t.name == *name), "{name}");
        }
        let v = serde_json::to_value(&c[0]).expect("j");
        assert!(v.get("inputSchema").is_some(), "MCP field name");
    }
}
