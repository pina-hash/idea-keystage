/**
 * Wire protocol v1 (docs/ARCHITECTURE.md section 5).
 *
 * The same JSON travels over the WebSocket (OBS Browser Sources, any browser)
 * and as the payload of the Tauri event "keystage://overlay" (Host A and the
 * settings preview). Field names here are the exact wire names. Every message
 * from the server carries a "t" tag; the guards below accept only messages
 * whose fields have the documented types, and the decoders fill defaults for
 * missing config fields so an older app version never breaks a newer page.
 *
 * Privacy: nothing in this module logs or stores message content.
 */

export const PROTOCOL_VERSION = 1;
export const DEFAULT_PORT = 47820;
export const APP_NAME = 'IDEA Keystage';

/* ------------------------------------------------------------------------ */
/* Enumerations                                                              */
/* ------------------------------------------------------------------------ */

export const ANCHORS = [
	'top-left',
	'top',
	'top-right',
	'left',
	'center',
	'right',
	'bottom-left',
	'bottom',
	'bottom-right',
] as const;
export type Anchor = (typeof ANCHORS)[number];

export const THEME_IDS = [
	'idea',
	'studio',
	'graphite',
	'blueprint',
	'carbon',
	'glass',
	'custom',
] as const;
export type ThemeId = (typeof THEME_IDS)[number];

export const FONTS = ['rajdhani', 'orbitron', 'share-tech-mono'] as const;
export type FontId = (typeof FONTS)[number];

export const CALLOUT_SIZES = ['s', 'm', 'l', 'xl'] as const;
export type CalloutSize = (typeof CALLOUT_SIZES)[number];

export const KEYBOARD_SIZES = ['s', 'm', 'l'] as const;
export type KeyboardSize = (typeof KEYBOARD_SIZES)[number];

export const KEYBOARD_LAYOUTS = ['full', 'tkl', '75', '60', 'laptop', 'left'] as const;
export type KeyboardLayout = (typeof KEYBOARD_LAYOUTS)[number];

export const DIRECTIONS = ['up', 'down'] as const;
export type StackDirection = (typeof DIRECTIONS)[number];

export const CURSOR_STYLES = ['ring', 'spotlight', 'dot'] as const;
export type CursorStyle = (typeof CURSOR_STYLES)[number];

export const CALLOUT_KINDS = ['combo', 'key', 'text', 'chord', 'mouse', 'masked'] as const;
export type CalloutKind = (typeof CALLOUT_KINDS)[number];

export const CHIP_KINDS = ['modifier', 'key', 'mouse'] as const;
export type ChipKind = (typeof CHIP_KINDS)[number];

export const MOUSE_BUTTONS = ['left', 'right', 'middle', 'x1', 'x2'] as const;
export type MouseButton = (typeof MOUSE_BUTTONS)[number];

export const MODIFIERS = ['ctrl', 'shift', 'alt', 'win'] as const;
export type Modifier = (typeof MODIFIERS)[number];

export const SOLIDWORKS_MODES = ['auto', 'on', 'off'] as const;
export type SolidworksMode = (typeof SOLIDWORKS_MODES)[number];

export const SUPPRESSIONS = ['password', 'excluded'] as const;
export type Suppression = (typeof SUPPRESSIONS)[number];

export const ROUTES = ['keys', 'keyboard', 'mouse', 'cursor', 'all'] as const;
export type Route = (typeof ROUTES)[number];

export const CLIENT_HOSTS = ['obs', 'browser', 'native', 'preview'] as const;
export type ClientHost = (typeof CLIENT_HOSTS)[number];

/* ------------------------------------------------------------------------ */
/* Config and state                                                          */
/* ------------------------------------------------------------------------ */

export interface CustomTheme {
	accent: string;
	face: string;
	ink: string;
	glow: string;
}

export interface CalloutsConfig {
	enabled: boolean;
	anchor: Anchor;
	offset_x: number;
	offset_y: number;
	max_stack: number;
	linger_ms: number;
	fade_ms: number;
	size: CalloutSize;
	direction: StackDirection;
	captions: boolean;
	/** Arrows, Enter, Tab and Backspace drawn as glyphs instead of words. */
	symbols: boolean;
}

export interface KeyboardConfig {
	enabled: boolean;
	layout: KeyboardLayout;
	anchor: Anchor;
	offset_x: number;
	offset_y: number;
	decay_ms: number;
	mouse: boolean;
	size: KeyboardSize;
}

export interface MouseConfig {
	enabled: boolean;
	anchor: Anchor;
	offset_x: number;
	offset_y: number;
}

export interface CursorConfig {
	enabled: boolean;
	style: CursorStyle;
	size: number;
	color: string;
	opacity: number;
	ripples: boolean;
	trail: boolean;
	wheel: boolean;
}

export interface IndicatorsConfig {
	paused: boolean;
	suppressed: boolean;
}

export interface OverlayConfig {
	theme: string;
	custom: CustomTheme | null;
	/** 1 = sized for 1080p. */
	scale: number;
	font: FontId;
	callouts: CalloutsConfig;
	keyboard: KeyboardConfig;
	mouse: MouseConfig;
	cursor: CursorConfig;
	indicators: IndicatorsConfig;
}

export interface ProfileRef {
	id: string;
	name: string;
}

export interface RecordingState {
	active: boolean;
	paused: boolean;
	timecode: string;
}

export interface SessionState {
	active: boolean;
	started_ms: number;
	markers: number;
}

export interface AppState {
	paused: boolean;
	/** Never names the excluded app on the wire. */
	suppressed: Suppression | null;
	profile: ProfileRef | null;
	solidworks_mode: SolidworksMode;
	keyboard_visible: boolean;
	/** false: the renderer in Host A (host=native) draws nothing. */
	native_visible: boolean;
	obs_clients: number;
	recording: RecordingState | null;
	session: SessionState | null;
}

/** Display legends for the active Windows layout, keyed by physical id ("sc:1E"). */
export interface Legends {
	layout: string;
	legends: Record<string, string>;
}

/* ------------------------------------------------------------------------ */
/* Callouts                                                                  */
/* ------------------------------------------------------------------------ */

export interface Chip {
	/** Canonical key name (ARCHITECTURE.md section 3) or mouse action name. */
	key: string;
	/** Display label (from the active layout). */
	label: string;
	kind: ChipKind;
}

export interface Callout {
	id: number;
	kind: CalloutKind;
	/** At least one step; a plain combo has one step. */
	steps: Chip[][];
	/** Text runs only. */
	text: string | null;
	/** 1 = no badge. */
	count: number;
	caption: string | null;
	context: string | null;
	profile: string | null;
	t_ms: number;
	updated_ms: number;
}

/* ------------------------------------------------------------------------ */
/* Messages                                                                  */
/* ------------------------------------------------------------------------ */

export interface HelloMsg {
	t: 'hello';
	v: number;
	app: string;
	version: string;
	config: OverlayConfig;
	state: AppState;
	legends: Legends | null;
}
export interface ConfigMsg {
	t: 'config';
	config: OverlayConfig;
}
export interface StateMsg {
	t: 'state';
	state: AppState;
}
export interface LegendsMsg {
	t: 'legends';
	layout: string;
	legends: Record<string, string>;
}
export interface CalloutMsg {
	t: 'callout';
	callout: Callout;
}
export interface KeyMsg {
	t: 'key';
	phys: string;
	key: string;
	down: boolean;
	modifier: Modifier | null;
	t_ms: number;
}
export interface ButtonMsg {
	t: 'button';
	button: MouseButton;
	down: boolean;
	x: number;
	y: number;
	t_ms: number;
}
export interface WheelMsg {
	t: 'wheel';
	dx: number;
	dy: number;
	x: number;
	y: number;
	t_ms: number;
}
export interface MoveMsg {
	t: 'move';
	x: number;
	y: number;
	t_ms: number;
}
export interface ReleaseAllMsg {
	t: 'release_all';
	t_ms: number;
}
export interface ClearMsg {
	t: 'clear';
}
export interface PongMsg {
	t: 'pong';
}

export type ServerMsg =
	| HelloMsg
	| ConfigMsg
	| StateMsg
	| LegendsMsg
	| CalloutMsg
	| KeyMsg
	| ButtonMsg
	| WheelMsg
	| MoveMsg
	| ReleaseAllMsg
	| ClearMsg
	| PongMsg;

export type ServerMsgType = ServerMsg['t'];

export interface ClientHelloMsg {
	t: 'hello';
	route: Route;
	host: ClientHost;
}
export interface PingMsg {
	t: 'ping';
}
export type ClientMsg = ClientHelloMsg | PingMsg;

/* ------------------------------------------------------------------------ */
/* Defaults                                                                  */
/* ------------------------------------------------------------------------ */

/**
 * Overlay defaults. keystage-core `settings.rs` must declare the same values
 * for `OverlayConfig::default()` (requested in .work/notes/renderer.md).
 */
export function defaultOverlayConfig(): OverlayConfig {
	return {
		theme: 'idea',
		custom: null,
		scale: 1,
		font: 'rajdhani',
		callouts: {
			enabled: true,
			anchor: 'bottom',
			offset_x: 0,
			offset_y: 96,
			max_stack: 4,
			linger_ms: 2200,
			fade_ms: 450,
			size: 'm',
			direction: 'up',
			captions: true,
			symbols: true,
		},
		keyboard: {
			enabled: false,
			layout: 'full',
			anchor: 'bottom-left',
			offset_x: 32,
			offset_y: 32,
			decay_ms: 700,
			mouse: true,
			size: 'm',
		},
		mouse: {
			enabled: false,
			anchor: 'bottom-right',
			offset_x: 32,
			offset_y: 32,
		},
		cursor: {
			enabled: false,
			style: 'ring',
			size: 44,
			color: '#78b870',
			opacity: 0.85,
			ripples: true,
			trail: false,
			wheel: true,
		},
		indicators: { paused: true, suppressed: true },
	};
}

export function defaultAppState(): AppState {
	return {
		paused: false,
		suppressed: null,
		profile: null,
		solidworks_mode: 'auto',
		keyboard_visible: true,
		native_visible: true,
		obs_clients: 0,
		recording: null,
		session: null,
	};
}

/** Limits shared with the settings window and the URL override parser. */
export const LIMITS = {
	scale: { min: 0.25, max: 4 },
	maxStack: { min: 1, max: 12 },
	lingerMs: { min: 300, max: 15000 },
	fadeMs: { min: 0, max: 3000 },
	decayMs: { min: 0, max: 5000 },
	offset: { min: -4000, max: 4000 },
	cursorSize: { min: 8, max: 240 },
} as const;

/* ------------------------------------------------------------------------ */
/* Primitive guards                                                          */
/* ------------------------------------------------------------------------ */

type Rec = Record<string, unknown>;

export function isRecord(v: unknown): v is Rec {
	return typeof v === 'object' && v !== null && !Array.isArray(v);
}
function isNum(v: unknown): v is number {
	return typeof v === 'number' && Number.isFinite(v);
}
function isBool(v: unknown): v is boolean {
	return typeof v === 'boolean';
}
function isStr(v: unknown): v is string {
	return typeof v === 'string';
}
function isStrOrNull(v: unknown): v is string | null {
	return v === null || typeof v === 'string';
}
function oneOf<T extends string>(list: readonly T[], v: unknown): v is T {
	return typeof v === 'string' && (list as readonly string[]).includes(v);
}

export function isAnchor(v: unknown): v is Anchor {
	return oneOf(ANCHORS, v);
}
export function isRoute(v: unknown): v is Route {
	return oneOf(ROUTES, v);
}
export function isKeyboardLayout(v: unknown): v is KeyboardLayout {
	return oneOf(KEYBOARD_LAYOUTS, v);
}
export function isCalloutSize(v: unknown): v is CalloutSize {
	return oneOf(CALLOUT_SIZES, v);
}
export function isKeyboardSize(v: unknown): v is KeyboardSize {
	return oneOf(KEYBOARD_SIZES, v);
}

/* ------------------------------------------------------------------------ */
/* Structure guards                                                          */
/* ------------------------------------------------------------------------ */

export function isChip(v: unknown): v is Chip {
	return isRecord(v) && isStr(v.key) && isStr(v.label) && oneOf(CHIP_KINDS, v.kind);
}

export function isCallout(v: unknown): v is Callout {
	if (!isRecord(v)) return false;
	if (!isNum(v.id) || !oneOf(CALLOUT_KINDS, v.kind)) return false;
	if (!Array.isArray(v.steps)) return false;
	for (const step of v.steps) {
		if (!Array.isArray(step) || !step.every(isChip)) return false;
	}
	return (
		isStrOrNull(v.text) &&
		isNum(v.count) &&
		isStrOrNull(v.caption) &&
		isStrOrNull(v.context) &&
		isStrOrNull(v.profile) &&
		isNum(v.t_ms) &&
		isNum(v.updated_ms)
	);
}

export function isCustomTheme(v: unknown): v is CustomTheme {
	return isRecord(v) && isStr(v.accent) && isStr(v.face) && isStr(v.ink) && isStr(v.glow);
}

export function isOverlayConfig(v: unknown): v is OverlayConfig {
	if (!isRecord(v)) return false;
	const c = v.callouts;
	const k = v.keyboard;
	const m = v.mouse;
	const cu = v.cursor;
	const ind = v.indicators;
	return (
		isStr(v.theme) &&
		(v.custom === null || isCustomTheme(v.custom)) &&
		isNum(v.scale) &&
		oneOf(FONTS, v.font) &&
		isRecord(c) &&
		isBool(c.enabled) &&
		isAnchor(c.anchor) &&
		isNum(c.offset_x) &&
		isNum(c.offset_y) &&
		isNum(c.max_stack) &&
		isNum(c.linger_ms) &&
		isNum(c.fade_ms) &&
		isCalloutSize(c.size) &&
		oneOf(DIRECTIONS, c.direction) &&
		isBool(c.captions) &&
		isBool(c.symbols) &&
		isRecord(k) &&
		isBool(k.enabled) &&
		isKeyboardLayout(k.layout) &&
		isAnchor(k.anchor) &&
		isNum(k.offset_x) &&
		isNum(k.offset_y) &&
		isNum(k.decay_ms) &&
		isBool(k.mouse) &&
		isKeyboardSize(k.size) &&
		isRecord(m) &&
		isBool(m.enabled) &&
		isAnchor(m.anchor) &&
		isNum(m.offset_x) &&
		isNum(m.offset_y) &&
		isRecord(cu) &&
		isBool(cu.enabled) &&
		oneOf(CURSOR_STYLES, cu.style) &&
		isNum(cu.size) &&
		isStr(cu.color) &&
		isNum(cu.opacity) &&
		isBool(cu.ripples) &&
		isBool(cu.trail) &&
		isBool(cu.wheel) &&
		isRecord(ind) &&
		isBool(ind.paused) &&
		isBool(ind.suppressed)
	);
}

function isProfileRef(v: unknown): v is ProfileRef {
	return isRecord(v) && isStr(v.id) && isStr(v.name);
}
function isRecording(v: unknown): v is RecordingState {
	return isRecord(v) && isBool(v.active) && isBool(v.paused) && isStr(v.timecode);
}
function isSession(v: unknown): v is SessionState {
	return isRecord(v) && isBool(v.active) && isNum(v.started_ms) && isNum(v.markers);
}

export function isAppState(v: unknown): v is AppState {
	return (
		isRecord(v) &&
		isBool(v.paused) &&
		(v.suppressed === null || oneOf(SUPPRESSIONS, v.suppressed)) &&
		(v.profile === null || isProfileRef(v.profile)) &&
		oneOf(SOLIDWORKS_MODES, v.solidworks_mode) &&
		isBool(v.keyboard_visible) &&
		isBool(v.native_visible) &&
		isNum(v.obs_clients) &&
		(v.recording === null || isRecording(v.recording)) &&
		(v.session === null || isSession(v.session))
	);
}

function isLegendMap(v: unknown): v is Record<string, string> {
	if (!isRecord(v)) return false;
	for (const value of Object.values(v)) if (!isStr(value)) return false;
	return true;
}

export function isLegends(v: unknown): v is Legends {
	return isRecord(v) && isStr(v.layout) && isLegendMap(v.legends);
}

/**
 * Strict guard for one server message. Config and state are accepted only
 * when complete; use `decodeServerMsg` to read a message whose config may
 * come from an older app version with missing fields.
 */
export function isServerMsg(v: unknown): v is ServerMsg {
	if (!isRecord(v) || !isStr(v.t)) return false;
	switch (v.t) {
		case 'hello':
			return (
				isNum(v.v) &&
				isStr(v.app) &&
				isStr(v.version) &&
				isOverlayConfig(v.config) &&
				isAppState(v.state) &&
				(v.legends === null || isLegends(v.legends))
			);
		case 'config':
			return isOverlayConfig(v.config);
		case 'state':
			return isAppState(v.state);
		case 'legends':
			return isStr(v.layout) && isLegendMap(v.legends);
		case 'callout':
			return isCallout(v.callout);
		case 'key':
			return (
				isStr(v.phys) &&
				isStr(v.key) &&
				isBool(v.down) &&
				(v.modifier === null || oneOf(MODIFIERS, v.modifier)) &&
				isNum(v.t_ms)
			);
		case 'button':
			return (
				oneOf(MOUSE_BUTTONS, v.button) &&
				isBool(v.down) &&
				isNum(v.x) &&
				isNum(v.y) &&
				isNum(v.t_ms)
			);
		case 'wheel':
			return isNum(v.dx) && isNum(v.dy) && isNum(v.x) && isNum(v.y) && isNum(v.t_ms);
		case 'move':
			return isNum(v.x) && isNum(v.y) && isNum(v.t_ms);
		case 'release_all':
			return isNum(v.t_ms);
		case 'clear':
		case 'pong':
			return true;
		default:
			return false;
	}
}

export function isClientMsg(v: unknown): v is ClientMsg {
	if (!isRecord(v)) return false;
	if (v.t === 'ping') return true;
	return v.t === 'hello' && isRoute(v.route) && oneOf(CLIENT_HOSTS, v.host);
}

/* ------------------------------------------------------------------------ */
/* Tolerant decoding                                                         */
/* ------------------------------------------------------------------------ */

function clamp(n: number, lo: number, hi: number): number {
	return Math.min(hi, Math.max(lo, n));
}

function pickNum(v: unknown, fallback: number, lo: number, hi: number): number {
	return isNum(v) ? clamp(v, lo, hi) : fallback;
}
function pickBool(v: unknown, fallback: boolean): boolean {
	return isBool(v) ? v : fallback;
}
function pickOne<T extends string>(list: readonly T[], v: unknown, fallback: T): T {
	return oneOf(list, v) ? v : fallback;
}

/**
 * Reads an OverlayConfig that may be partial (an older or newer app) and
 * returns a complete one: every missing or mistyped field takes its default,
 * numbers are clamped to LIMITS. Returns null when `v` is not an object.
 */
export function normalizeOverlayConfig(v: unknown): OverlayConfig | null {
	if (!isRecord(v)) return null;
	const d = defaultOverlayConfig();
	const c = isRecord(v.callouts) ? v.callouts : {};
	const k = isRecord(v.keyboard) ? v.keyboard : {};
	const m = isRecord(v.mouse) ? v.mouse : {};
	const cu = isRecord(v.cursor) ? v.cursor : {};
	const ind = isRecord(v.indicators) ? v.indicators : {};
	const L = LIMITS;
	return {
		theme: isStr(v.theme) && v.theme.length > 0 ? v.theme : d.theme,
		custom: isCustomTheme(v.custom) ? { ...v.custom } : null,
		scale: pickNum(v.scale, d.scale, L.scale.min, L.scale.max),
		font: pickOne(FONTS, v.font, d.font),
		callouts: {
			enabled: pickBool(c.enabled, d.callouts.enabled),
			anchor: pickOne(ANCHORS, c.anchor, d.callouts.anchor),
			offset_x: pickNum(c.offset_x, d.callouts.offset_x, L.offset.min, L.offset.max),
			offset_y: pickNum(c.offset_y, d.callouts.offset_y, L.offset.min, L.offset.max),
			max_stack: Math.round(
				pickNum(c.max_stack, d.callouts.max_stack, L.maxStack.min, L.maxStack.max)
			),
			linger_ms: pickNum(c.linger_ms, d.callouts.linger_ms, L.lingerMs.min, L.lingerMs.max),
			fade_ms: pickNum(c.fade_ms, d.callouts.fade_ms, L.fadeMs.min, L.fadeMs.max),
			size: pickOne(CALLOUT_SIZES, c.size, d.callouts.size),
			direction: pickOne(DIRECTIONS, c.direction, d.callouts.direction),
			captions: pickBool(c.captions, d.callouts.captions),
			symbols: pickBool(c.symbols, d.callouts.symbols),
		},
		keyboard: {
			enabled: pickBool(k.enabled, d.keyboard.enabled),
			layout: pickOne(KEYBOARD_LAYOUTS, k.layout, d.keyboard.layout),
			anchor: pickOne(ANCHORS, k.anchor, d.keyboard.anchor),
			offset_x: pickNum(k.offset_x, d.keyboard.offset_x, L.offset.min, L.offset.max),
			offset_y: pickNum(k.offset_y, d.keyboard.offset_y, L.offset.min, L.offset.max),
			decay_ms: pickNum(k.decay_ms, d.keyboard.decay_ms, L.decayMs.min, L.decayMs.max),
			mouse: pickBool(k.mouse, d.keyboard.mouse),
			size: pickOne(KEYBOARD_SIZES, k.size, d.keyboard.size),
		},
		mouse: {
			enabled: pickBool(m.enabled, d.mouse.enabled),
			anchor: pickOne(ANCHORS, m.anchor, d.mouse.anchor),
			offset_x: pickNum(m.offset_x, d.mouse.offset_x, L.offset.min, L.offset.max),
			offset_y: pickNum(m.offset_y, d.mouse.offset_y, L.offset.min, L.offset.max),
		},
		cursor: {
			enabled: pickBool(cu.enabled, d.cursor.enabled),
			style: pickOne(CURSOR_STYLES, cu.style, d.cursor.style),
			size: pickNum(cu.size, d.cursor.size, L.cursorSize.min, L.cursorSize.max),
			color: isStr(cu.color) && cu.color.length > 0 ? cu.color : d.cursor.color,
			opacity: pickNum(cu.opacity, d.cursor.opacity, 0, 1),
			ripples: pickBool(cu.ripples, d.cursor.ripples),
			trail: pickBool(cu.trail, d.cursor.trail),
			wheel: pickBool(cu.wheel, d.cursor.wheel),
		},
		indicators: {
			paused: pickBool(ind.paused, d.indicators.paused),
			suppressed: pickBool(ind.suppressed, d.indicators.suppressed),
		},
	};
}

/** Same idea as normalizeOverlayConfig, for AppState. */
export function normalizeAppState(v: unknown): AppState | null {
	if (!isRecord(v)) return null;
	const d = defaultAppState();
	return {
		paused: pickBool(v.paused, d.paused),
		suppressed: oneOf(SUPPRESSIONS, v.suppressed) ? v.suppressed : null,
		profile: isProfileRef(v.profile) ? { id: v.profile.id, name: v.profile.name } : null,
		solidworks_mode: pickOne(SOLIDWORKS_MODES, v.solidworks_mode, d.solidworks_mode),
		keyboard_visible: pickBool(v.keyboard_visible, d.keyboard_visible),
		native_visible: pickBool(v.native_visible, d.native_visible),
		obs_clients: isNum(v.obs_clients) ? Math.max(0, Math.round(v.obs_clients)) : 0,
		recording: isRecording(v.recording) ? { ...v.recording } : null,
		session: isSession(v.session) ? { ...v.session } : null,
	};
}

/**
 * Decodes one message from the wire. Accepts a JSON string or an already
 * parsed value (Tauri event payloads arrive parsed). Config and state are
 * normalized (missing fields take defaults); every other message must pass
 * the strict guard. Returns null for anything malformed or unknown, which
 * the caller ignores silently.
 */
export function decodeServerMsg(raw: unknown): ServerMsg | null {
	let v: unknown = raw;
	if (typeof raw === 'string') {
		try {
			v = JSON.parse(raw);
		} catch {
			return null;
		}
	}
	if (!isRecord(v) || !isStr(v.t)) return null;
	switch (v.t) {
		case 'hello': {
			const config = normalizeOverlayConfig(v.config);
			const state = normalizeAppState(v.state);
			if (!config || !state) return null;
			return {
				t: 'hello',
				v: isNum(v.v) ? v.v : PROTOCOL_VERSION,
				app: isStr(v.app) ? v.app : APP_NAME,
				version: isStr(v.version) ? v.version : '',
				config,
				state,
				legends: isLegends(v.legends) ? v.legends : null,
			};
		}
		case 'config': {
			const config = normalizeOverlayConfig(v.config);
			return config ? { t: 'config', config } : null;
		}
		case 'state': {
			const state = normalizeAppState(v.state);
			return state ? { t: 'state', state } : null;
		}
		default:
			return isServerMsg(v) ? v : null;
	}
}

export function encodeClientMsg(msg: ClientMsg): string {
	return JSON.stringify(msg);
}
