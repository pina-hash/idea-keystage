/**
 * Settings window data shapes. Field names are the exact serde names of the
 * Rust types in crates/keystage-core (settings.rs, rules.rs, import.rs,
 * profile.rs, ipc.rs), so a value goes over IPC unchanged. The overlay part
 * of the settings is the wire protocol's OverlayConfig (src/lib/protocol.ts).
 */
import type {
	OverlayConfig,
	ProfileRef,
	RecordingState,
	Route,
	SolidworksMode,
	Suppression,
} from '../lib/protocol';

/* ------------------------------------------------------------------------ */
/* Settings (ARCHITECTURE.md section 7, keystage-core settings.rs)           */
/* ------------------------------------------------------------------------ */

export type TypingMode = 'shortcuts_only' | 'all';

/** keystage-core `DisplayRules` (rules.rs). */
export interface DisplayRules {
	typing: TypingMode;
	typed_values: boolean;
	show_modifier_side: boolean;
	repeat_window_ms: number;
	chord_window_ms: number;
	chord_max_steps: number;
	text_run_gap_ms: number;
	mouse_callouts: boolean;
	plain_left_clicks: boolean;
	show_injected: boolean;
	hidden_combos: string[];
	drag_threshold_px: number;
	double_click_ms: number;
	double_click_px: number;
	gesture_min_px: number;
	gesture_max_ms: number;
	modifier_verify_ms: number;
	modifier_stale_ms: number;
}

/** keystage-core `ProfileDisplay`: an absent or null field keeps the profile's own value. */
export interface ProfileDisplay {
	mode?: TypingMode | null;
	typed_values?: boolean | null;
	mouse_captions?: boolean | null;
}

/** keystage-core `UserShortcut` (import.rs). `source` is always "user-import". */
export interface UserShortcut {
	keys: string[];
	doc_keys: string;
	command: string;
	category: string;
	source: string;
}

export interface HostSettings {
	native_overlay: boolean;
	hide_native_when_obs: boolean;
	/** Monitor device name (`\\.\DISPLAY1`); null is the primary monitor. */
	monitor: string | null;
	exclude_from_capture: boolean;
}

export interface ServerSettings {
	port: number;
}

export interface ProfileSettings {
	solidworks_mode: SolidworksMode;
	/** The user's own profiles (keystage.profile/1 objects), kept as read. */
	user: unknown[];
	solidworks_user_shortcuts: UserShortcut[];
	solidworks_display: ProfileDisplay;
}

export interface PrivacySettings {
	excluded_apps: string[];
}

export const HOTKEY_NAMES = [
	'pause',
	'toggle_keyboard',
	'toggle_solidworks',
	'chapter_marker',
	'session_toggle',
] as const;
export type HotkeyName = (typeof HOTKEY_NAMES)[number];
export type Hotkeys = Record<HotkeyName, string>;

export interface SoundSettings {
	enabled: boolean;
	pack: string;
	/** 0 to 1. */
	volume: number;
}

export interface ObsSettings {
	enabled: boolean;
	host: string;
	port: number;
	/** Base64 DPAPI blob written by the app; the settings window never writes it. */
	password_dpapi: string | null;
	auto_session: boolean;
}

export interface SessionSettings {
	folder: string | null;
	autosave: boolean;
}

export interface StartupSettings {
	start_with_windows: boolean;
	/**
	 * Open this window when the app starts. Not in keystage-core 0.1.0's
	 * `StartupSettings` yet: requested in .work/notes/settings.md. Absent
	 * means true.
	 */
	show_settings?: boolean;
}

export interface Settings {
	version: number;
	first_run_done: boolean;
	host: HostSettings;
	server: ServerSettings;
	overlay: OverlayConfig;
	display: DisplayRules;
	profiles: ProfileSettings;
	privacy: PrivacySettings;
	hotkeys: Hotkeys;
	sound: SoundSettings;
	obs: ObsSettings;
	session: SessionSettings;
	startup: StartupSettings;
}

/* ------------------------------------------------------------------------ */
/* Status and command results (ARCHITECTURE.md section 12, ipc.rs)           */
/* ------------------------------------------------------------------------ */

export type ObsState = 'off' | 'connecting' | 'connected' | 'auth_failed' | 'error';

export interface Status {
	paused: boolean;
	suppressed: Suppression | null;
	profile: ProfileRef | null;
	solidworks_mode: SolidworksMode;
	keyboard_visible: boolean;
	server: { port: number; running: boolean; error: string | null; clients: number };
	obs: {
		enabled: boolean;
		state: ObsState;
		error: string | null;
		recording: RecordingState | null;
	};
	session: null | {
		id: string;
		active: boolean;
		/** ISO 8601. */
		started: string;
		callouts: number;
		markers: number;
	};
	hook: { installed: boolean; dropped: number };
	version: string;
}

/** `list_monitors` entry, physical pixels. */
export interface Monitor {
	id: string;
	name: string;
	x: number;
	y: number;
	width: number;
	height: number;
	/** DPI scale (1.5 = 150 percent). */
	scale: number;
	primary: boolean;
}

export interface ObsTestResult {
	ok: boolean;
	obs_version?: string;
	ws_version?: string;
	error?: string;
}

export interface ObsPasswordResult {
	stored: boolean;
}

export type ImportSource = { kind: 'text'; text: string } | { kind: 'file'; path: string };
export type ImportFormat = 'copy_list' | 'sldreg' | 'unknown';

/** keystage-core `Change`: a caption an import adds or changes. */
export interface ImportChange {
	keys: string[];
	doc_keys: string;
	command: string;
	/** The sourced caption it replaces (changed entries only). */
	previous: string | null;
	category: string;
}

export interface ImportPreview {
	format: ImportFormat;
	entries: UserShortcut[];
	added: ImportChange[];
	changed: ImportChange[];
	unchanged: number;
	warnings: string[];
	/** Always false until a real export has validated the parser (DECISIONS item 4). */
	validated: boolean;
}

export interface SessionSummary {
	id: string;
	/** ISO 8601. */
	started: string;
	duration_ms: number;
	callouts: number;
	markers: number;
}

export type ExportFormat = 'srt' | 'md' | 'json' | 'chapters';

export interface ExportResult {
	path: string;
}

export interface RouteUrl {
	route: Route;
	url: string;
	width: number;
	height: number;
}

export interface ServerUrls {
	port: number;
	running: boolean;
	error: string | null;
	routes: RouteUrl[];
}

export type FolderKind = 'sessions' | 'config';

/* ------------------------------------------------------------------------ */
/* Profiles (ARCHITECTURE.md section 9, profile.rs)                          */
/* ------------------------------------------------------------------------ */

export interface ProfileSource {
	id: string;
	url: string;
	title: string;
	version: string;
	read: string;
	[extra: string]: unknown;
}

export interface ProfileShortcut {
	id: string;
	keys: string[];
	doc_keys: string;
	command: string;
	description: string;
	group: string;
	context: string;
	versions: string[];
	source_ids: string[];
	quote: string;
	/** "user-import" for entries from the user's own SolidWorks. */
	source?: string | null;
	/** "screenshot" when the quote was transcribed from an image. */
	evidence?: string;
	[extra: string]: unknown;
}

export interface ProfileMouse {
	id: string;
	button: string;
	action: string;
	modifiers: string[];
	direction: string | null;
	command: string;
	description: string;
	context: string;
	versions: string[];
	source_ids: string[];
	quote: string;
	[extra: string]: unknown;
}

export interface ProfileGap {
	topic: string;
	url: string;
	reason: string;
	[extra: string]: unknown;
}

/** `profile_shortcuts` result: the built-in data merged with the user's imports. */
export interface ProfileData {
	schema: string;
	id: string;
	name: string;
	match?: { process: string[] };
	display?: ProfileDisplay;
	data_note?: string;
	read?: string;
	sources: ProfileSource[];
	shortcuts: ProfileShortcut[];
	mouse: ProfileMouse[];
	differences: string[];
	gaps: ProfileGap[];
}

/* ------------------------------------------------------------------------ */
/* Additions this window needs beyond the section 12 table                   */
/* ------------------------------------------------------------------------ */

/** `hotkeys_status` entry: whether Windows accepted each global hotkey. */
export interface HotkeyState {
	name: HotkeyName;
	accelerator: string;
	registered: boolean;
	/** Why registration failed, in plain words; null when registered. */
	error: string | null;
}

/** `platform_info` result. */
export interface PlatformInfo {
	/** Windows build number (19041 is Windows 10 2004); null when unknown. */
	os_build: number | null;
	/** WDA_EXCLUDEFROMCAPTURE needs build 19041 or later (DECISIONS item 7). */
	exclude_from_capture_supported: boolean;
}

export type DialogFilter = { name: string; extensions: string[] };
