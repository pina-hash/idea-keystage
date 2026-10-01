//! Wire protocol v1 (ARCHITECTURE.md section 5): WebSocket text frames and
//! Tauri event payloads, identical JSON. Field names, the `"t"` tag values
//! and the lowercase enum strings are part of the contract; golden tests in
//! this module and sample files in `tests/fixtures/protocol/` pin them.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::event::{Callout, Mod, MouseButton};
use crate::serde_util::{u32_lenient, u64_lenient};

/// Protocol version carried in `hello.v`.
pub const PROTOCOL_VERSION: u32 = 1;
/// Name carried in `hello.app` and `/api/health`.
pub const APP_NAME: &str = "IDEA Keystage";
/// App version carried in `hello.version`.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Default local server port.
pub const DEFAULT_PORT: u16 = 47820;

/// Server to client messages.
// `hello` and `config` carry the whole overlay config. Messages are built and
// serialized at once and never stored in bulk, so their size does not matter
// and boxing would only make every caller wordier.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ServerMsg {
    Hello {
        v: u32,
        app: String,
        version: String,
        config: OverlayConfig,
        state: AppState,
        legends: Option<Legends>,
    },
    Config {
        config: OverlayConfig,
    },
    State {
        state: AppState,
    },
    Legends(Legends),
    Callout {
        callout: Callout,
    },
    Key {
        phys: String,
        key: String,
        down: bool,
        modifier: Option<Mod>,
        t_ms: u64,
    },
    Button {
        button: MouseButton,
        down: bool,
        x: f32,
        y: f32,
        t_ms: u64,
    },
    Wheel {
        dx: i32,
        dy: i32,
        x: f32,
        y: f32,
        t_ms: u64,
    },
    Move {
        x: f32,
        y: f32,
        t_ms: u64,
    },
    ReleaseAll {
        t_ms: u64,
    },
    Clear,
    Pong,
}

impl ServerMsg {
    /// A `hello` with this app's name, version and protocol version.
    pub fn hello(config: OverlayConfig, state: AppState, legends: Option<Legends>) -> ServerMsg {
        ServerMsg::Hello {
            v: PROTOCOL_VERSION,
            app: APP_NAME.to_string(),
            version: APP_VERSION.to_string(),
            config,
            state,
            legends,
        }
    }

    /// The `"t"` value of this message.
    pub fn tag(&self) -> &'static str {
        match self {
            ServerMsg::Hello { .. } => "hello",
            ServerMsg::Config { .. } => "config",
            ServerMsg::State { .. } => "state",
            ServerMsg::Legends(_) => "legends",
            ServerMsg::Callout { .. } => "callout",
            ServerMsg::Key { .. } => "key",
            ServerMsg::Button { .. } => "button",
            ServerMsg::Wheel { .. } => "wheel",
            ServerMsg::Move { .. } => "move",
            ServerMsg::ReleaseAll { .. } => "release_all",
            ServerMsg::Clear => "clear",
            ServerMsg::Pong => "pong",
        }
    }

    /// Compact JSON for the wire. Serialization of these plain types cannot
    /// fail; non-finite floats (which JSON cannot carry) are written as null
    /// by serde_json, and the renderer ignores such a message.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| String::from("{\"t\":\"clear\"}"))
    }

    pub fn from_json(s: &str) -> Result<ServerMsg, serde_json::Error> {
        serde_json::from_str(s)
    }
}

/// Which overlay a source shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Route {
    Keys,
    Keyboard,
    Mouse,
    Cursor,
    All,
}

/// Where a client runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClientHost {
    Obs,
    Browser,
    Native,
    Preview,
}

/// Client to server messages.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ClientMsg {
    /// Informational.
    Hello { route: Route, host: ClientHost },
    /// Answered by [`ServerMsg::Pong`].
    Ping,
}

impl ClientMsg {
    pub fn from_json(s: &str) -> Result<ClientMsg, serde_json::Error> {
        serde_json::from_str(s)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| String::from("{\"t\":\"ping\"}"))
    }
}

/// Keycap legends of the active layout, keyed by physical wire id.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Legends {
    /// Windows layout id (KLID), e.g. `00000409`.
    pub layout: String,
    pub legends: BTreeMap<String, String>,
}

/// Why the overlay is blank, as the wire says it. An excluded app is never named.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Suppressed {
    Password,
    Excluded,
}

/// SolidWorks profile switching.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SolidworksMode {
    /// Follow the foreground app.
    #[default]
    Auto,
    /// Always use the SolidWorks profile.
    On,
    /// Never use it.
    Off,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileRef {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordingState {
    pub active: bool,
    pub paused: bool,
    /// `HH:MM:SS.mmm` from OBS.
    pub timecode: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionState {
    pub active: bool,
    pub started_ms: u64,
    pub markers: u32,
}

/// App state the overlay needs to draw indicators and to hide Host A.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppState {
    pub paused: bool,
    pub suppressed: Option<Suppressed>,
    pub profile: Option<ProfileRef>,
    pub solidworks_mode: SolidworksMode,
    pub keyboard_visible: bool,
    /// False: the renderer in Host A draws nothing.
    pub native_visible: bool,
    /// Connected OBS or browser sources.
    pub obs_clients: u32,
    /// From OBS; `None` when not connected.
    pub recording: Option<RecordingState>,
    pub session: Option<SessionState>,
}

impl Default for AppState {
    fn default() -> Self {
        AppState {
            paused: false,
            suppressed: None,
            profile: None,
            solidworks_mode: SolidworksMode::Auto,
            keyboard_visible: true,
            native_visible: true,
            obs_clients: 0,
            recording: None,
            session: None,
        }
    }
}

/// Screen anchor for an overlay element.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Anchor {
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Font {
    #[default]
    Rajdhani,
    Orbitron,
    ShareTechMono,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CalloutSize {
    S,
    #[default]
    M,
    L,
    Xl,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyboardSize {
    S,
    #[default]
    M,
    L,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StackDirection {
    #[default]
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyboardLayout {
    #[default]
    #[serde(rename = "full")]
    Full,
    #[serde(rename = "tkl")]
    Tkl,
    #[serde(rename = "75")]
    Seventy5,
    #[serde(rename = "60")]
    Sixty,
    #[serde(rename = "laptop")]
    Laptop,
    #[serde(rename = "left")]
    Left,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CursorStyle {
    #[default]
    Ring,
    Spotlight,
    Dot,
}

/// Colors of the `custom` theme (CSS color strings, `#RRGGBB` or `#RRGGBBAA`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomTheme {
    pub accent: String,
    pub face: String,
    pub ink: String,
    pub glow: String,
}

impl Default for CustomTheme {
    fn default() -> Self {
        CustomTheme {
            accent: "#78b870".into(),
            face: "#1b1f1d".into(),
            ink: "#f4f6f5".into(),
            glow: "#8fe08a".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CalloutsConfig {
    pub enabled: bool,
    pub anchor: Anchor,
    pub offset_x: f64,
    pub offset_y: f64,
    #[serde(deserialize_with = "u32_lenient")]
    pub max_stack: u32,
    #[serde(deserialize_with = "u64_lenient")]
    pub linger_ms: u64,
    #[serde(deserialize_with = "u64_lenient")]
    pub fade_ms: u64,
    pub size: CalloutSize,
    pub direction: StackDirection,
    pub captions: bool,
    /// Arrows, Enter, Tab and Backspace as glyphs instead of words.
    pub symbols: bool,
}

impl Default for CalloutsConfig {
    fn default() -> Self {
        CalloutsConfig {
            enabled: true,
            anchor: Anchor::Bottom,
            offset_x: 0.0,
            offset_y: 96.0,
            max_stack: 4,
            linger_ms: 2200,
            fade_ms: 450,
            size: CalloutSize::M,
            direction: StackDirection::Up,
            captions: true,
            symbols: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyboardConfig {
    pub enabled: bool,
    pub layout: KeyboardLayout,
    pub anchor: Anchor,
    pub offset_x: f64,
    pub offset_y: f64,
    #[serde(deserialize_with = "u64_lenient")]
    pub decay_ms: u64,
    pub mouse: bool,
    pub size: KeyboardSize,
}

impl Default for KeyboardConfig {
    fn default() -> Self {
        KeyboardConfig {
            enabled: false,
            layout: KeyboardLayout::Full,
            anchor: Anchor::BottomLeft,
            offset_x: 32.0,
            offset_y: 32.0,
            decay_ms: 700,
            mouse: true,
            size: KeyboardSize::M,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MouseConfig {
    pub enabled: bool,
    pub anchor: Anchor,
    pub offset_x: f64,
    pub offset_y: f64,
}

impl Default for MouseConfig {
    fn default() -> Self {
        MouseConfig {
            enabled: false,
            anchor: Anchor::BottomRight,
            offset_x: 32.0,
            offset_y: 32.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CursorConfig {
    pub enabled: bool,
    pub style: CursorStyle,
    pub size: f64,
    pub color: String,
    pub opacity: f64,
    pub ripples: bool,
    pub trail: bool,
    pub wheel: bool,
}

impl Default for CursorConfig {
    fn default() -> Self {
        CursorConfig {
            enabled: false,
            style: CursorStyle::Ring,
            size: 44.0,
            color: "#78b870".into(),
            opacity: 0.85,
            ripples: true,
            trail: false,
            wheel: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct IndicatorsConfig {
    pub paused: bool,
    pub suppressed: bool,
}

impl Default for IndicatorsConfig {
    fn default() -> Self {
        IndicatorsConfig {
            paused: true,
            suppressed: true,
        }
    }
}

/// Theme ids the renderer ships.
pub const THEMES: [&str; 7] = [
    "idea",
    "studio",
    "graphite",
    "blueprint",
    "carbon",
    "glass",
    "custom",
];

/// How the overlay looks. The same defaults are declared by the renderer in
/// `src/lib/protocol.ts` (`defaultOverlayConfig`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OverlayConfig {
    pub theme: String,
    pub custom: Option<CustomTheme>,
    /// 1 = sized for 1080p.
    pub scale: f64,
    pub font: Font,
    pub callouts: CalloutsConfig,
    pub keyboard: KeyboardConfig,
    pub mouse: MouseConfig,
    pub cursor: CursorConfig,
    pub indicators: IndicatorsConfig,
}

impl Default for OverlayConfig {
    fn default() -> Self {
        OverlayConfig {
            theme: "idea".into(),
            custom: None,
            scale: 1.0,
            font: Font::Rajdhani,
            callouts: CalloutsConfig::default(),
            keyboard: KeyboardConfig::default(),
            mouse: MouseConfig::default(),
            cursor: CursorConfig::default(),
            indicators: IndicatorsConfig::default(),
        }
    }
}

/// Limits shared with the renderer's `LIMITS` in `src/lib/protocol.ts`.
pub mod limits {
    pub const SCALE: (f64, f64) = (0.25, 4.0);
    pub const MAX_STACK: (u32, u32) = (1, 12);
    pub const LINGER_MS: (u64, u64) = (300, 15_000);
    pub const FADE_MS: (u64, u64) = (0, 3_000);
    pub const DECAY_MS: (u64, u64) = (0, 5_000);
    pub const OFFSET: (f64, f64) = (-4_000.0, 4_000.0);
    pub const CURSOR_SIZE: (f64, f64) = (8.0, 240.0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Chip, ChipKind};
    use crate::CalloutKind;

    fn sample_callout() -> Callout {
        Callout {
            id: 7,
            kind: CalloutKind::Combo,
            steps: vec![vec![
                Chip::new("Ctrl", "Ctrl", ChipKind::Modifier),
                Chip::new("8", "8", ChipKind::Key),
            ]],
            text: None,
            count: 1,
            caption: Some("Fixture caption".into()),
            context: None,
            profile: Some("solidworks".into()),
            t_ms: 1234,
            updated_ms: 1234,
        }
    }

    fn round_trip(msg: &ServerMsg, expected: &str) {
        let json = msg.to_json();
        assert_eq!(json, expected);
        let back = ServerMsg::from_json(&json).unwrap();
        assert_eq!(&back, msg);
    }

    #[test]
    fn golden_callout() {
        // The shape of section 5's example. The caption is synthetic: the
        // contract's example pairing is not in the sourced data.
        round_trip(
            &ServerMsg::Callout {
                callout: sample_callout(),
            },
            r#"{"t":"callout","callout":{"id":7,"kind":"combo","steps":[[{"key":"Ctrl","label":"Ctrl","kind":"modifier"},{"key":"8","label":"8","kind":"key"}]],"text":null,"count":1,"caption":"Fixture caption","context":null,"profile":"solidworks","t_ms":1234,"updated_ms":1234}}"#,
        );
    }

    #[test]
    fn golden_key_button_wheel_move() {
        round_trip(
            &ServerMsg::Key {
                phys: "sc:1D".into(),
                key: "Ctrl".into(),
                down: true,
                modifier: Some(Mod::Ctrl),
                t_ms: 1234,
            },
            r#"{"t":"key","phys":"sc:1D","key":"Ctrl","down":true,"modifier":"ctrl","t_ms":1234}"#,
        );
        round_trip(
            &ServerMsg::Key {
                phys: "sc:1E".into(),
                key: "A".into(),
                down: false,
                modifier: None,
                t_ms: 9,
            },
            r#"{"t":"key","phys":"sc:1E","key":"A","down":false,"modifier":null,"t_ms":9}"#,
        );
        round_trip(
            &ServerMsg::Button {
                button: MouseButton::Middle,
                down: true,
                x: 0.52,
                y: 0.47,
                t_ms: 1234,
            },
            r#"{"t":"button","button":"middle","down":true,"x":0.52,"y":0.47,"t_ms":1234}"#,
        );
        round_trip(
            &ServerMsg::Wheel {
                dx: 0,
                dy: -120,
                x: 0.52,
                y: 0.47,
                t_ms: 1234,
            },
            r#"{"t":"wheel","dx":0,"dy":-120,"x":0.52,"y":0.47,"t_ms":1234}"#,
        );
        round_trip(
            &ServerMsg::Move {
                x: 0.52,
                y: 0.47,
                t_ms: 1234,
            },
            r#"{"t":"move","x":0.52,"y":0.47,"t_ms":1234}"#,
        );
    }

    #[test]
    fn golden_release_clear_pong() {
        round_trip(
            &ServerMsg::ReleaseAll { t_ms: 1234 },
            r#"{"t":"release_all","t_ms":1234}"#,
        );
        round_trip(&ServerMsg::Clear, r#"{"t":"clear"}"#);
        round_trip(&ServerMsg::Pong, r#"{"t":"pong"}"#);
    }

    #[test]
    fn golden_legends() {
        let mut map = BTreeMap::new();
        map.insert("sc:1E".to_string(), "A".to_string());
        map.insert("sc:27".to_string(), ";".to_string());
        round_trip(
            &ServerMsg::Legends(Legends {
                layout: "00000409".into(),
                legends: map,
            }),
            r#"{"t":"legends","layout":"00000409","legends":{"sc:1E":"A","sc:27":";"}}"#,
        );
    }

    #[test]
    fn golden_state() {
        round_trip(
            &ServerMsg::State {
                state: AppState::default(),
            },
            r#"{"t":"state","state":{"paused":false,"suppressed":null,"profile":null,"solidworks_mode":"auto","keyboard_visible":true,"native_visible":true,"obs_clients":0,"recording":null,"session":null}}"#,
        );
        let busy = AppState {
            paused: true,
            suppressed: Some(Suppressed::Excluded),
            profile: Some(ProfileRef {
                id: "solidworks".into(),
                name: "SolidWorks".into(),
            }),
            solidworks_mode: SolidworksMode::On,
            keyboard_visible: false,
            native_visible: false,
            obs_clients: 2,
            recording: Some(RecordingState {
                active: true,
                paused: false,
                timecode: "00:01:02.345".into(),
            }),
            session: Some(SessionState {
                active: true,
                started_ms: 1000,
                markers: 3,
            }),
        };
        round_trip(
            &ServerMsg::State { state: busy },
            r#"{"t":"state","state":{"paused":true,"suppressed":"excluded","profile":{"id":"solidworks","name":"SolidWorks"},"solidworks_mode":"on","keyboard_visible":false,"native_visible":false,"obs_clients":2,"recording":{"active":true,"paused":false,"timecode":"00:01:02.345"},"session":{"active":true,"started_ms":1000,"markers":3}}}"#,
        );
    }

    const DEFAULT_CONFIG_JSON: &str = r##"{"theme":"idea","custom":null,"scale":1.0,"font":"rajdhani","callouts":{"enabled":true,"anchor":"bottom","offset_x":0.0,"offset_y":96.0,"max_stack":4,"linger_ms":2200,"fade_ms":450,"size":"m","direction":"up","captions":true,"symbols":true},"keyboard":{"enabled":false,"layout":"full","anchor":"bottom-left","offset_x":32.0,"offset_y":32.0,"decay_ms":700,"mouse":true,"size":"m"},"mouse":{"enabled":false,"anchor":"bottom-right","offset_x":32.0,"offset_y":32.0},"cursor":{"enabled":false,"style":"ring","size":44.0,"color":"#78b870","opacity":0.85,"ripples":true,"trail":false,"wheel":true},"indicators":{"paused":true,"suppressed":true}}"##;

    #[test]
    fn golden_config() {
        round_trip(
            &ServerMsg::Config {
                config: OverlayConfig::default(),
            },
            &format!(r#"{{"t":"config","config":{DEFAULT_CONFIG_JSON}}}"#),
        );
    }

    #[test]
    fn golden_hello() {
        let msg = ServerMsg::hello(OverlayConfig::default(), AppState::default(), None);
        let expected = format!(
            r#"{{"t":"hello","v":1,"app":"IDEA Keystage","version":"{}","config":{},"state":{{"paused":false,"suppressed":null,"profile":null,"solidworks_mode":"auto","keyboard_visible":true,"native_visible":true,"obs_clients":0,"recording":null,"session":null}},"legends":null}}"#,
            APP_VERSION, DEFAULT_CONFIG_JSON
        );
        round_trip(&msg, &expected);
        assert_eq!(APP_VERSION, "0.1.0");
    }

    #[test]
    fn golden_client_messages() {
        let hello = ClientMsg::Hello {
            route: Route::Keys,
            host: ClientHost::Obs,
        };
        assert_eq!(
            hello.to_json(),
            r#"{"t":"hello","route":"keys","host":"obs"}"#
        );
        assert_eq!(
            ClientMsg::from_json(r#"{"t":"hello","route":"all","host":"native"}"#).unwrap(),
            ClientMsg::Hello {
                route: Route::All,
                host: ClientHost::Native
            }
        );
        assert_eq!(ClientMsg::Ping.to_json(), r#"{"t":"ping"}"#);
        assert_eq!(
            ClientMsg::from_json(r#"{"t":"ping"}"#).unwrap(),
            ClientMsg::Ping
        );
        assert!(ClientMsg::from_json(r#"{"t":"hello","route":"nope","host":"obs"}"#).is_err());
        assert!(ClientMsg::from_json(r#"{"t":"other"}"#).is_err());
    }

    #[test]
    fn enum_strings() {
        let s = |v: &dyn erased::Ser| v.json();
        assert_eq!(s(&Anchor::TopLeft), "\"top-left\"");
        assert_eq!(s(&Anchor::BottomRight), "\"bottom-right\"");
        assert_eq!(s(&Font::ShareTechMono), "\"share-tech-mono\"");
        assert_eq!(s(&CalloutSize::Xl), "\"xl\"");
        assert_eq!(s(&KeyboardLayout::Seventy5), "\"75\"");
        assert_eq!(s(&KeyboardLayout::Sixty), "\"60\"");
        assert_eq!(s(&KeyboardLayout::Tkl), "\"tkl\"");
        assert_eq!(s(&CursorStyle::Spotlight), "\"spotlight\"");
        assert_eq!(s(&SolidworksMode::Off), "\"off\"");
        assert_eq!(s(&Suppressed::Password), "\"password\"");
        assert_eq!(s(&MouseButton::X1), "\"x1\"");
        assert_eq!(s(&Mod::Win), "\"win\"");
        assert_eq!(s(&CalloutKind::Masked), "\"masked\"");
        assert_eq!(s(&ChipKind::Mouse), "\"mouse\"");
    }

    mod erased {
        pub trait Ser {
            fn json(&self) -> String;
        }
        impl<T: serde::Serialize> Ser for T {
            fn json(&self) -> String {
                serde_json::to_string(self).unwrap()
            }
        }
    }

    #[test]
    fn partial_config_takes_defaults() {
        let c: OverlayConfig =
            serde_json::from_str(r#"{"theme":"carbon","callouts":{"anchor":"top"}}"#).unwrap();
        assert_eq!(c.theme, "carbon");
        assert_eq!(c.callouts.anchor, Anchor::Top);
        assert_eq!(c.callouts.linger_ms, 2200);
        assert_eq!(c.keyboard, KeyboardConfig::default());
        let s: AppState = serde_json::from_str(r#"{"paused":true}"#).unwrap();
        assert!(s.paused && s.native_visible);
    }

    #[test]
    fn tags_match_serialization() {
        let msgs = vec![
            ServerMsg::hello(OverlayConfig::default(), AppState::default(), None),
            ServerMsg::Config {
                config: OverlayConfig::default(),
            },
            ServerMsg::State {
                state: AppState::default(),
            },
            ServerMsg::Legends(Legends::default()),
            ServerMsg::Callout {
                callout: sample_callout(),
            },
            ServerMsg::Key {
                phys: "sc:1E".into(),
                key: "A".into(),
                down: true,
                modifier: None,
                t_ms: 0,
            },
            ServerMsg::Button {
                button: MouseButton::Left,
                down: true,
                x: 0.0,
                y: 0.0,
                t_ms: 0,
            },
            ServerMsg::Wheel {
                dx: 0,
                dy: 120,
                x: 0.0,
                y: 0.0,
                t_ms: 0,
            },
            ServerMsg::Move {
                x: 0.0,
                y: 0.0,
                t_ms: 0,
            },
            ServerMsg::ReleaseAll { t_ms: 0 },
            ServerMsg::Clear,
            ServerMsg::Pong,
        ];
        for m in msgs {
            let v: serde_json::Value = serde_json::from_str(&m.to_json()).unwrap();
            assert_eq!(v["t"], m.tag());
        }
    }
}
