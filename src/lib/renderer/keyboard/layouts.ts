/**
 * Keyboard layouts for the keyboard visual: geometry in key units (1u = one
 * key pitch) and the PHYSICAL key each drawn key stands for.
 *
 * Keys are identified by keystage-core's wire ids for physical keys
 * (`PhysKey::wire_id`, crates/keystage-core/src/event.rs): "sc:XX" for a set 1
 * scan code, "sc:E0XX" when the hook reported the extended flag. Every value
 * is the "Scan 1 Make" column of Microsoft's scan code table, Keyboard Input
 * Overview, "Scan Codes":
 * https://learn.microsoft.com/en-us/windows/win32/inputdev/about-keyboard-input#scan-codes
 * (read 2026-10-01). Two rows of that table list more than one code:
 *
 *   Print Screen   0xE037, and 0x0054 "emitted on Alt+Print screen" (Note 1)
 *   Pause          0xE11D45, 0xE046 "emitted on Ctrl+Pause" (Note 2),
 *                  0x0045 "as seen in legacy keyboard messages" (Note 3)
 *   Num Lock       0x0045, and 0xE045 "as seen in legacy keyboard messages"
 *
 * so Pause and Num Lock can both arrive as "sc:45". That one code is shared
 * and resolved by the canonical key name the core sends with it (SHARED_CODES).
 * 0xE11D45 has no wire id (the core's ids hold at most two bytes).
 *
 * Geometry follows the common ANSI row offsets (Tab 1.5u, Caps Lock 1.75u,
 * left Shift 2.25u, bottom row 1.25u modifiers and a 6.25u Space bar). The ISO
 * variant (Enter over two rows, a 1.25u left Shift and the 102nd key, scan
 * code 0x56) is built from the same rows; the keyboard visual switches to it
 * once the 102nd key is pressed, because only an ISO keyboard has that key.
 *
 * Default legends are the US layout's, used until a `legends` message arrives.
 */
import type { KeyboardLayout, Modifier } from '../../protocol';
import { KEYBOARD_FOOTPRINTS } from '../metrics';

export type LegendKind = 'char' | 'fn' | 'word' | 'glyph' | 'none';
export type KeyGlyph = 'up' | 'down' | 'left' | 'right' | 'enter' | 'tab' | 'backspace';

export interface KeyLegend {
	/** Text drawn on the key (also the accessible name of a glyph). */
	text: string;
	kind: LegendKind;
	glyph?: KeyGlyph;
}

/** A rectangle in key units. */
export interface URect {
	x: number;
	y: number;
	w: number;
	h: number;
}

export interface KeyDef {
	/** The key's primary physical id, unique in its layout. */
	id: string;
	/** Every physical id that lights this key, `id` first. */
	codes: readonly string[];
	/**
	 * Canonical key names this key reports (ARCHITECTURE.md section 3). Used
	 * for the shared code and for injected input that carries no scan code.
	 */
	names: readonly string[];
	/** Bounding box in key units. */
	x: number;
	y: number;
	w: number;
	h: number;
	/** The key's outline as an orthogonal polygon, clockwise (y down). */
	outline: readonly (readonly [number, number])[];
	/** US legend. */
	legend: KeyLegend;
	/** Its legend follows the active Windows layout (the legends message). */
	char: boolean;
	mod: Modifier | null;
	/** A second id whose legend is drawn small in the corner (60%: the backtick on Esc). */
	sub: string | null;
}

export interface LayoutDef {
	id: KeyboardLayout;
	iso: boolean;
	name: string;
	keys: readonly KeyDef[];
	/** Width and height of the key area in key units (origin 0, 0). */
	width: number;
	height: number;
}

/**
 * Wire ids two keys may share, resolved by the canonical name in the key
 * message: Microsoft lists 0x45 for Num Lock and, in legacy keyboard
 * messages, for Pause.
 */
export const SHARED_CODES: Readonly<Record<string, readonly string[]>> = {
	'sc:45': ['Pause', 'NumLock'],
};

/** Wire ids of input that carried no scan code (injected keys). */
export function isNoScanId(phys: string): boolean {
	return phys === 'sc:00' || phys === 'sc:E000' || phys === 'sc:0';
}

/* ------------------------------------------------------------------------ */
/* The key table                                                             */
/* ------------------------------------------------------------------------ */

interface KeyInfo {
	codes: readonly string[];
	names: readonly string[];
	legend: KeyLegend;
	char?: boolean;
	mod?: Modifier;
	sub?: string;
}

const ch = (text: string): KeyLegend => ({ text, kind: 'char' });
const fn = (text: string): KeyLegend => ({ text, kind: 'fn' });
const word = (text: string): KeyLegend => ({ text, kind: 'word' });
const glyph = (g: KeyGlyph, text: string): KeyLegend => ({ text, kind: 'glyph', glyph: g });

function charKey(code: string, name: string, legend = name): KeyInfo {
	return { codes: [code], names: [name], legend: ch(legend), char: true };
}

/**
 * Every physical key a layout may draw, by a short handle. Codes are the
 * Scan 1 Make values of Microsoft's table (see the file header).
 */
const K = {
	esc: { codes: ['sc:01'], names: ['Esc'], legend: word('Esc') },
	f1: { codes: ['sc:3B'], names: ['F1'], legend: fn('F1') },
	f2: { codes: ['sc:3C'], names: ['F2'], legend: fn('F2') },
	f3: { codes: ['sc:3D'], names: ['F3'], legend: fn('F3') },
	f4: { codes: ['sc:3E'], names: ['F4'], legend: fn('F4') },
	f5: { codes: ['sc:3F'], names: ['F5'], legend: fn('F5') },
	f6: { codes: ['sc:40'], names: ['F6'], legend: fn('F6') },
	f7: { codes: ['sc:41'], names: ['F7'], legend: fn('F7') },
	f8: { codes: ['sc:42'], names: ['F8'], legend: fn('F8') },
	f9: { codes: ['sc:43'], names: ['F9'], legend: fn('F9') },
	f10: { codes: ['sc:44'], names: ['F10'], legend: fn('F10') },
	f11: { codes: ['sc:57'], names: ['F11'], legend: fn('F11') },
	f12: { codes: ['sc:58'], names: ['F12'], legend: fn('F12') },
	prtsc: { codes: ['sc:E037', 'sc:54'], names: ['PrintScreen'], legend: word('PrtSc') },
	scrlk: { codes: ['sc:46'], names: ['ScrollLock'], legend: word('ScrLk') },
	pause: { codes: ['sc:45', 'sc:E046'], names: ['Pause'], legend: word('Pause') },

	grave: charKey('sc:29', '`'),
	d1: charKey('sc:02', '1'),
	d2: charKey('sc:03', '2'),
	d3: charKey('sc:04', '3'),
	d4: charKey('sc:05', '4'),
	d5: charKey('sc:06', '5'),
	d6: charKey('sc:07', '6'),
	d7: charKey('sc:08', '7'),
	d8: charKey('sc:09', '8'),
	d9: charKey('sc:0A', '9'),
	d0: charKey('sc:0B', '0'),
	minus: charKey('sc:0C', '-'),
	equal: charKey('sc:0D', '='),
	bksp: { codes: ['sc:0E'], names: ['Backspace'], legend: glyph('backspace', 'Backspace') },

	tab: { codes: ['sc:0F'], names: ['Tab'], legend: glyph('tab', 'Tab') },
	q: charKey('sc:10', 'Q'),
	w: charKey('sc:11', 'W'),
	e: charKey('sc:12', 'E'),
	r: charKey('sc:13', 'R'),
	t: charKey('sc:14', 'T'),
	y: charKey('sc:15', 'Y'),
	u: charKey('sc:16', 'U'),
	i: charKey('sc:17', 'I'),
	o: charKey('sc:18', 'O'),
	p: charKey('sc:19', 'P'),
	lbr: charKey('sc:1A', '['),
	rbr: charKey('sc:1B', ']'),
	bslash: charKey('sc:2B', '\\'),

	caps: { codes: ['sc:3A'], names: ['CapsLock'], legend: word('Caps') },
	a: charKey('sc:1E', 'A'),
	s: charKey('sc:1F', 'S'),
	d: charKey('sc:20', 'D'),
	f: charKey('sc:21', 'F'),
	g: charKey('sc:22', 'G'),
	h: charKey('sc:23', 'H'),
	j: charKey('sc:24', 'J'),
	k: charKey('sc:25', 'K'),
	l: charKey('sc:26', 'L'),
	semi: charKey('sc:27', ';'),
	quote: charKey('sc:28', "'"),
	enter: { codes: ['sc:1C'], names: ['Enter'], legend: glyph('enter', 'Enter') },

	lshift: { codes: ['sc:2A'], names: ['Shift'], legend: word('Shift'), mod: 'shift' },
	intl: charKey('sc:56', 'IntlBackslash', '\\'),
	z: charKey('sc:2C', 'Z'),
	x: charKey('sc:2D', 'X'),
	c: charKey('sc:2E', 'C'),
	v: charKey('sc:2F', 'V'),
	b: charKey('sc:30', 'B'),
	n: charKey('sc:31', 'N'),
	m: charKey('sc:32', 'M'),
	comma: charKey('sc:33', ','),
	period: charKey('sc:34', '.'),
	slash: charKey('sc:35', '/'),
	rshift: { codes: ['sc:36'], names: ['Shift'], legend: word('Shift'), mod: 'shift' },

	lctrl: { codes: ['sc:1D'], names: ['Ctrl'], legend: word('Ctrl'), mod: 'ctrl' },
	lwin: { codes: ['sc:E05B'], names: ['Win'], legend: word('Win'), mod: 'win' },
	lalt: { codes: ['sc:38'], names: ['Alt'], legend: word('Alt'), mod: 'alt' },
	space: { codes: ['sc:39'], names: ['Space'], legend: word('Space'), char: true },
	ralt: { codes: ['sc:E038'], names: ['Alt', 'AltGr'], legend: word('Alt'), mod: 'alt' },
	rwin: { codes: ['sc:E05C'], names: ['Win'], legend: word('Win'), mod: 'win' },
	menu: { codes: ['sc:E05D'], names: ['Menu'], legend: word('Menu') },
	rctrl: { codes: ['sc:E01D'], names: ['Ctrl'], legend: word('Ctrl'), mod: 'ctrl' },

	ins: { codes: ['sc:E052'], names: ['Insert'], legend: word('Ins') },
	home: { codes: ['sc:E047'], names: ['Home'], legend: word('Home') },
	pgup: { codes: ['sc:E049'], names: ['PageUp'], legend: word('PgUp') },
	del: { codes: ['sc:E053'], names: ['Delete'], legend: word('Del') },
	end: { codes: ['sc:E04F'], names: ['End'], legend: word('End') },
	pgdn: { codes: ['sc:E051'], names: ['PageDown'], legend: word('PgDn') },
	up: { codes: ['sc:E048'], names: ['Up'], legend: glyph('up', 'Up') },
	left: { codes: ['sc:E04B'], names: ['Left'], legend: glyph('left', 'Left') },
	down: { codes: ['sc:E050'], names: ['Down'], legend: glyph('down', 'Down') },
	right: { codes: ['sc:E04D'], names: ['Right'], legend: glyph('right', 'Right') },

	numlk: { codes: ['sc:E045', 'sc:45'], names: ['NumLock'], legend: word('Num') },
	ndiv: { codes: ['sc:E035'], names: ['NumDivide'], legend: ch('/') },
	nmul: { codes: ['sc:37'], names: ['NumMultiply'], legend: ch('*') },
	nsub: { codes: ['sc:4A'], names: ['NumSubtract'], legend: ch('-') },
	n7: { codes: ['sc:47'], names: ['Num7', 'NumHome'], legend: ch('7') },
	n8: { codes: ['sc:48'], names: ['Num8', 'NumUp'], legend: ch('8') },
	n9: { codes: ['sc:49'], names: ['Num9', 'NumPageUp'], legend: ch('9') },
	nadd: { codes: ['sc:4E'], names: ['NumAdd'], legend: ch('+') },
	n4: { codes: ['sc:4B'], names: ['Num4', 'NumLeft'], legend: ch('4') },
	n5: { codes: ['sc:4C'], names: ['Num5', 'NumClear'], legend: ch('5') },
	n6: { codes: ['sc:4D'], names: ['Num6', 'NumRight'], legend: ch('6') },
	n1: { codes: ['sc:4F'], names: ['Num1', 'NumEnd'], legend: ch('1') },
	n2: { codes: ['sc:50'], names: ['Num2', 'NumDown'], legend: ch('2') },
	n3: { codes: ['sc:51'], names: ['Num3', 'NumPageDown'], legend: ch('3') },
	nenter: { codes: ['sc:E01C'], names: ['NumEnter'], legend: glyph('enter', 'Num Enter') },
	n0: { codes: ['sc:52'], names: ['Num0', 'NumInsert'], legend: ch('0') },
	ndot: { codes: ['sc:53'], names: ['NumDecimal', 'NumDelete'], legend: ch('.') },
} satisfies Record<string, KeyInfo>;

export type KeyHandle = keyof typeof K;

/** A key placed in a layout, before outlines are computed. */
interface Placed {
	key: KeyHandle;
	x: number;
	y: number;
	w: number;
	h: number;
	/** Extra ids that light this key in this layout only (folds). */
	extra?: readonly string[];
	/** Override the key's US legend in this layout. */
	legend?: KeyLegend;
	sub?: string;
	isoEnter?: boolean;
}

/** One item of a row: a key handle, `[handle, width]`, or a gap in key units. */
type RowItem = KeyHandle | readonly [KeyHandle, number] | { gap: number };

function row(y: number, x0: number, items: readonly RowItem[], h = 1): Placed[] {
	const out: Placed[] = [];
	let x = x0;
	for (const it of items) {
		if (typeof it === 'object' && 'gap' in it) {
			x += it.gap;
			continue;
		}
		const [key, w] = typeof it === 'string' ? [it, 1] : it;
		out.push({ key, x, y, w, h });
		x += w;
	}
	return out;
}

function at(key: KeyHandle, x: number, y: number, w = 1, h = 1): Placed {
	return { key, x, y, w, h };
}

/* ------------------------------------------------------------------------ */
/* Rows shared by every layout (the 15u alphanumeric block)                  */
/* ------------------------------------------------------------------------ */

const NUM_ROW: RowItem[] = [
	'grave',
	'd1',
	'd2',
	'd3',
	'd4',
	'd5',
	'd6',
	'd7',
	'd8',
	'd9',
	'd0',
	'minus',
	'equal',
	['bksp', 2],
];
const Q_ROW: RowItem[] = [
	['tab', 1.5],
	'q',
	'w',
	'e',
	'r',
	't',
	'y',
	'u',
	'i',
	'o',
	'p',
	'lbr',
	'rbr',
	['bslash', 1.5],
];
const HOME_ROW: RowItem[] = [
	['caps', 1.75],
	'a',
	's',
	'd',
	'f',
	'g',
	'h',
	'j',
	'k',
	'l',
	'semi',
	'quote',
	['enter', 2.25],
];
const LETTERS_Z: RowItem[] = ['z', 'x', 'c', 'v', 'b', 'n', 'm', 'comma', 'period', 'slash'];
const SHIFT_ROW: RowItem[] = [['lshift', 2.25], ...LETTERS_Z, ['rshift', 2.75]];
const BOTTOM_ROW: RowItem[] = [
	['lctrl', 1.25],
	['lwin', 1.25],
	['lalt', 1.25],
	['space', 6.25],
	['ralt', 1.25],
	['rwin', 1.25],
	['menu', 1.25],
	['rctrl', 1.25],
];

/** The standard function row: Esc, a 1u gap, then F1 to F12 in groups of four. */
const F_ROW: RowItem[] = [
	'esc',
	{ gap: 1 },
	'f1',
	'f2',
	'f3',
	'f4',
	{ gap: 0.5 },
	'f5',
	'f6',
	'f7',
	'f8',
	{ gap: 0.5 },
	'f9',
	'f10',
	'f11',
	'f12',
];

/** Rows of the 15u block starting at `y0` with 1u pitch. */
function alphaBlock(y0: number): Placed[] {
	return [
		...row(y0, 0, NUM_ROW),
		...row(y0 + 1, 0, Q_ROW),
		...row(y0 + 2, 0, HOME_ROW),
		...row(y0 + 3, 0, SHIFT_ROW),
		...row(y0 + 4, 0, BOTTOM_ROW),
	];
}

/* ------------------------------------------------------------------------ */
/* Layouts                                                                    */
/* ------------------------------------------------------------------------ */

/** Main rows start under the function row and its 0.5u gap. */
const MAIN_Y = 1.5;

function navCluster(x0: number): Placed[] {
	const y = MAIN_Y;
	return [
		...row(0, x0, ['prtsc', 'scrlk', 'pause']),
		...row(y, x0, ['ins', 'home', 'pgup']),
		...row(y + 1, x0, ['del', 'end', 'pgdn']),
		at('up', x0 + 1, y + 3),
		...row(y + 4, x0, ['left', 'down', 'right']),
	];
}

function numpad(x0: number): Placed[] {
	const y = MAIN_Y;
	return [
		...row(y, x0, ['numlk', 'ndiv', 'nmul', 'nsub']),
		...row(y + 1, x0, ['n7', 'n8', 'n9']),
		at('nadd', x0 + 3, y + 1, 1, 2),
		...row(y + 2, x0, ['n4', 'n5', 'n6']),
		...row(y + 3, x0, ['n1', 'n2', 'n3']),
		at('nenter', x0 + 3, y + 3, 1, 2),
		at('n0', x0, y + 4, 2),
		at('ndot', x0 + 2, y + 4),
	];
}

function fullKeys(): Placed[] {
	return [...row(0, 0, F_ROW), ...alphaBlock(MAIN_Y), ...navCluster(15.25), ...numpad(18.5)];
}

function tklKeys(): Placed[] {
	return [...row(0, 0, F_ROW), ...alphaBlock(MAIN_Y), ...navCluster(15.25)];
}

/**
 * 75 percent: a 16u board. Function row with 0.25u gaps, Print Screen and
 * Delete at its end; a right column of Home, Page Up, Page Down and End; the
 * arrows tucked under a 1.75u right Shift; a 1u right Alt, Menu and Ctrl.
 */
function seventyFiveKeys(): Placed[] {
	const y = 1.25;
	const f: RowItem[] = [
		'esc',
		{ gap: 0.25 },
		'f1',
		'f2',
		'f3',
		'f4',
		{ gap: 0.25 },
		'f5',
		'f6',
		'f7',
		'f8',
		{ gap: 0.25 },
		'f9',
		'f10',
		'f11',
		'f12',
		{ gap: 0.25 },
		'prtsc',
		'del',
	];
	return [
		...row(0, 0, f),
		...row(y, 0, [...NUM_ROW, 'home']),
		...row(y + 1, 0, [...Q_ROW, 'pgup']),
		...row(y + 2, 0, [...HOME_ROW, 'pgdn']),
		...row(y + 3, 0, [['lshift', 2.25], ...LETTERS_Z, ['rshift', 1.75], 'up', 'end']),
		...row(y + 4, 0, [
			['lctrl', 1.25],
			['lwin', 1.25],
			['lalt', 1.25],
			['space', 6.25],
			'ralt',
			'menu',
			'rctrl',
			'left',
			'down',
			'right',
		]),
	];
}

/**
 * 60 percent: the 15u block alone. Its top-left key is Esc, as on 60 percent
 * boards; the backtick key lights it too and its legend is drawn small in
 * the corner.
 */
function sixtyKeys(): Placed[] {
	const keys = alphaBlock(0);
	const grave = keys.findIndex((p) => p.key === 'grave');
	const g = keys[grave] as Placed;
	keys[grave] = { ...g, key: 'esc', extra: ['sc:29'], sub: 'sc:29' };
	return keys;
}

/**
 * Laptop: a 15u board with a short (0.75u) function row of Esc, F1 to F12,
 * Print Screen and Delete; a 5.75u Space bar, a right Alt and Ctrl, and the
 * arrows inside the block with half-height Up and Down. Fn is not drawn: it
 * is handled inside the keyboard and sends no scan code.
 */
function laptopKeys(): Placed[] {
	const y = 1;
	const fItems: RowItem[] = [
		'esc',
		'f1',
		'f2',
		'f3',
		'f4',
		'f5',
		'f6',
		'f7',
		'f8',
		'f9',
		'f10',
		'f11',
		'f12',
		'prtsc',
		'del',
	];
	return [
		...row(0, 0, fItems, 0.75),
		...row(y, 0, NUM_ROW),
		...row(y + 1, 0, Q_ROW),
		...row(y + 2, 0, HOME_ROW),
		...row(y + 3, 0, SHIFT_ROW),
		...row(y + 4, 0, [
			['lctrl', 1.25],
			['lwin', 1.25],
			['lalt', 1.25],
			['space', 5.75],
			['ralt', 1.25],
			['rctrl', 1.25],
			'left',
		]),
		at('up', 13, y + 4, 1, 0.5),
		at('down', 13, y + 4.5, 1, 0.5),
		at('right', 14, y + 4),
	];
}

/** Keys of the left-hand zone, by handle (a subset of the full layout). */
export const LEFT_ZONE: readonly KeyHandle[] = [
	'esc',
	'f1',
	'f2',
	'f3',
	'f4',
	'f5',
	'f6',
	'f7',
	'f8',
	'grave',
	'd1',
	'd2',
	'd3',
	'd4',
	'd5',
	'd6',
	'tab',
	'q',
	'w',
	'e',
	'r',
	't',
	'caps',
	'a',
	's',
	'd',
	'f',
	'g',
	'lshift',
	'intl',
	'z',
	'x',
	'c',
	'v',
	'b',
	'lctrl',
	'lwin',
	'lalt',
	'space',
];

/**
 * Right-hand modifiers fold onto the left ones in the left-hand zone, so a
 * shortcut held with the right Ctrl still lights Ctrl.
 */
const LEFT_FOLDS: Partial<Record<KeyHandle, readonly string[]>> = {
	lctrl: ['sc:E01D'],
	lshift: ['sc:36'],
	lalt: ['sc:E038'],
	lwin: ['sc:E05C'],
};

function leftKeys(iso: boolean): Placed[] {
	const zone = new Set<KeyHandle>(LEFT_ZONE);
	return isoTransform(fullKeys(), iso)
		.filter((p) => zone.has(p.key))
		.map((p) => {
			const extra = LEFT_FOLDS[p.key];
			return extra ? { ...p, extra } : p;
		});
}

/* ------------------------------------------------------------------------ */
/* ISO                                                                        */
/* ------------------------------------------------------------------------ */

/**
 * Turns ANSI rows into ISO rows: the backslash key moves from the end of the
 * Q row to the home row beside Enter, Enter spans two rows, and the left
 * Shift shrinks to 1.25u to make room for the 102nd key.
 */
function isoTransform(keys: Placed[], iso: boolean): Placed[] {
	if (!iso) return keys;
	const bs = keys.find((p) => p.key === 'bslash');
	const enter = keys.find((p) => p.key === 'enter');
	const shift = keys.find((p) => p.key === 'lshift');
	if (!bs || !enter || !shift) return keys;
	const out = keys.filter((p) => p !== bs && p !== enter && p !== shift);
	out.push({ key: 'enter', x: bs.x, y: bs.y, w: bs.w, h: 2, isoEnter: true });
	out.push({ key: 'bslash', x: enter.x, y: enter.y, w: 1, h: 1 });
	out.push({ ...shift, w: 1.25 });
	out.push({ key: 'intl', x: shift.x + 1.25, y: shift.y, w: 1, h: 1 });
	return out;
}

/* ------------------------------------------------------------------------ */
/* Building                                                                   */
/* ------------------------------------------------------------------------ */

type Pt = readonly [number, number];

function rectOutline(x: number, y: number, w: number, h: number): Pt[] {
	return [
		[x, y],
		[x + w, y],
		[x + w, y + h],
		[x, y + h],
	];
}

/**
 * The ISO Enter: 1.5u wide on the upper row, 1.25u wide on the lower row,
 * flush right. Clockwise from the top-left corner.
 */
function isoEnterOutline(x: number, y: number, w: number): Pt[] {
	const right = x + w;
	return [
		[x, y],
		[right, y],
		[right, y + 2],
		[right - 1.25, y + 2],
		[right - 1.25, y + 1],
		[x, y + 1],
	];
}

function round4(n: number): number {
	return Math.round(n * 10000) / 10000;
}

function toDef(p: Placed): KeyDef {
	const info: KeyInfo = K[p.key];
	const codes = [...info.codes, ...(p.extra ?? [])];
	const outline = p.isoEnter ? isoEnterOutline(p.x, p.y, p.w) : rectOutline(p.x, p.y, p.w, p.h);
	return {
		id: info.codes[0] as string,
		codes,
		names: info.names,
		x: round4(p.x),
		y: round4(p.y),
		w: round4(p.w),
		h: round4(p.h),
		outline: outline.map(([a, b]) => [round4(a), round4(b)] as const),
		legend: p.legend ?? info.legend,
		char: info.char === true,
		mod: info.mod ?? null,
		sub: p.sub ?? info.sub ?? null,
	};
}

const NAMES: Readonly<Record<KeyboardLayout, string>> = {
	full: 'Full size',
	tkl: 'Tenkeyless',
	'75': '75 percent',
	'60': '60 percent',
	laptop: 'Laptop',
	left: 'Left-hand zone',
};

function placedFor(id: KeyboardLayout, iso: boolean): Placed[] {
	switch (id) {
		case 'full':
			return isoTransform(fullKeys(), iso);
		case 'tkl':
			return isoTransform(tklKeys(), iso);
		case '75':
			return isoTransform(seventyFiveKeys(), iso);
		case '60':
			return isoTransform(sixtyKeys(), iso);
		case 'laptop':
			return isoTransform(laptopKeys(), iso);
		case 'left':
			return leftKeys(iso);
	}
}

const cache = new Map<string, LayoutDef>();

/** The layout, ANSI or ISO. Layouts are built once and shared. */
export function getLayout(id: KeyboardLayout, iso = false): LayoutDef {
	const cacheKey = `${id}:${iso ? 'iso' : 'ansi'}`;
	const hit = cache.get(cacheKey);
	if (hit) return hit;
	const keys = placedFor(id, iso)
		.map(toDef)
		.sort((a, b) => a.y - b.y || a.x - b.x);
	let width = 0;
	let height = 0;
	for (const k of keys) {
		width = Math.max(width, k.x + k.w);
		height = Math.max(height, k.y + k.h);
	}
	const def: LayoutDef = {
		id,
		iso,
		name: NAMES[id],
		keys,
		width: round4(width),
		height: round4(height),
	};
	cache.set(cacheKey, def);
	return def;
}

/** Every layout id, in settings order. */
export const LAYOUT_IDS: readonly KeyboardLayout[] = ['full', 'tkl', '75', '60', 'laptop', 'left'];

/** The box (key units) a layout is drawn in, housing included (metrics.ts). */
export function footprintOf(id: KeyboardLayout): { w: number; h: number } {
	return KEYBOARD_FOOTPRINTS[id];
}

/** The US legend of a physical id, for keys the legends message does not cover. */
export function usLegendOf(phys: string): string | null {
	for (const info of Object.values(K) as KeyInfo[]) {
		if (info.codes.includes(phys)) return info.legend.text;
	}
	return null;
}
