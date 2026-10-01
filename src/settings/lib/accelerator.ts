/**
 * Key names and accelerator strings, ported from keystage-core keys.rs so the
 * settings window can check a hotkey before it is saved:
 *
 * * canonical key names (ARCHITECTURE.md section 3), `isCanonical`
 * * `parseKeyName`: the aliases core accepts (`Escape`, `PgUp`, `KeyP`, ...)
 * * `parseAccelerator` / `formatAccelerator`: stored order Ctrl, Alt, AltGr,
 *   Shift, Win (core `ACCEL_ORDER`), e.g. `Ctrl+Alt+Shift+P`
 * * `isRegistrable`: what tauri-plugin-global-shortcut can register (core
 *   `Accelerator::to_global_shortcut`)
 * * `keyFromCode`: a KeyboardEvent `code` to a canonical name, for capture
 *
 * accelerator.test.ts checks the lists against keys.rs.
 */

export const MOD_NAMES = ['Ctrl', 'Alt', 'AltGr', 'Shift', 'Win'] as const;
export type ModName = (typeof MOD_NAMES)[number];

/** Display order of modifiers in callouts and import keys (Ctrl, Shift, Alt, AltGr, Win). */
export const DISPLAY_MOD_ORDER: readonly ModName[] = ['Ctrl', 'Shift', 'Alt', 'AltGr', 'Win'];

/** keystage-core `NAMED`: every canonical name that is not a letter, digit, F-key or VK_xx. */
export const NAMED_KEYS: readonly string[] = [
	'Ctrl',
	'Shift',
	'Alt',
	'Win',
	'AltGr',
	'Space',
	'Enter',
	'Esc',
	'Tab',
	'Backspace',
	'Delete',
	'Insert',
	'Home',
	'End',
	'PageUp',
	'PageDown',
	'Up',
	'Down',
	'Left',
	'Right',
	'CapsLock',
	'PrintScreen',
	'ScrollLock',
	'Pause',
	'Menu',
	'NumLock',
	'Num0',
	'Num1',
	'Num2',
	'Num3',
	'Num4',
	'Num5',
	'Num6',
	'Num7',
	'Num8',
	'Num9',
	'NumAdd',
	'NumSubtract',
	'NumMultiply',
	'NumDivide',
	'NumDecimal',
	'NumEnter',
	'NumHome',
	'NumEnd',
	'NumPageUp',
	'NumPageDown',
	'NumUp',
	'NumDown',
	'NumLeft',
	'NumRight',
	'NumInsert',
	'NumDelete',
	'NumClear',
	'`',
	'-',
	'=',
	'[',
	']',
	'\\',
	';',
	"'",
	',',
	'.',
	'/',
	'VolumeUp',
	'VolumeDown',
	'VolumeMute',
	'MediaPlayPause',
	'MediaNext',
	'MediaPrev',
	'MediaStop',
	'BrowserBack',
	'BrowserForward',
	'BrowserRefresh',
	'BrowserHome',
	'BrowserSearch',
	'LaunchMail',
	'LaunchApp1',
	'LaunchApp2',
	'IntlBackslash',
];
const NAMED_SET = new Set(NAMED_KEYS);

export function isCanonical(name: string): boolean {
	if (NAMED_SET.has(name)) return true;
	if (name.length === 1) return /^[A-Z0-9]$/.test(name);
	const f = /^F([1-9][0-9]?)$/.exec(name);
	if (f) {
		const n = Number(f[1]);
		return n >= 1 && n <= 24;
	}
	return /^VK_[0-9A-F]{2}$/.test(name);
}

export function isModifierName(name: string): name is ModName {
	return (MOD_NAMES as readonly string[]).includes(name);
}

/** keystage-core `ModKey::from_alias`. */
export function modFromAlias(lower: string): ModName | null {
	switch (lower) {
		case 'ctrl':
		case 'control':
		case 'ctl':
		case 'commandorcontrol':
		case 'cmdorctrl':
		case 'commandorctrl':
		case 'cmdorcontrol':
			return 'Ctrl';
		case 'shift':
			return 'Shift';
		case 'alt':
		case 'option':
			return 'Alt';
		case 'altgr':
		case 'altgraph':
			return 'AltGr';
		case 'win':
		case 'windows':
		case 'super':
		case 'meta':
		case 'cmd':
		case 'command':
		case 'logo':
			return 'Win';
		default:
			return null;
	}
}

const ALIASES: Readonly<Record<string, string>> = {
	space: 'Space',
	spacebar: 'Space',
	enter: 'Enter',
	return: 'Enter',
	esc: 'Esc',
	escape: 'Esc',
	tab: 'Tab',
	backspace: 'Backspace',
	back: 'Backspace',
	bksp: 'Backspace',
	delete: 'Delete',
	del: 'Delete',
	insert: 'Insert',
	ins: 'Insert',
	home: 'Home',
	end: 'End',
	pageup: 'PageUp',
	pgup: 'PageUp',
	prior: 'PageUp',
	pagedown: 'PageDown',
	pgdn: 'PageDown',
	pgdown: 'PageDown',
	next: 'PageDown',
	up: 'Up',
	arrowup: 'Up',
	uparrow: 'Up',
	down: 'Down',
	arrowdown: 'Down',
	downarrow: 'Down',
	left: 'Left',
	arrowleft: 'Left',
	leftarrow: 'Left',
	right: 'Right',
	arrowright: 'Right',
	rightarrow: 'Right',
	capslock: 'CapsLock',
	caps: 'CapsLock',
	printscreen: 'PrintScreen',
	prtsc: 'PrintScreen',
	prtscn: 'PrintScreen',
	print: 'PrintScreen',
	snapshot: 'PrintScreen',
	scrolllock: 'ScrollLock',
	scroll: 'ScrollLock',
	pause: 'Pause',
	break: 'Pause',
	menu: 'Menu',
	apps: 'Menu',
	application: 'Menu',
	contextmenu: 'Menu',
	menukey: 'Menu',
	numlock: 'NumLock',
	backquote: '`',
	grave: '`',
	backtick: '`',
	minus: '-',
	dash: '-',
	hyphen: '-',
	equal: '=',
	equals: '=',
	bracketleft: '[',
	leftbracket: '[',
	bracketright: ']',
	rightbracket: ']',
	backslash: '\\',
	semicolon: ';',
	quote: "'",
	apostrophe: "'",
	comma: ',',
	period: '.',
	dot: '.',
	slash: '/',
	intlbackslash: 'IntlBackslash',
	numpadenter: 'NumEnter',
	numenter: 'NumEnter',
	numpadadd: 'NumAdd',
	numadd: 'NumAdd',
	'num+': 'NumAdd',
	numpadsubtract: 'NumSubtract',
	numsubtract: 'NumSubtract',
	'num-': 'NumSubtract',
	numpadmultiply: 'NumMultiply',
	nummultiply: 'NumMultiply',
	'num*': 'NumMultiply',
	numpaddivide: 'NumDivide',
	numdivide: 'NumDivide',
	'num/': 'NumDivide',
	numpaddecimal: 'NumDecimal',
	numdecimal: 'NumDecimal',
	'num.': 'NumDecimal',
	audiovolumeup: 'VolumeUp',
	volumeup: 'VolumeUp',
	audiovolumedown: 'VolumeDown',
	volumedown: 'VolumeDown',
	audiovolumemute: 'VolumeMute',
	volumemute: 'VolumeMute',
	mute: 'VolumeMute',
	mediaplaypause: 'MediaPlayPause',
	playpause: 'MediaPlayPause',
	mediatracknext: 'MediaNext',
	medianext: 'MediaNext',
	mediatrackprevious: 'MediaPrev',
	mediaprev: 'MediaPrev',
	mediastop: 'MediaStop',
};

/** keystage-core `parse_key_name`: one key as people and Tauri write it, to its canonical name. */
export function parseKeyName(raw: string): string | null {
	const t = raw.trim();
	if (t.length === 0) return null;
	if (isCanonical(t)) return t;
	if ([...t].length === 1) return /^[a-zA-Z]$/.test(t) ? t.toUpperCase() : null;
	const compact = t.replace(/[\s_-]/g, '');
	const lower = compact.toLowerCase();
	const mod = modFromAlias(lower);
	if (mod) return mod;
	const named = ALIASES[lower];
	if (named) return named;
	let m = /^key([a-z])$/.exec(lower);
	if (m && m[1]) return m[1].toUpperCase();
	m = /^digit([0-9])$/.exec(lower);
	if (m && m[1]) return m[1];
	m = /^(?:numpad|num)([0-9])$/.exec(lower);
	if (m && m[1]) return `Num${m[1]}`;
	m = /^f([1-9][0-9]?)$/.exec(lower);
	if (m && m[1]) {
		const n = Number(m[1]);
		if (n >= 1 && n <= 24) return `F${n}`;
	}
	return null;
}

export interface Accelerator {
	/** In stored order (Ctrl, Alt, AltGr, Shift, Win), no repeats. */
	mods: ModName[];
	key: string;
}

export type AcceleratorResult = { ok: true; accel: Accelerator } | { ok: false; error: string };

function sortMods(mods: Iterable<ModName>): ModName[] {
	const set = new Set(mods);
	return MOD_NAMES.filter((m) => set.has(m));
}

/** keystage-core `parse_accelerator`, with the same error cases in plain words. */
export function parseAccelerator(s: string): AcceleratorResult {
	const trimmed = s.trim();
	if (trimmed.length === 0) return { ok: false, error: 'The shortcut is empty.' };
	const mods = new Set<ModName>();
	let key: string | null = null;
	for (const raw of trimmed.split('+')) {
		const part = raw.trim();
		if (part.length === 0) return { ok: false, error: 'The shortcut has an empty part.' };
		const mod = modFromAlias(part.replace(/\s/g, '').toLowerCase());
		if (mod) {
			mods.add(mod);
			continue;
		}
		const name = parseKeyName(part);
		if (!name) return { ok: false, error: `"${part}" is not a key name.` };
		if (key !== null) return { ok: false, error: 'The shortcut has more than one key.' };
		key = name;
	}
	if (key === null) return { ok: false, error: 'The shortcut needs a key besides the modifiers.' };
	return { ok: true, accel: { mods: sortMods(mods), key } };
}

export function formatAccelerator(a: Accelerator): string {
	return [...sortMods(a.mods), a.key].join('+');
}

/** The canonical stored form, or null when the string cannot be read. */
export function normalizeAccelerator(s: string): string | null {
	const r = parseAccelerator(s);
	return r.ok ? formatAccelerator(r.accel) : null;
}

/** The parts of an accelerator in display order (Ctrl, Shift, Alt, Win, key), for keycaps. */
export function acceleratorParts(s: string): string[] {
	const r = parseAccelerator(s);
	if (!r.ok) return s.trim().length > 0 ? [s.trim()] : [];
	const set = new Set(r.accel.mods);
	return [...DISPLAY_MOD_ORDER.filter((m) => set.has(m)), r.accel.key];
}

export function isFunctionKey(key: string): boolean {
	return /^F([1-9]|1[0-9]|2[0-4])$/.test(key);
}

/** Ctrl, Alt or Win is held (core `ModSet::has_command`). */
export function hasCommandModifier(a: Accelerator): boolean {
	return a.mods.some((m) => m === 'Ctrl' || m === 'Alt' || m === 'Win');
}

const REGISTRABLE_SINGLE = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789`\\[],-.=';/";
const REGISTRABLE_NAMED = new Set([
	'Space',
	'Enter',
	'Tab',
	'Esc',
	'Backspace',
	'Delete',
	'Insert',
	'Home',
	'End',
	'PageUp',
	'PageDown',
	'Up',
	'Down',
	'Left',
	'Right',
	'CapsLock',
	'PrintScreen',
	'ScrollLock',
	'Pause',
	'NumLock',
	'NumAdd',
	'NumSubtract',
	'NumMultiply',
	'NumDivide',
	'NumDecimal',
	'NumEnter',
	'VolumeUp',
	'VolumeDown',
	'VolumeMute',
	'MediaPlayPause',
	'MediaStop',
	'MediaNext',
	'MediaPrev',
]);

/** keystage-core `to_global_shortcut().is_some()`: the global-shortcut plugin can register it. */
export function isRegistrable(a: Accelerator): boolean {
	if (a.mods.includes('AltGr')) return false;
	const k = a.key;
	if (k.length === 1) return REGISTRABLE_SINGLE.includes(k);
	if (isFunctionKey(k)) return true;
	if (/^Num[0-9]$/.test(k)) return true;
	return REGISTRABLE_NAMED.has(k);
}

const CODE_NAMES: Readonly<Record<string, string>> = {
	Space: 'Space',
	Enter: 'Enter',
	NumpadEnter: 'NumEnter',
	Escape: 'Esc',
	Tab: 'Tab',
	Backspace: 'Backspace',
	Delete: 'Delete',
	Insert: 'Insert',
	Home: 'Home',
	End: 'End',
	PageUp: 'PageUp',
	PageDown: 'PageDown',
	ArrowUp: 'Up',
	ArrowDown: 'Down',
	ArrowLeft: 'Left',
	ArrowRight: 'Right',
	CapsLock: 'CapsLock',
	PrintScreen: 'PrintScreen',
	ScrollLock: 'ScrollLock',
	Pause: 'Pause',
	ContextMenu: 'Menu',
	NumLock: 'NumLock',
	NumpadAdd: 'NumAdd',
	NumpadSubtract: 'NumSubtract',
	NumpadMultiply: 'NumMultiply',
	NumpadDivide: 'NumDivide',
	NumpadDecimal: 'NumDecimal',
	Backquote: '`',
	Minus: '-',
	Equal: '=',
	BracketLeft: '[',
	BracketRight: ']',
	Backslash: '\\',
	Semicolon: ';',
	Quote: "'",
	Comma: ',',
	Period: '.',
	Slash: '/',
	IntlBackslash: 'IntlBackslash',
	AudioVolumeUp: 'VolumeUp',
	AudioVolumeDown: 'VolumeDown',
	AudioVolumeMute: 'VolumeMute',
	MediaPlayPause: 'MediaPlayPause',
	MediaTrackNext: 'MediaNext',
	MediaTrackPrevious: 'MediaPrev',
	MediaStop: 'MediaStop',
};

/** Modifier keys by KeyboardEvent `code`. */
export function modifierFromCode(code: string): ModName | null {
	switch (code) {
		case 'ControlLeft':
		case 'ControlRight':
			return 'Ctrl';
		case 'ShiftLeft':
		case 'ShiftRight':
			return 'Shift';
		case 'AltLeft':
			return 'Alt';
		case 'AltRight':
			return 'Alt';
		case 'MetaLeft':
		case 'MetaRight':
		case 'OSLeft':
		case 'OSRight':
			return 'Win';
		default:
			return null;
	}
}

/**
 * The canonical name of the physical key in a KeyboardEvent (`code` is the
 * physical key, like the core's names, which come from the US legend), or
 * null for a modifier or an unknown key.
 */
export function keyFromCode(code: string): string | null {
	if (modifierFromCode(code)) return null;
	const letter = /^Key([A-Z])$/.exec(code);
	if (letter && letter[1]) return letter[1];
	const digit = /^Digit([0-9])$/.exec(code);
	if (digit && digit[1]) return digit[1];
	const num = /^Numpad([0-9])$/.exec(code);
	if (num && num[1]) return `Num${num[1]}`;
	const f = /^F([1-9][0-9]?)$/.exec(code);
	if (f && f[1]) {
		const n = Number(f[1]);
		return n >= 1 && n <= 24 ? `F${n}` : null;
	}
	return CODE_NAMES[code] ?? null;
}

export interface CaptureEvent {
	code: string;
	ctrlKey: boolean;
	altKey: boolean;
	shiftKey: boolean;
	metaKey: boolean;
}

/** The modifiers an event says are held, in stored order. */
export function heldModifiers(e: Omit<CaptureEvent, 'code'>): ModName[] {
	const held: ModName[] = [];
	if (e.ctrlKey) held.push('Ctrl');
	if (e.altKey) held.push('Alt');
	if (e.shiftKey) held.push('Shift');
	if (e.metaKey) held.push('Win');
	return held;
}

export type CaptureStep =
	| { kind: 'modifiers'; mods: ModName[] }
	| { kind: 'done'; accel: Accelerator }
	| { kind: 'cancel' }
	| { kind: 'unknown'; code: string };

/**
 * Reads one keydown during hotkey capture. Modifiers alone keep waiting
 * (and show what is held); Esc with nothing held cancels; any other key
 * finishes the shortcut.
 */
export function captureStep(e: CaptureEvent): CaptureStep {
	const held = heldModifiers(e);
	const asMod = modifierFromCode(e.code);
	if (asMod) {
		if (!held.includes(asMod)) held.push(asMod);
		return { kind: 'modifiers', mods: sortMods(held) };
	}
	const key = keyFromCode(e.code);
	if (key === 'Esc' && held.length === 0) return { kind: 'cancel' };
	if (!key) return { kind: 'unknown', code: e.code };
	return { kind: 'done', accel: { mods: sortMods(held), key } };
}
