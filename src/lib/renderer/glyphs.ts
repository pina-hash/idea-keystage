/**
 * Key glyphs (the `symbols` option), mouse chip parsing and label helpers.
 * Glyphs are SVG paths in a 24 x 24 box, drawn with `currentColor`, so they
 * are crisp at any scale and need no font coverage.
 */
import type { Callout, Chip } from '../protocol';

export type GlyphId = 'up' | 'down' | 'left' | 'right' | 'enter' | 'tab' | 'backspace';

export interface Glyph {
	/** Stroked path data, 24 x 24 box, 2px round strokes. */
	d: string;
}

export const GLYPHS: Readonly<Record<GlyphId, Glyph>> = {
	up: { d: 'M12 19V5M6 11l6-6 6 6' },
	down: { d: 'M12 5v14M6 13l6 6 6-6' },
	left: { d: 'M19 12H5M11 6l-6 6 6 6' },
	right: { d: 'M5 12h14M13 6l6 6-6 6' },
	enter: { d: 'M19 5v7a3 3 0 0 1-3 3H6M10 11l-4 4 4 4' },
	tab: { d: 'M4 12h14M13 7l5 5-5 5M20 6v12' },
	backspace: { d: 'M9 5h10a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1H9l-6-7zM11.5 9.5l5 5M16.5 9.5l-5 5' },
};

/** Plain words for the glyphs, used as accessible text. */
export const GLYPH_WORDS: Readonly<Record<GlyphId, string>> = {
	up: 'Up arrow',
	down: 'Down arrow',
	left: 'Left arrow',
	right: 'Right arrow',
	enter: 'Enter',
	tab: 'Tab',
	backspace: 'Backspace',
};

const KEY_GLYPHS: Readonly<Record<string, GlyphId>> = {
	Up: 'up',
	Down: 'down',
	Left: 'left',
	Right: 'right',
	Enter: 'enter',
	Tab: 'tab',
	Backspace: 'backspace',
};

export interface KeyGlyph {
	id: GlyphId;
	/** A word drawn before the glyph ("Num" for the numpad keys). */
	prefix: string | null;
	/** Accessible words for the whole key. */
	words: string;
}

/**
 * The glyph for a canonical key name, or null when the key has none.
 * Numpad arrows and Enter keep a "Num" prefix so they never read as the
 * navigation keys.
 */
export function glyphFor(key: string): KeyGlyph | null {
	const direct = KEY_GLYPHS[key];
	if (direct) return { id: direct, prefix: null, words: GLYPH_WORDS[direct] };
	if (key.startsWith('Num')) {
		const rest = KEY_GLYPHS[key.slice(3)];
		if (rest && rest !== 'tab' && rest !== 'backspace') {
			return { id: rest, prefix: 'Num', words: `Num ${GLYPH_WORDS[rest]}` };
		}
	}
	return null;
}

/* ------------------------------------------------------------------------ */
/* Mouse chips                                                               */
/* ------------------------------------------------------------------------ */

export type MousePart = 'left' | 'right' | 'middle' | 'x1' | 'x2' | 'wheel';
export type MouseHint = 'up' | 'down' | 'left' | 'right' | null;

export interface MouseChipInfo {
	part: MousePart | null;
	/** Wheel direction or gesture direction, when the chip names one. */
	hint: MouseHint;
}

const PART_WORDS: ReadonlyArray<[RegExp, MousePart]> = [
	[/\bwheel\b|\bscroll\b/i, 'wheel'],
	[/\bmiddle\b|\bmmb\b/i, 'middle'],
	[/\bx1\b|\bback\b/i, 'x1'],
	[/\bx2\b|\bforward\b/i, 'x2'],
	[/\bright\b|\brmb\b/i, 'right'],
	[/\bleft\b|\blmb\b/i, 'left'],
];

/**
 * Reads which mouse part a chip is about from its canonical key and label
 * ("Middle drag", "Wheel up", "Ctrl + Left click", "Right gesture up").
 * The first part word wins, in the order wheel, middle, x1, x2, right, left,
 * so "Right gesture left" is the right button.
 */
export function mouseChipInfo(chip: Chip): MouseChipInfo {
	const text = `${chip.key} ${chip.label}`.replace(/([a-z])([A-Z])/g, '$1 $2');
	let part: MousePart | null = null;
	let at = Infinity;
	for (const [re, p] of PART_WORDS) {
		const m = re.exec(text);
		if (m && m.index < at) {
			part = p;
			at = m.index;
		}
	}
	let hint: MouseHint = null;
	const after = part === null ? text : text.slice(at).replace(/^\S+/, '');
	const dir = /\b(up|down|left|right)\b/i.exec(after);
	if (dir && dir[1]) hint = dir[1].toLowerCase() as MouseHint;
	return { part, hint };
}

/* ------------------------------------------------------------------------ */
/* Labels                                                                    */
/* ------------------------------------------------------------------------ */

/** "x4" style repeat badge text with the multiplication sign. */
export function repeatText(count: number): string | null {
	return Number.isFinite(count) && count > 1 ? `×${Math.round(count)}` : null;
}

/** Typed text shown in the readout: the end of a long run, with an ellipsis. */
export const TEXT_RUN_MAX = 40;
export function textRunDisplay(text: string, max = TEXT_RUN_MAX): string {
	const chars = [...text];
	if (chars.length <= max) return text;
	return `…${chars.slice(chars.length - (max - 1)).join('')}`;
}

/** A label is "short" when it is one character (A, 8, ;): drawn larger. */
export function isShortLabel(label: string): boolean {
	return [...label].length <= 1;
}

/**
 * Plain-text form of a callout, for accessible labels (never logged).
 * Mirrors keystage-core's `Callout::text_form`: "Ctrl + Shift + S",
 * "Alt, F, C", "Ctrl + Z x4", "25.4 Enter".
 */
export function calloutText(c: Callout): string {
	const steps = c.steps.map((s) => s.map((chip) => chip.label).join(' + ')).filter((s) => s);
	let out: string;
	if (c.kind === 'text' || c.kind === 'masked') {
		const base = c.kind === 'text' ? (c.text ?? '') : 'Typing';
		out = [base, ...steps].filter((s) => s.length > 0).join(' ');
	} else {
		out = steps.join(', ');
	}
	if (c.count > 1) out += ` x${c.count}`;
	return out;
}
