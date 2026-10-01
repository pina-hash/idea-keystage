/**
 * Defaults and normalizers for the settings window.
 *
 * The defaults mirror keystage-core (`Settings::default()` in settings.rs,
 * `DisplayRules::default()` in rules.rs, `synth::PACKS`); defaults.test.ts
 * reads those Rust files and fails if the two drift apart. The normalizers
 * fill anything missing from an IPC reply with these defaults, so an older
 * app version never leaves a control without a value.
 */
import {
	DEFAULT_PORT,
	SOLIDWORKS_MODES,
	SUPPRESSIONS,
	defaultOverlayConfig,
	isRecord,
	normalizeOverlayConfig,
	type SolidworksMode,
} from '../lib/protocol';
import {
	HOTKEY_NAMES,
	type DisplayRules,
	type ExportResult,
	type Hotkeys,
	type ObsState,
	type ProfileDisplay,
	type Settings,
	type Status,
	type TypingMode,
	type UserShortcut,
} from './types';

/** keystage-core `DEFAULT_EXCLUDED_APPS`: a starting list, not sourced data. */
export const DEFAULT_EXCLUDED_APPS: readonly string[] = [
	'1Password.exe',
	'Bitwarden.exe',
	'KeePass.exe',
	'KeePassXC.exe',
	'Enpass.exe',
	'NordPass.exe',
	'Dashlane.exe',
	'RoboForm.exe',
	'keeperpasswordmanager.exe',
	'CredentialUIBroker.exe',
	'consent.exe',
	'LogonUI.exe',
];

/** ARCHITECTURE.md section 7 default hotkeys. */
export const DEFAULT_HOTKEYS: Readonly<Hotkeys> = {
	pause: 'Ctrl+Alt+Shift+P',
	toggle_keyboard: 'Ctrl+Alt+Shift+K',
	toggle_solidworks: 'Ctrl+Alt+Shift+S',
	chapter_marker: 'Ctrl+Alt+Shift+M',
	session_toggle: 'Ctrl+Alt+Shift+L',
};

export const DEFAULT_OBS_PORT = 4455;
export const DEFAULT_OBS_HOST = '127.0.0.1';

/** keystage-core `synth::PACKS`, in the order settings lists them. */
export interface SoundPack {
	id: string;
	name: string;
	description: string;
}
export const SOUND_PACKS: readonly SoundPack[] = [
	{ id: 'soft-linear', name: 'Soft linear', description: 'Smooth, rounded and quiet.' },
	{ id: 'tactile', name: 'Tactile', description: 'A small bump before each bottom out.' },
	{ id: 'clicky', name: 'Clicky', description: 'Crisp double click on every press.' },
	{ id: 'deep', name: 'Deep', description: 'Low, full and heavy.' },
	{ id: 'typewriter', name: 'Typewriter', description: 'Metal strike over a wooden thud.' },
	{ id: 'bubble', name: 'Bubble', description: 'Soft pitched pops.' },
	{ id: 'ceramic', name: 'Ceramic', description: 'Bright, glassy ring.' },
	{ id: 'muted', name: 'Muted', description: 'Barely there, for quiet rooms.' },
	{ id: 'laptop', name: 'Laptop', description: 'Short, tight scissor switches.' },
	{ id: 'studio', name: 'Studio', description: 'Balanced and warm, sits under a voice.' },
	{ id: 'hollow', name: 'Hollow', description: 'Resonant case with a long tail.' },
	{ id: 'pebble', name: 'Pebble', description: 'Dry stone clack.' },
];
export const DEFAULT_SOUND_PACK = 'soft-linear';

/** keystage-core `DisplayRules::validate` ranges, for the controls that edit them. */
export const RULE_LIMITS = {
	repeat_window_ms: { min: 0, max: 10_000 },
	chord_window_ms: { min: 0, max: 5_000 },
	text_run_gap_ms: { min: 100, max: 10_000 },
	double_click_ms: { min: 100, max: 2_000 },
	gesture_max_ms: { min: 100, max: 10_000 },
	chord_max_steps: { min: 2, max: 8 },
	drag_threshold_px: { min: 1, max: 200 },
	double_click_px: { min: 1, max: 200 },
	gesture_min_px: { min: 10, max: 2_000 },
} as const;

export function defaultDisplayRules(): DisplayRules {
	return {
		typing: 'shortcuts_only',
		typed_values: true,
		show_modifier_side: false,
		repeat_window_ms: 1000,
		chord_window_ms: 900,
		chord_max_steps: 4,
		text_run_gap_ms: 1500,
		mouse_callouts: false,
		plain_left_clicks: false,
		show_injected: true,
		hidden_combos: [],
		drag_threshold_px: 4,
		double_click_ms: 500,
		double_click_px: 4,
		gesture_min_px: 40,
		gesture_max_ms: 1500,
		modifier_verify_ms: 2000,
		modifier_stale_ms: 10_000,
	};
}

export function defaultSettings(): Settings {
	return {
		version: 1,
		first_run_done: false,
		host: {
			native_overlay: true,
			hide_native_when_obs: true,
			monitor: null,
			exclude_from_capture: false,
		},
		server: { port: DEFAULT_PORT },
		overlay: defaultOverlayConfig(),
		display: defaultDisplayRules(),
		profiles: {
			solidworks_mode: 'auto',
			user: [],
			solidworks_user_shortcuts: [],
			solidworks_display: {},
		},
		privacy: { excluded_apps: [...DEFAULT_EXCLUDED_APPS] },
		hotkeys: { ...DEFAULT_HOTKEYS },
		sound: { enabled: false, pack: DEFAULT_SOUND_PACK, volume: 0.6 },
		obs: {
			enabled: false,
			host: DEFAULT_OBS_HOST,
			port: DEFAULT_OBS_PORT,
			password_dpapi: null,
			auto_session: true,
		},
		session: { folder: null, autosave: true },
		startup: { start_with_windows: false, show_settings: true },
	};
}

/* ------------------------------------------------------------------------ */
/* Normalizers                                                               */
/* ------------------------------------------------------------------------ */

type Rec = Record<string, unknown>;

function rec(v: unknown): Rec {
	return isRecord(v) ? v : {};
}
function bool(v: unknown, d: boolean): boolean {
	return typeof v === 'boolean' ? v : d;
}
function num(v: unknown, d: number, min = -Infinity, max = Infinity): number {
	if (typeof v !== 'number' || !Number.isFinite(v)) return d;
	return Math.min(max, Math.max(min, v));
}
function int(v: unknown, d: number, min = -Infinity, max = Infinity): number {
	return Math.round(num(v, d, min, max));
}
function str(v: unknown, d: string): string {
	return typeof v === 'string' ? v : d;
}
function strOrNull(v: unknown): string | null {
	return typeof v === 'string' && v.trim().length > 0 ? v : null;
}
function oneOf<T extends string>(list: readonly T[], v: unknown, d: T): T {
	return typeof v === 'string' && (list as readonly string[]).includes(v) ? (v as T) : d;
}
function strings(v: unknown, d: readonly string[]): string[] {
	return Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string') : [...d];
}

const TYPING_MODES: readonly TypingMode[] = ['shortcuts_only', 'all'];
const OBS_STATES: readonly ObsState[] = ['off', 'connecting', 'connected', 'auth_failed', 'error'];

export function normalizeDisplayRules(v: unknown): DisplayRules {
	const d = defaultDisplayRules();
	const r = rec(v);
	const L = RULE_LIMITS;
	return {
		typing: oneOf(TYPING_MODES, r.typing, d.typing),
		typed_values: bool(r.typed_values, d.typed_values),
		show_modifier_side: bool(r.show_modifier_side, d.show_modifier_side),
		repeat_window_ms: int(
			r.repeat_window_ms,
			d.repeat_window_ms,
			L.repeat_window_ms.min,
			L.repeat_window_ms.max
		),
		chord_window_ms: int(
			r.chord_window_ms,
			d.chord_window_ms,
			L.chord_window_ms.min,
			L.chord_window_ms.max
		),
		chord_max_steps: int(
			r.chord_max_steps,
			d.chord_max_steps,
			L.chord_max_steps.min,
			L.chord_max_steps.max
		),
		text_run_gap_ms: int(
			r.text_run_gap_ms,
			d.text_run_gap_ms,
			L.text_run_gap_ms.min,
			L.text_run_gap_ms.max
		),
		mouse_callouts: bool(r.mouse_callouts, d.mouse_callouts),
		plain_left_clicks: bool(r.plain_left_clicks, d.plain_left_clicks),
		show_injected: bool(r.show_injected, d.show_injected),
		hidden_combos: strings(r.hidden_combos, d.hidden_combos),
		drag_threshold_px: int(
			r.drag_threshold_px,
			d.drag_threshold_px,
			L.drag_threshold_px.min,
			L.drag_threshold_px.max
		),
		double_click_ms: int(
			r.double_click_ms,
			d.double_click_ms,
			L.double_click_ms.min,
			L.double_click_ms.max
		),
		double_click_px: int(
			r.double_click_px,
			d.double_click_px,
			L.double_click_px.min,
			L.double_click_px.max
		),
		gesture_min_px: int(
			r.gesture_min_px,
			d.gesture_min_px,
			L.gesture_min_px.min,
			L.gesture_min_px.max
		),
		gesture_max_ms: int(
			r.gesture_max_ms,
			d.gesture_max_ms,
			L.gesture_max_ms.min,
			L.gesture_max_ms.max
		),
		modifier_verify_ms: int(r.modifier_verify_ms, d.modifier_verify_ms, 250, 60_000),
		modifier_stale_ms: int(r.modifier_stale_ms, d.modifier_stale_ms, 1000, 600_000),
	};
}

function normalizeProfileDisplay(v: unknown): ProfileDisplay {
	const r = rec(v);
	const out: ProfileDisplay = {};
	if (typeof r.mode === 'string' && (TYPING_MODES as readonly string[]).includes(r.mode)) {
		out.mode = r.mode as TypingMode;
	}
	if (typeof r.typed_values === 'boolean') out.typed_values = r.typed_values;
	if (typeof r.mouse_captions === 'boolean') out.mouse_captions = r.mouse_captions;
	return out;
}

function normalizeUserShortcut(v: unknown): UserShortcut | null {
	const r = rec(v);
	const keys = strings(r.keys, []);
	const command = str(r.command, '');
	if (keys.length === 0 || command.trim().length === 0) return null;
	return {
		keys,
		doc_keys: str(r.doc_keys, keys.join('+')),
		command,
		category: str(r.category, ''),
		source: 'user-import',
	};
}

function normalizeHotkeys(v: unknown): Hotkeys {
	const r = rec(v);
	const out = { ...DEFAULT_HOTKEYS };
	for (const name of HOTKEY_NAMES) {
		const value = r[name];
		if (typeof value === 'string' && value.trim().length > 0) out[name] = value.trim();
	}
	return out;
}

/**
 * Fills every missing or unreadable field of a settings object from the
 * defaults and clamps numbers the way keystage-core does. Returns the
 * defaults when `v` is not an object.
 */
export function normalizeSettings(v: unknown): Settings {
	const d = defaultSettings();
	const r = rec(v);
	const host = rec(r.host);
	const server = rec(r.server);
	const profiles = rec(r.profiles);
	const privacy = rec(r.privacy);
	const sound = rec(r.sound);
	const obs = rec(r.obs);
	const session = rec(r.session);
	const startup = rec(r.startup);
	const shortcuts = Array.isArray(profiles.solidworks_user_shortcuts)
		? profiles.solidworks_user_shortcuts
				.map(normalizeUserShortcut)
				.filter((s): s is UserShortcut => s !== null)
		: [];
	const pack = str(sound.pack, d.sound.pack);
	return {
		version: int(r.version, d.version),
		first_run_done: bool(r.first_run_done, d.first_run_done),
		host: {
			native_overlay: bool(host.native_overlay, d.host.native_overlay),
			hide_native_when_obs: bool(host.hide_native_when_obs, d.host.hide_native_when_obs),
			monitor: strOrNull(host.monitor),
			exclude_from_capture: bool(host.exclude_from_capture, d.host.exclude_from_capture),
		},
		server: { port: int(server.port, d.server.port, 1, 65535) },
		overlay: normalizeOverlayConfig(r.overlay) ?? d.overlay,
		display: normalizeDisplayRules(r.display),
		profiles: {
			solidworks_mode: oneOf<SolidworksMode>(
				SOLIDWORKS_MODES,
				profiles.solidworks_mode,
				d.profiles.solidworks_mode
			),
			user: Array.isArray(profiles.user) ? [...profiles.user] : [],
			solidworks_user_shortcuts: shortcuts,
			solidworks_display: normalizeProfileDisplay(profiles.solidworks_display),
		},
		privacy: { excluded_apps: strings(privacy.excluded_apps, d.privacy.excluded_apps) },
		hotkeys: normalizeHotkeys(r.hotkeys),
		sound: {
			enabled: bool(sound.enabled, d.sound.enabled),
			pack: SOUND_PACKS.some((p) => p.id === pack) ? pack : d.sound.pack,
			volume: num(sound.volume, d.sound.volume, 0, 1),
		},
		obs: {
			enabled: bool(obs.enabled, d.obs.enabled),
			host: str(obs.host, d.obs.host).trim() || d.obs.host,
			port: int(obs.port, d.obs.port, 1, 65535),
			password_dpapi: strOrNull(obs.password_dpapi),
			auto_session: bool(obs.auto_session, d.obs.auto_session),
		},
		session: {
			folder: strOrNull(session.folder),
			autosave: bool(session.autosave, d.session.autosave),
		},
		startup: {
			start_with_windows: bool(startup.start_with_windows, d.startup.start_with_windows),
			show_settings: bool(startup.show_settings, true),
		},
	};
}

export function defaultStatus(): Status {
	return {
		paused: false,
		suppressed: null,
		profile: null,
		solidworks_mode: 'auto',
		keyboard_visible: true,
		server: { port: DEFAULT_PORT, running: false, error: null, clients: 0 },
		obs: { enabled: false, state: 'off', error: null, recording: null },
		session: null,
		hook: { installed: false, dropped: 0 },
		version: '',
		notices: [],
	};
}

/** Non-empty, distinct sentences from a reply's list (anything else is dropped). */
function sentences(v: unknown): string[] {
	if (!Array.isArray(v)) return [];
	const out: string[] = [];
	for (const x of v) {
		if (typeof x !== 'string') continue;
		const t = x.trim();
		if (t.length > 0 && !out.includes(t)) out.push(t);
	}
	return out;
}

/** Fills a Status reply the same way (tolerant of missing fields). */
export function normalizeStatus(v: unknown): Status {
	const d = defaultStatus();
	const r = rec(v);
	const server = rec(r.server);
	const obs = rec(r.obs);
	const hook = rec(r.hook);
	const profile = isRecord(r.profile) ? r.profile : null;
	const recording = isRecord(obs.recording) ? obs.recording : null;
	const session = isRecord(r.session) ? r.session : null;
	return {
		paused: bool(r.paused, d.paused),
		suppressed:
			typeof r.suppressed === 'string' && (SUPPRESSIONS as readonly string[]).includes(r.suppressed)
				? (r.suppressed as Status['suppressed'])
				: null,
		profile:
			profile && typeof profile.id === 'string'
				? { id: profile.id, name: str(profile.name, profile.id) }
				: null,
		solidworks_mode: oneOf<SolidworksMode>(SOLIDWORKS_MODES, r.solidworks_mode, d.solidworks_mode),
		keyboard_visible: bool(r.keyboard_visible, d.keyboard_visible),
		server: {
			port: int(server.port, d.server.port, 0, 65535),
			running: bool(server.running, d.server.running),
			error: strOrNull(server.error),
			clients: int(server.clients, 0, 0),
		},
		obs: {
			enabled: bool(obs.enabled, d.obs.enabled),
			state: oneOf(OBS_STATES, obs.state, d.obs.state),
			error: strOrNull(obs.error),
			recording: recording
				? {
						active: bool(recording.active, false),
						paused: bool(recording.paused, false),
						timecode: str(recording.timecode, ''),
					}
				: null,
		},
		session: session
			? {
					id: str(session.id, ''),
					active: bool(session.active, false),
					started: str(session.started, ''),
					callouts: int(session.callouts, 0, 0),
					markers: int(session.markers, 0, 0),
				}
			: null,
		hook: {
			installed: bool(hook.installed, d.hook.installed),
			dropped: int(hook.dropped, 0, 0),
		},
		version: str(r.version, d.version),
		notices: sentences(r.notices),
	};
}

/** Fills a `session_export` reply: the path written, and its warnings. */
export function normalizeExportResult(v: unknown, requestedPath: string): ExportResult {
	const r = rec(v);
	return {
		path: typeof r.path === 'string' && r.path.trim() ? r.path : requestedPath,
		warnings: sentences(r.warnings),
	};
}

/** A deep copy through JSON (settings hold only JSON values). */
export function clone<T>(v: T): T {
	return JSON.parse(JSON.stringify(v)) as T;
}

/** Structural equality through JSON with sorted keys. */
export function sameJson(a: unknown, b: unknown): boolean {
	return stableStringify(a) === stableStringify(b);
}

export function stableStringify(v: unknown): string {
	return JSON.stringify(v, (_key, value: unknown) => {
		if (isRecord(value)) {
			const out: Rec = {};
			for (const k of Object.keys(value).sort()) {
				if (value[k] !== undefined) out[k] = value[k];
			}
			return out;
		}
		return value;
	});
}
