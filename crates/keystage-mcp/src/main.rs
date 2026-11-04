//! `idea-keystage-mcp`: the IDEA Keystage connector (docs/ARCHITECTURE.md
//! section 14, docs/MCP.md). A stdio MCP server (JSON-RPC 2.0, one message
//! per line) that relays tool calls to the running app's control API on
//! `127.0.0.1`. It holds no tools of its own: the list comes from the app,
//! so new tools arrive with app updates. It writes nothing but protocol to
//! stdout, logs nothing, and connects nowhere but `127.0.0.1`.

mod bridge;
mod http;

use std::io::{self, BufRead, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use bridge::{Bridge, HttpApp};

fn main() {
    let app = Arc::new(HttpApp::from_env());
    let out = Arc::new(Mutex::new(io::stdout()));
    let bridge = Arc::new(Bridge::new(app));

    // Tell the client when the app's tool list appears or changes (the app
    // was started after Claude, or updated).
    {
        let bridge = bridge.clone();
        let out = out.clone();
        thread::spawn(move || loop {
            thread::sleep(Duration::from_secs(5));
            if let Some(note) = bridge.poll_tools() {
                let mut o = out.lock().unwrap_or_else(|e| e.into_inner());
                let _ = writeln!(o, "{note}");
                let _ = o.flush();
            }
        });
    }

    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let bridge = bridge.clone();
        let out = out.clone();
        // A long `perform` must not hold up `ping` or `tools/list`.
        thread::spawn(move || {
            if let Some(reply) = bridge.handle_line(&line) {
                let mut o = out.lock().unwrap_or_else(|e| e.into_inner());
                let _ = writeln!(o, "{reply}");
                let _ = o.flush();
            }
        });
    }
}
