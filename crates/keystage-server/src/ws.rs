//! One WebSocket client: `hello` first, then every published message in
//! order. A client that falls behind is resynchronized with a fresh `hello`
//! and continues with the newest messages still queued. Nothing a client
//! sends is logged.

use std::sync::Arc;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::ws::{close_code, CloseFrame, Message, Utf8Bytes, WebSocket};
use futures_util::{SinkExt, StreamExt};
use keystage_core::protocol::{ClientMsg, ServerMsg};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::watch;
use tokio::time::{interval_at, timeout, Instant, MissedTickBehavior};

use crate::hub::{ClientGuard, Hub};

/// Largest message a client may send (its `hello` and `ping` are tiny).
pub(crate) const MAX_CLIENT_MESSAGE: usize = 4 * 1024;
/// The server pings this often ...
const KEEPALIVE: Duration = Duration::from_secs(15);
/// ... and drops a client it has heard nothing from for this long.
const IDLE_TIMEOUT: Duration = Duration::from_secs(45);
/// A client that cannot take a message for this long is dropped.
const SEND_TIMEOUT: Duration = Duration::from_secs(10);

type Sink = futures_util::stream::SplitSink<WebSocket, Message>;

async fn send(sink: &mut Sink, msg: Message) -> bool {
    matches!(timeout(SEND_TIMEOUT, sink.send(msg)).await, Ok(Ok(())))
}

fn text(s: &str) -> Message {
    Message::Text(Utf8Bytes::from(s))
}

pub(crate) async fn run(
    socket: WebSocket,
    hub: Hub,
    guard: ClientGuard,
    mut stop: watch::Receiver<bool>,
) {
    if *stop.borrow() {
        return;
    }
    let (mut sink, mut stream) = socket.split();
    // Subscribe before taking the snapshot so nothing published in between
    // is lost (an update the snapshot already holds is harmless: callouts
    // are upserts).
    let mut rx = hub.subscribe();
    let hello: Arc<str> = hub.snapshot();
    if !send(&mut sink, text(&hello)).await {
        return;
    }
    tracing::debug!(clients = hub.counts().total, "overlay client connected");

    let mut keepalive = interval_at(Instant::now() + KEEPALIVE, KEEPALIVE);
    keepalive.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut last_heard = Instant::now();
    let pong = ServerMsg::Pong.to_json();

    loop {
        tokio::select! {
            biased;
            _ = stop.changed() => {
                let _ = timeout(
                    Duration::from_millis(500),
                    sink.send(Message::Close(Some(CloseFrame {
                        code: close_code::AWAY,
                        reason: Utf8Bytes::from_static("IDEA Keystage is stopping"),
                    }))),
                )
                .await;
                break;
            }
            incoming = stream.next() => match incoming {
                Some(Ok(Message::Text(t))) => {
                    last_heard = Instant::now();
                    match ClientMsg::from_json(t.as_str()) {
                        Ok(ClientMsg::Hello { route, host }) => guard.identify(route, host),
                        Ok(ClientMsg::Ping) => {
                            if !send(&mut sink, text(&pong)).await {
                                break;
                            }
                        }
                        // Unknown or malformed: ignored, never logged.
                        Err(_) => {}
                    }
                }
                Some(Ok(Message::Binary(_) | Message::Ping(_) | Message::Pong(_))) => {
                    last_heard = Instant::now();
                }
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
            },
            msg = rx.recv() => match msg {
                Ok(m) => {
                    if !send(&mut sink, text(&m)).await {
                        break;
                    }
                }
                Err(RecvError::Lagged(skipped)) => {
                    // The oldest messages are gone. Send a fresh snapshot,
                    // then continue with the newest messages still queued:
                    // they are in order and end with the latest of each kind
                    // (callouts are upserts), so the client converges on the
                    // current state.
                    tracing::debug!(skipped, "overlay client fell behind; sent a fresh hello");
                    let hello = hub.snapshot();
                    if !send(&mut sink, text(&hello)).await {
                        break;
                    }
                }
                Err(RecvError::Closed) => break,
            },
            _ = keepalive.tick() => {
                if last_heard.elapsed() > IDLE_TIMEOUT {
                    tracing::debug!("overlay client stopped answering; closing");
                    break;
                }
                if !send(&mut sink, Message::Ping(Bytes::new())).await {
                    break;
                }
            }
        }
    }
    drop(guard);
    tracing::debug!(clients = hub.counts().total, "overlay client disconnected");
}
