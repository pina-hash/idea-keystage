//! The MCP side: `initialize`, `ping`, `tools/list`, `tools/call`, and the
//! `notifications/tools/list_changed` the poller sends when the app's list
//! appears or changes (MCP specification, "Tools",
//! https://modelcontextprotocol.io/specification/2025-06-18/server/tools).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use keystage_automation::token::{ControlFile, CONTROL_FILE};
use keystage_automation::tools::ToolDef;
use serde_json::{json, Value};

use crate::http;

/// Protocol versions this server speaks, newest first.
pub const VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

/// The app's identifier folder (Tauri `app_local_data_dir`).
pub const APP_ID: &str = "com.ideabosco.keystage";

/// Why the app could not be reached, in words for Claude to pass on.
pub const NOT_RUNNING: &str = "IDEA Keystage is not running on this computer, or its connector is turned off. Start IDEA Keystage from the Start menu (and check that \"Allow the IDEA Keystage connector\" is on, on its General page), then try again.";

/// What the bridge needs from the app.
pub trait App: Send + Sync {
    fn tools(&self) -> Result<Vec<ToolDef>, String>;
    /// `Ok(result)` or `Err(sentence)`.
    fn call(&self, name: &str, args: &Value) -> Result<Value, String>;
}

/// The real app, over HTTP.
pub struct HttpApp {
    control: PathBuf,
}

impl HttpApp {
    /// `IDEA_KEYSTAGE_CONTROL` names `control.json` directly (tests); the
    /// default is `%LOCALAPPDATA%\com.ideabosco.keystage\control.json`.
    pub fn from_env() -> HttpApp {
        let control = std::env::var_os("IDEA_KEYSTAGE_CONTROL")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                let base = std::env::var_os("LOCALAPPDATA")
                    .map(PathBuf::from)
                    .or_else(|| {
                        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share"))
                    })
                    .unwrap_or_default();
                base.join(APP_ID).join(CONTROL_FILE)
            });
        HttpApp { control }
    }

    fn file(&self) -> Result<ControlFile, String> {
        let text = std::fs::read_to_string(&self.control).map_err(|_| NOT_RUNNING.to_string())?;
        serde_json::from_str(&text).map_err(|_| NOT_RUNNING.to_string())
    }

    fn send(
        &self,
        method: &str,
        path: &str,
        body: Option<&str>,
        wait: Duration,
    ) -> Result<Value, String> {
        let f = self.file()?;
        let r = http::send(method, f.port, path, &f.token, body, wait)
            .map_err(|_| NOT_RUNNING.to_string())?;
        let v: Value = serde_json::from_str(&r.body).map_err(|_| {
            format!(
                "IDEA Keystage answered with status {} and no readable body.",
                r.status
            )
        })?;
        if r.status == 401 {
            return Err("IDEA Keystage refused the connector's key. Quit and start IDEA Keystage again, then try again.".into());
        }
        Ok(v)
    }
}

impl App for HttpApp {
    fn tools(&self) -> Result<Vec<ToolDef>, String> {
        let v = self.send("GET", "/api/control/tools", None, Duration::from_secs(5))?;
        serde_json::from_value(v["tools"].clone()).map_err(|_| NOT_RUNNING.to_string())
    }

    fn call(&self, name: &str, args: &Value) -> Result<Value, String> {
        let body = json!({ "tool": name, "arguments": args }).to_string();
        // `perform` may run for many minutes.
        let v = self.send(
            "POST",
            "/api/control/call",
            Some(&body),
            Duration::from_secs(45 * 60),
        )?;
        if v["ok"] == json!(true) {
            Ok(v["result"].clone())
        } else {
            Err(v["error"]
                .as_str()
                .unwrap_or("IDEA Keystage could not do that.")
                .to_string())
        }
    }
}

/// The tool listed while the app cannot be reached.
fn offline_tools() -> Vec<ToolDef> {
    vec![ToolDef {
        name: "status".into(),
        description: "IDEA Keystage's state. While the app is not running this is the only tool; start IDEA Keystage and its tools appear.".into(),
        input_schema: json!({"type": "object", "properties": {}, "additionalProperties": false}),
    }]
}

pub struct Bridge {
    app: Arc<dyn App>,
    last: Mutex<Option<Vec<String>>>,
}

fn reply(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn error(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

impl Bridge {
    pub fn new(app: Arc<dyn App>) -> Bridge {
        Bridge {
            app,
            last: Mutex::new(None),
        }
    }

    fn tool_list(&self) -> Vec<ToolDef> {
        let tools = self.app.tools().unwrap_or_else(|_| offline_tools());
        let names = tools.iter().map(|t| t.name.clone()).collect();
        *self.last.lock().unwrap_or_else(|e| e.into_inner()) = Some(names);
        tools
    }

    /// The `list_changed` notification when the app's list differs from
    /// what was last listed.
    pub fn poll_tools(&self) -> Option<String> {
        let tools = self.app.tools().unwrap_or_else(|_| offline_tools());
        let names: Vec<String> = tools.iter().map(|t| t.name.clone()).collect();
        let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
        match last.as_ref() {
            // Nothing listed yet: the client has not asked.
            None => None,
            Some(prev) if *prev == names => None,
            Some(_) => {
                *last = Some(names);
                Some(
                    json!({"jsonrpc": "2.0", "method": "notifications/tools/list_changed"})
                        .to_string(),
                )
            }
        }
    }

    /// One incoming line; the reply line, if any.
    pub fn handle_line(&self, line: &str) -> Option<String> {
        let msg: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => return Some(error(&Value::Null, -32700, "Parse error").to_string()),
        };
        self.handle(&msg).map(|v| v.to_string())
    }

    pub fn handle(&self, msg: &Value) -> Option<Value> {
        let method = msg["method"].as_str()?;
        let id = msg.get("id")?.clone();
        Some(match method {
            "initialize" => {
                let asked = msg["params"]["protocolVersion"].as_str().unwrap_or("");
                let version = VERSIONS
                    .iter()
                    .find(|v| **v == asked)
                    .copied()
                    .unwrap_or(VERSIONS[0]);
                reply(
                    &id,
                    json!({
                        "protocolVersion": version,
                        "capabilities": {"tools": {"listChanged": true}},
                        "serverInfo": {"name": "idea-keystage", "title": "IDEA Keystage", "version": env!("CARGO_PKG_VERSION")},
                        "instructions": "Drives IDEA Keystage on this Windows computer to record tutorials: Session Logs, OBS recording, the overlay, and (when the user allowed it in IDEA Keystage) the real mouse and keyboard. Call status first. Rehearse to find coordinates, then record the take in one perform call. A person touching the mouse or keyboard stops automation at once."
                    }),
                )
            }
            "ping" => reply(&id, json!({})),
            "tools/list" => reply(&id, json!({"tools": self.tool_list()})),
            "tools/call" => {
                let name = msg["params"]["name"].as_str().unwrap_or("");
                let args = msg["params"]
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                match self.app.call(name, &args) {
                    Ok(result) => {
                        let text = serde_json::to_string_pretty(&result).unwrap_or_default();
                        let mut r =
                            json!({"content": [{"type": "text", "text": text}], "isError": false});
                        if result.is_object() {
                            r["structuredContent"] = result;
                        }
                        reply(&id, r)
                    }
                    Err(e) => reply(
                        &id,
                        json!({"content": [{"type": "text", "text": e}], "isError": true}),
                    ),
                }
            }
            _ => error(&id, -32601, "Method not found"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct Fake {
        up: AtomicBool,
    }

    impl App for Fake {
        fn tools(&self) -> Result<Vec<ToolDef>, String> {
            if self.up.load(Ordering::SeqCst) {
                Ok(keystage_automation::tools::catalog())
            } else {
                Err(NOT_RUNNING.into())
            }
        }
        fn call(&self, name: &str, args: &Value) -> Result<Value, String> {
            match name {
                "status" if self.up.load(Ordering::SeqCst) => {
                    Ok(json!({"version": "0.3.0", "args": args}))
                }
                "status" => Err(NOT_RUNNING.into()),
                _ => Err("no".into()),
            }
        }
    }

    fn bridge(up: bool) -> (Arc<Fake>, Bridge) {
        let f = Arc::new(Fake {
            up: AtomicBool::new(up),
        });
        let b = Bridge::new(f.clone());
        (f, b)
    }

    #[test]
    fn initialize_ping_and_unknown_methods() {
        let (_, b) = bridge(true);
        let r = b
            .handle(&json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": {"name": "claude-ai", "version": "1"}}}))
            .expect("reply");
        assert_eq!(r["id"], 1);
        assert_eq!(r["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(r["result"]["capabilities"]["tools"]["listChanged"], true);
        let r = b.handle(&json!({"jsonrpc": "2.0", "id": 2, "method": "initialize", "params": {"protocolVersion": "1999-01-01"}})).expect("r");
        assert_eq!(r["result"]["protocolVersion"], VERSIONS[0]);
        assert_eq!(
            b.handle(&json!({"jsonrpc": "2.0", "id": "p", "method": "ping"}))
                .expect("r")["result"],
            json!({})
        );
        assert_eq!(
            b.handle(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"})),
            None,
            "no reply to a notification"
        );
        assert_eq!(
            b.handle(&json!({"jsonrpc": "2.0", "id": 3, "method": "resources/list"}))
                .expect("r")["error"]["code"],
            -32601
        );
        let parse = b.handle_line("{not json").expect("r");
        assert!(parse.contains("-32700"));
    }

    #[test]
    fn tools_come_from_the_app_and_calls_are_relayed() {
        let (f, b) = bridge(false);
        let r = b
            .handle(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}))
            .expect("r");
        assert_eq!(
            r["result"]["tools"].as_array().map(Vec::len),
            Some(1),
            "offline: status only"
        );
        let r = b.handle(&json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "status"}})).expect("r");
        assert_eq!(r["result"]["isError"], true);
        assert!(r["result"]["content"][0]["text"]
            .as_str()
            .expect("t")
            .contains("not running"));
        assert_eq!(b.poll_tools(), None, "still offline");
        f.up.store(true, Ordering::SeqCst);
        let note = b.poll_tools().expect("changed");
        assert!(note.contains("notifications/tools/list_changed"));
        assert_eq!(b.poll_tools(), None, "said once");
        let r = b
            .handle(&json!({"jsonrpc": "2.0", "id": 3, "method": "tools/list"}))
            .expect("r");
        assert!(r["result"]["tools"].as_array().map(Vec::len).unwrap_or(0) > 20);
        let r = b.handle(&json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "status", "arguments": {"a": 1}}})).expect("r");
        assert_eq!(r["result"]["isError"], false);
        assert_eq!(r["result"]["structuredContent"]["version"], "0.3.0");
        assert_eq!(r["result"]["structuredContent"]["args"]["a"], 1);
    }

    #[test]
    fn the_http_app_reads_the_control_file_and_sends_the_token() {
        let l = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = l.local_addr().expect("a").port();
        let token = "a".repeat(64);
        let d = tempfile::tempdir().expect("d");
        let control = d.path().join("control.json");
        std::fs::write(
            &control,
            json!({"port": port, "token": token, "version": "0.3.0"}).to_string(),
        )
        .expect("w");
        let seen = Arc::new(Mutex::new(Vec::<String>::new()));
        let seen2 = seen.clone();
        std::thread::spawn(move || {
            for s in l.incoming().take(2) {
                let mut s = s.expect("s");
                let mut r = BufReader::new(s.try_clone().expect("c"));
                let mut head = String::new();
                loop {
                    let mut line = String::new();
                    r.read_line(&mut line).expect("r");
                    if line.trim().is_empty() {
                        break;
                    }
                    head.push_str(&line);
                }
                seen2.lock().expect("l").push(head.clone());
                let body = if head.starts_with("GET") {
                    json!({"tools": keystage_automation::tools::catalog()}).to_string()
                } else {
                    json!({"ok": false, "error": "Refused: turn it on first."}).to_string()
                };
                let _ = write!(
                    s,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        let app = HttpApp { control };
        assert!(app.tools().expect("tools").len() > 20);
        assert_eq!(
            app.call("perform", &json!({"actions": []})),
            Err("Refused: turn it on first.".into())
        );
        let seen = seen.lock().expect("l").clone();
        assert!(seen
            .iter()
            .all(|h| h.contains(&format!("Authorization: Bearer {}", "a".repeat(64)))));
        let missing = HttpApp {
            control: d.path().join("none.json"),
        };
        assert_eq!(missing.tools().err().as_deref(), Some(NOT_RUNNING));
    }
}
