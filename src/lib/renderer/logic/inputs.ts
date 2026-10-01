/**
 * Key and mouse state for the keyboard visual, the mouse diagram and the
 * cursor effects, as pure logic with an injected clock.
 *
 * Times named `at` are the renderer's local clock (ms, the same clock the
 * caller passes in). `t_ms` from the wire is the app's monotonic clock and is
 * kept only for reference: the two clocks are never compared.
 */
import type { ButtonMsg, KeyMsg, Modifier, MouseButton, MoveMsg, WheelMsg } from '../../protocol';
import { MOUSE_BUTTONS } from '../../protocol';

/** One physical key the keyboard visual may light. */
export interface KeyState {
	/** Physical id, "sc:1E" or "sc:E01D". */
	phys: string;
	/** Canonical key name. */
	key: string;
	modifier: Modifier | null;
	down: boolean;
	/** Local time of the last key-down. */
	pressedAt: number;
	/** Local time of the key-up; null while held. */
	releasedAt: number | null;
}

export type KeyStates = Readonly<Record<string, KeyState>>;

/** Per-button pressed state with timestamps for decay. */
export interface ButtonState {
	down: boolean;
	pressedAt: number | null;
	releasedAt: number | null;
}

export type MouseButtons = Readonly<Record<MouseButton, ButtonState>>;

export type WheelDirection = 'up' | 'down' | 'left' | 'right';

/** One wheel notch (or a coalesced group), for the diagram and the cursor. */
export interface WheelPulse {
	/** Monotonic sequence number; a new pulse always has a larger one. */
	seq: number;
	direction: WheelDirection;
	/** Notches in this pulse (WHEEL_DELTA 120 = 1). */
	notches: number;
	x: number;
	y: number;
	at: number;
}

/** A button press for click ripples. */
export interface ClickEvent {
	seq: number;
	button: MouseButton;
	x: number;
	y: number;
	at: number;
}

export interface PointerState {
	/** Normalized to the chosen monitor (0..1, may fall outside). */
	x: number;
	y: number;
	/** false until the first position arrives. */
	known: boolean;
	/** Local time of the last position update. */
	movedAt: number;
}

export interface MouseState {
	buttons: MouseButtons;
	pointer: PointerState;
	/** The latest wheel pulse, or null before the first one. */
	wheel: WheelPulse | null;
	/** Recent wheel pulses, oldest first, at most RECENT_LIMIT. */
	wheels: readonly WheelPulse[];
	/** Recent button presses, oldest first, at most RECENT_LIMIT. */
	clicks: readonly ClickEvent[];
}

export const RECENT_LIMIT = 24;
/** Recent wheel pulses and clicks older than this are dropped. */
export const RECENT_MS = 1500;

export function emptyButtons(): Record<MouseButton, ButtonState> {
	const out = {} as Record<MouseButton, ButtonState>;
	for (const b of MOUSE_BUTTONS) out[b] = { down: false, pressedAt: null, releasedAt: null };
	return out;
}

export function emptyMouse(): MouseState {
	return {
		buttons: emptyButtons(),
		pointer: { x: 0.5, y: 0.5, known: false, movedAt: 0 },
		wheel: null,
		wheels: [],
		clicks: [],
	};
}

/* ------------------------------------------------------------------------ */
/* Keys                                                                      */
/* ------------------------------------------------------------------------ */

/**
 * Applies one `key` message. Returns a new object when anything changed,
 * otherwise the same object (cheap equality for the caller).
 */
export function applyKey(keys: KeyStates, msg: KeyMsg, at: number): KeyStates {
	const prev = keys[msg.phys];
	if (msg.down) {
		if (prev && prev.down && prev.key === msg.key) return keys; // auto-repeat
		return {
			...keys,
			[msg.phys]: {
				phys: msg.phys,
				key: msg.key,
				modifier: msg.modifier,
				down: true,
				pressedAt: at,
				releasedAt: null,
			},
		};
	}
	// A key-up with no matching key-down still lights the key briefly.
	return {
		...keys,
		[msg.phys]: {
			phys: msg.phys,
			key: msg.key,
			modifier: msg.modifier,
			down: false,
			pressedAt: prev ? prev.pressedAt : at,
			releasedAt: at,
		},
	};
}

/** Every held key is released at `at` (release_all, suppression, focus change). */
export function releaseAllKeys(keys: KeyStates, at: number): KeyStates {
	let changed = false;
	const out: Record<string, KeyState> = {};
	for (const [id, k] of Object.entries(keys)) {
		if (k.down) {
			out[id] = { ...k, down: false, releasedAt: at };
			changed = true;
		} else {
			out[id] = k;
		}
	}
	return changed ? out : keys;
}

/** Drops released keys whose decay has finished. */
export function pruneKeys(keys: KeyStates, at: number, decayMs: number): KeyStates {
	let changed = false;
	const out: Record<string, KeyState> = {};
	for (const [id, k] of Object.entries(keys)) {
		if (!k.down && k.releasedAt !== null && at - k.releasedAt >= decayMs) {
			changed = true;
			continue;
		}
		out[id] = k;
	}
	return changed ? out : keys;
}

/** When the next released key finishes decaying, or null. */
export function nextKeyDeadline(keys: KeyStates, decayMs: number): number | null {
	let next: number | null = null;
	for (const k of Object.values(keys)) {
		if (k.down || k.releasedAt === null) continue;
		const t = k.releasedAt + decayMs;
		if (next === null || t < next) next = t;
	}
	return next;
}

/**
 * How lit a key is, 1 while held, falling linearly to 0 over `decayMs` after
 * release. Exposed for the keyboard visual.
 */
export function keyIntensity(k: KeyState | undefined, at: number, decayMs: number): number {
	if (!k) return 0;
	if (k.down) return 1;
	if (k.releasedAt === null) return 0;
	if (decayMs <= 0) return 0;
	const t = (at - k.releasedAt) / decayMs;
	return t >= 1 ? 0 : t <= 0 ? 1 : 1 - t;
}

/* ------------------------------------------------------------------------ */
/* Mouse                                                                     */
/* ------------------------------------------------------------------------ */

let seqCounter = 0;
function nextSeq(): number {
	seqCounter += 1;
	return seqCounter;
}

function withPointer(m: MouseState, x: number, y: number, at: number): PointerState {
	if (m.pointer.known && m.pointer.x === x && m.pointer.y === y) return m.pointer;
	return { x, y, known: true, movedAt: at };
}

function recent<T extends { at: number }>(list: readonly T[], item: T, at: number): T[] {
	const kept = list.filter((e) => at - e.at < RECENT_MS);
	kept.push(item);
	return kept.length > RECENT_LIMIT ? kept.slice(kept.length - RECENT_LIMIT) : kept;
}

export function applyButton(m: MouseState, msg: ButtonMsg, at: number): MouseState {
	const prev = m.buttons[msg.button];
	const next: ButtonState = msg.down
		? { down: true, pressedAt: at, releasedAt: null }
		: { down: false, pressedAt: prev.pressedAt ?? at, releasedAt: at };
	const clicks = msg.down
		? recent(m.clicks, { seq: nextSeq(), button: msg.button, x: msg.x, y: msg.y, at }, at)
		: m.clicks;
	return {
		...m,
		buttons: { ...m.buttons, [msg.button]: next },
		pointer: withPointer(m, msg.x, msg.y, at),
		clicks,
	};
}

export function wheelDirection(dx: number, dy: number): WheelDirection | null {
	if (dy === 0 && dx === 0) return null;
	if (Math.abs(dy) >= Math.abs(dx)) return dy > 0 ? 'up' : 'down';
	return dx > 0 ? 'right' : 'left';
}

export function applyWheel(m: MouseState, msg: WheelMsg, at: number): MouseState {
	const direction = wheelDirection(msg.dx, msg.dy);
	if (!direction) return m;
	const delta = Math.abs(direction === 'up' || direction === 'down' ? msg.dy : msg.dx);
	const pulse: WheelPulse = {
		seq: nextSeq(),
		direction,
		notches: Math.max(1, Math.round(delta / 120)),
		x: msg.x,
		y: msg.y,
		at,
	};
	return {
		...m,
		pointer: withPointer(m, msg.x, msg.y, at),
		wheel: pulse,
		wheels: recent(m.wheels, pulse, at),
	};
}

export function applyMove(m: MouseState, msg: MoveMsg, at: number): MouseState {
	const pointer = withPointer(m, msg.x, msg.y, at);
	return pointer === m.pointer ? m : { ...m, pointer };
}

export function releaseAllButtons(m: MouseState, at: number): MouseState {
	let changed = false;
	const buttons = { ...m.buttons };
	for (const b of MOUSE_BUTTONS) {
		const s = buttons[b];
		if (s.down) {
			buttons[b] = { down: false, pressedAt: s.pressedAt, releasedAt: at };
			changed = true;
		}
	}
	return changed ? { ...m, buttons } : m;
}

/** Drops wheel pulses and clicks older than RECENT_MS. */
export function pruneMouse(m: MouseState, at: number): MouseState {
	const wheels = m.wheels.filter((e) => at - e.at < RECENT_MS);
	const clicks = m.clicks.filter((e) => at - e.at < RECENT_MS);
	if (wheels.length === m.wheels.length && clicks.length === m.clicks.length) return m;
	return { ...m, wheels, clicks };
}

export function nextMouseDeadline(m: MouseState): number | null {
	let next: number | null = null;
	for (const e of m.wheels) if (next === null || e.at + RECENT_MS < next) next = e.at + RECENT_MS;
	for (const e of m.clicks) if (next === null || e.at + RECENT_MS < next) next = e.at + RECENT_MS;
	return next;
}
