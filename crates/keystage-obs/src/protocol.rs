//! obs-websocket 5 messages (protocol.md, read 2026-10-01): every message is
//! `{ "op": number, "d": object }`. Only what Keystage needs is modeled:
//! Hello, Identify, Identified, Event, Request and RequestResponse.

use serde_json::{json, Map, Value};

/// Opcodes.
pub const OP_HELLO: u64 = 0;
pub const OP_IDENTIFY: u64 = 1;
pub const OP_IDENTIFIED: u64 = 2;
pub const OP_EVENT: u64 = 5;
pub const OP_REQUEST: u64 = 6;
pub const OP_REQUEST_RESPONSE: u64 = 7;

/// The RPC version Keystage speaks ("Latest Supported RPC Version: 1").
pub const RPC_VERSION: u64 = 1;

/// `EventSubscription` bits.
pub mod subscription {
    pub const GENERAL: u32 = 1;
    pub const OUTPUTS: u32 = 1 << 6;
}

/// General (for `ExitStarted`) plus Outputs (for `RecordStateChanged`) = 65.
/// Never the literal `All`, whose value differs between obs-websocket 5.6
/// (2047) and 5.7 (4095), and which would add events Keystage ignores.
pub const EVENT_SUBSCRIPTIONS: u32 = subscription::GENERAL | subscription::OUTPUTS;

/// `WebSocketCloseCode` values the client acts on.
pub mod close_code {
    /// "The authentication attempt (via `Identify`) failed."
    pub const AUTHENTICATION_FAILED: u16 = 4009;
    /// The requested RPC version is not supported.
    pub const UNSUPPORTED_RPC_VERSION: u16 = 4010;
    /// "If you receive this code, you must not automatically reconnect."
    pub const SESSION_INVALIDATED: u16 = 4011;
}

/// `RequestStatus` codes the client acts on.
pub mod status_code {
    pub const SUCCESS: u32 = 100;
    pub const UNKNOWN_REQUEST_TYPE: u32 = 204;
    pub const OUTPUT_NOT_RUNNING: u32 = 501;
    pub const OUTPUT_PAUSED: u32 = 502;
    pub const REQUEST_PROCESSING_FAILED: u32 = 702;
}

/// The requests Keystage sends.
pub const GET_VERSION: &str = "GetVersion";
pub const GET_RECORD_STATUS: &str = "GetRecordStatus";
pub const CREATE_RECORD_CHAPTER: &str = "CreateRecordChapter";

/// A parsed server message.
#[derive(Clone, Debug, PartialEq)]
pub enum Incoming {
    Hello {
        obs_version: Option<String>,
        ws_version: Option<String>,
        rpc_version: u64,
        /// `(challenge, salt)` when OBS requires authentication.
        auth: Option<(String, String)>,
    },
    Identified {
        rpc_version: u64,
    },
    Event {
        event_type: String,
        data: Value,
    },
    Response {
        request_type: String,
        request_id: String,
        ok: bool,
        code: u32,
        data: Value,
    },
    /// Valid JSON with an op Keystage does not use.
    Other,
}

fn str_field(d: &Map<String, Value>, k: &str) -> Option<String> {
    d.get(k).and_then(Value::as_str).map(str::to_string)
}

/// Parses one text frame. `None` for anything that is not a protocol
/// message.
pub fn parse(text: &str) -> Option<Incoming> {
    let v: Value = serde_json::from_str(text).ok()?;
    let op = v.get("op")?.as_u64()?;
    let d = v.get("d")?.as_object()?;
    Some(match op {
        OP_HELLO => {
            let auth = d
                .get("authentication")
                .and_then(Value::as_object)
                .and_then(|a| Some((str_field(a, "challenge")?, str_field(a, "salt")?)));
            Incoming::Hello {
                obs_version: str_field(d, "obsStudioVersion"),
                ws_version: str_field(d, "obsWebSocketVersion"),
                rpc_version: d.get("rpcVersion").and_then(Value::as_u64).unwrap_or(0),
                auth,
            }
        }
        OP_IDENTIFIED => Incoming::Identified {
            rpc_version: d
                .get("negotiatedRpcVersion")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        },
        OP_EVENT => Incoming::Event {
            event_type: str_field(d, "eventType")?,
            data: d.get("eventData").cloned().unwrap_or(Value::Null),
        },
        OP_REQUEST_RESPONSE => {
            let st = d.get("requestStatus")?.as_object()?;
            Incoming::Response {
                request_type: str_field(d, "requestType").unwrap_or_default(),
                request_id: str_field(d, "requestId")?,
                ok: st.get("result").and_then(Value::as_bool).unwrap_or(false),
                code: st
                    .get("code")
                    .and_then(Value::as_u64)
                    .and_then(|c| u32::try_from(c).ok())
                    .unwrap_or(0),
                data: d.get("responseData").cloned().unwrap_or(Value::Null),
            }
        }
        _ => Incoming::Other,
    })
}

/// The Identify message (op 1).
pub fn identify(authentication: Option<&str>, event_subscriptions: u32) -> String {
    let mut d = Map::new();
    d.insert("rpcVersion".into(), json!(RPC_VERSION));
    if let Some(a) = authentication {
        d.insert("authentication".into(), json!(a));
    }
    d.insert("eventSubscriptions".into(), json!(event_subscriptions));
    json!({ "op": OP_IDENTIFY, "d": Value::Object(d) }).to_string()
}

/// A Request message (op 6).
pub fn request(request_type: &str, request_id: &str, data: Option<Value>) -> String {
    let mut d = Map::new();
    d.insert("requestType".into(), json!(request_type));
    d.insert("requestId".into(), json!(request_id));
    if let Some(data) = data {
        d.insert("requestData".into(), data);
    }
    json!({ "op": OP_REQUEST, "d": Value::Object(d) }).to_string()
}

/// Fields of a `GetRecordStatus` answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordStatus {
    pub active: bool,
    pub paused: bool,
    pub timecode: String,
    pub duration_ms: u64,
}

impl RecordStatus {
    pub fn from_data(data: &Value) -> Option<RecordStatus> {
        let d = data.as_object()?;
        let duration = d
            .get("outputDuration")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        Some(RecordStatus {
            active: d.get("outputActive")?.as_bool()?,
            paused: d
                .get("outputPaused")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            timecode: str_field(d, "outputTimecode").unwrap_or_default(),
            duration_ms: if duration.is_finite() && duration > 0.0 {
                duration.round() as u64
            } else {
                0
            },
        })
    }
}

/// Fields of a `GetVersion` answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    pub obs_version: String,
    pub ws_version: String,
    pub available_requests: Vec<String>,
}

impl Version {
    pub fn from_data(data: &Value) -> Option<Version> {
        let d = data.as_object()?;
        Some(Version {
            obs_version: str_field(d, "obsVersion").unwrap_or_default(),
            ws_version: str_field(d, "obsWebSocketVersion").unwrap_or_default(),
            available_requests: d
                .get("availableRequests")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
        })
    }

    pub fn has(&self, request: &str) -> bool {
        self.available_requests.iter().any(|r| r == request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subscriptions_are_general_and_outputs() {
        assert_eq!(EVENT_SUBSCRIPTIONS, 65);
    }

    #[test]
    fn parses_the_protocol_examples() {
        // Hello with auth, verbatim from protocol.md.
        let hello = r#"{"op":0,"d":{"obsStudioVersion":"30.2.2","obsWebSocketVersion":"5.5.2","rpcVersion":1,"authentication":{"challenge":"+IxH4CnCiqpX1rM9scsNynZzbOe4KhDeYcTNS3PDaeY=","salt":"lM1GncleQOaCu9lT1yeUZhFYnqhsLLP1G5lAGo3ixaI="}}}"#;
        assert_eq!(
            parse(hello),
            Some(Incoming::Hello {
                obs_version: Some("30.2.2".into()),
                ws_version: Some("5.5.2".into()),
                rpc_version: 1,
                auth: Some((
                    "+IxH4CnCiqpX1rM9scsNynZzbOe4KhDeYcTNS3PDaeY=".into(),
                    "lM1GncleQOaCu9lT1yeUZhFYnqhsLLP1G5lAGo3ixaI=".into()
                )),
            })
        );
        let no_auth = r#"{"op":0,"d":{"obsStudioVersion":"30.2.2","obsWebSocketVersion":"5.5.2","rpcVersion":1}}"#;
        assert!(matches!(
            parse(no_auth),
            Some(Incoming::Hello { auth: None, .. })
        ));
        assert_eq!(
            parse(r#"{"op":2,"d":{"negotiatedRpcVersion":1}}"#),
            Some(Incoming::Identified { rpc_version: 1 })
        );
        let ev = parse(r#"{"op":5,"d":{"eventType":"StudioModeStateChanged","eventIntent":1,"eventData":{"studioModeEnabled":true}}}"#).unwrap();
        assert!(
            matches!(ev, Incoming::Event { ref event_type, .. } if event_type == "StudioModeStateChanged")
        );
        let fail = parse(r#"{"op":7,"d":{"requestType":"SetCurrentProgramScene","requestId":"f819dcf0-89cc-11eb-8f0e-382c4ac93b9c","requestStatus":{"result":false,"code":608,"comment":"Parameter: sceneName"}}}"#).unwrap();
        assert_eq!(
            fail,
            Incoming::Response {
                request_type: "SetCurrentProgramScene".into(),
                request_id: "f819dcf0-89cc-11eb-8f0e-382c4ac93b9c".into(),
                ok: false,
                code: 608,
                data: Value::Null,
            }
        );
        let exit = parse(r#"{"op":5,"d":{"eventType":"ExitStarted","eventIntent":1}}"#).unwrap();
        assert!(matches!(
            exit,
            Incoming::Event {
                data: Value::Null,
                ..
            }
        ));
        assert_eq!(parse(r#"{"op":9,"d":{}}"#), Some(Incoming::Other));
        assert_eq!(parse("not json"), None);
        assert_eq!(parse(r#"{"op":"0","d":{}}"#), None);
        assert_eq!(parse(r#"{"op":5,"d":{}}"#), None, "event without a type");
    }

    #[test]
    fn builds_identify_and_requests() {
        let v: Value = serde_json::from_str(&identify(
            Some("1Ct943GAT+6YQUUX47Ia/ncufilbe6+oD6lY+5kaCu4="),
            EVENT_SUBSCRIPTIONS,
        ))
        .unwrap();
        assert_eq!(
            v,
            json!({"op":1,"d":{"rpcVersion":1,"authentication":"1Ct943GAT+6YQUUX47Ia/ncufilbe6+oD6lY+5kaCu4=","eventSubscriptions":65}})
        );
        let v: Value = serde_json::from_str(&identify(None, 0)).unwrap();
        assert!(v["d"].get("authentication").is_none());
        let v: Value = serde_json::from_str(&request(GET_VERSION, "ks-1", None)).unwrap();
        assert_eq!(
            v,
            json!({"op":6,"d":{"requestType":"GetVersion","requestId":"ks-1"}})
        );
        let v: Value = serde_json::from_str(&request(
            CREATE_RECORD_CHAPTER,
            "ks-7",
            Some(json!({"chapterName":"Part one"})),
        ))
        .unwrap();
        assert_eq!(v["d"]["requestData"]["chapterName"], "Part one");
    }

    #[test]
    fn record_status_and_version_fields() {
        let s = RecordStatus::from_data(&json!({"outputActive":true,"outputPaused":false,"outputTimecode":"00:01:23.450","outputDuration":83450,"outputBytes":41943040})).unwrap();
        assert_eq!(
            s,
            RecordStatus {
                active: true,
                paused: false,
                timecode: "00:01:23.450".into(),
                duration_ms: 83_450
            }
        );
        let f =
            RecordStatus::from_data(&json!({"outputActive":false,"outputDuration":12.6})).unwrap();
        assert_eq!(f.duration_ms, 13);
        assert!(RecordStatus::from_data(&json!({})).is_none());
        let v = Version::from_data(&json!({"obsVersion":"32.2.2","obsWebSocketVersion":"5.7.4","rpcVersion":1,"availableRequests":["GetVersion","CreateRecordChapter"]})).unwrap();
        assert!(v.has(CREATE_RECORD_CHAPTER));
        assert!(!v.has("StartRecord"));
        assert_eq!(v.obs_version, "32.2.2");
    }
}
