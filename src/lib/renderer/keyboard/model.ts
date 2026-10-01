/**
 * Pure logic behind the keyboard visual: which drawn key a key message
 * lights, the legend each key shows, and the decay of a released key.
 * No DOM, no Svelte, an injected clock: tested in model.test.ts.
 */
import type { Legends, Modifier } from '../../protocol';
import type { KeyState, KeyStates } from '../logic/inputs';
import { contrastRatio, parseColor } from '../../themes/color';
import { SHARED_CODES, isNoScanId, type KeyDef, type LayoutDef, type KeyLegend } from './layouts';

/* ------------------------------------------------------------------------ */
/* Resolving key messages to drawn keys                                       */
/* ------------------------------------------------------------------------ */

export interface LayoutIndex {
	layout: LayoutDef;
	/** Every id that lights a key, to the keys it may light (two for a shared code). */
	byCode: ReadonlyMap<string, readonly KeyDef[]>;
	/** Canonical name to the first key (in reading order) that reports it. */
	byName: ReadonlyMap<string, KeyDef>;
}

const indexCache = new WeakMap<LayoutDef, LayoutIndex>();

export function indexLayout(layout: LayoutDef): LayoutIndex {
	const hit = indexCache.get(layout);
	if (hit) return hit;
	const byCode = new Map<string, KeyDef[]>();
	const byName = new Map<string, KeyDef>();
	for (const k of layout.keys) {
		for (const c of k.codes) {
			const list = byCode.get(c);
			if (list) list.push(k);
			else byCode.set(c, [k]);
		}
		for (const n of k.names) if (!byName.has(n)) byName.set(n, k);
	}
	const index: LayoutIndex = { layout, byCode, byName };
	indexCache.set(layout, index);
	return index;
}

/** Normalizes a wire id to the core's spelling ("sc:e01d" to "sc:E01D"). */
export function normalizePhys(phys: string): string {
	const m = /^sc:([0-9a-fA-F]{1,4})$/.exec(phys.trim());
	return m && m[1] ? `sc:${m[1].toUpperCase()}` : phys;
}

/**
 * The drawn key a key message lights, or null when the layout has no such
 * key (a numpad key on a tenkeyless drawing, for example).
 *
 * 1. The physical id decides. A code two keys share (sc:45, Pause or Num
 *    Lock) is resolved by the canonical name, and lights nothing when the
 *    name matches neither.
 * 2. Input with no scan code (injected keys) falls back to the name: first a
 *    character key whose current legend is that name, then the first key
 *    that reports the name.
 * Character keys are never matched by name otherwise, because on a non-US
 * layout the name (from the virtual key) and the physical key differ.
 */
export function resolveKey(
	index: LayoutIndex,
	phys: string,
	name: string,
	legends: Legends | null = null
): KeyDef | null {
	const id = normalizePhys(phys);
	const candidates = index.byCode.get(id);
	if (candidates && candidates.length > 0) {
		if (SHARED_CODES[id] || candidates.length > 1) {
			return candidates.find((k) => k.names.includes(name)) ?? null;
		}
		return candidates[0] ?? null;
	}
	if (!isNoScanId(id)) return null;
	const wanted = name.trim().toUpperCase();
	if (wanted.length > 0) {
		for (const k of index.layout.keys) {
			if (!k.char) continue;
			const shown = legendFor(k, legends);
			if (shown.text.toUpperCase() === wanted) return k;
		}
	}
	return index.byName.get(name) ?? null;
}

/** What one drawn key shows. */
export interface KeyLight {
	down: boolean;
	pressedAt: number;
	releasedAt: number | null;
	/** The modifier hue to light it in, or null for the press color. */
	modifier: Modifier | null;
	/** The canonical name of the latest message (AltGr on a right Alt). */
	name: string;
}

function later(a: KeyLight, s: KeyState): boolean {
	const at = s.releasedAt ?? s.pressedAt;
	const bt = a.releasedAt ?? a.pressedAt;
	return at >= bt;
}

/**
 * Lit keys by drawn key id. Several messages that land on one key (a shared
 * code, or a folded right modifier in the left-hand zone) merge: held if any
 * is held, otherwise the latest release.
 */
export function lightsFor(
	index: LayoutIndex,
	keys: KeyStates,
	legends: Legends | null
): Map<string, KeyLight> {
	const out = new Map<string, KeyLight>();
	for (const s of Object.values(keys)) {
		const def = resolveKey(index, s.phys, s.key, legends);
		if (!def) continue;
		const modifier = s.modifier ?? def.mod;
		const prev = out.get(def.id);
		if (!prev) {
			out.set(def.id, {
				down: s.down,
				pressedAt: s.pressedAt,
				releasedAt: s.down ? null : s.releasedAt,
				modifier,
				name: s.key,
			});
			continue;
		}
		if (s.down && !prev.down) {
			out.set(def.id, {
				down: true,
				pressedAt: s.pressedAt,
				releasedAt: null,
				modifier,
				name: s.key,
			});
		} else if (s.down === prev.down && later(prev, s)) {
			out.set(def.id, {
				down: s.down,
				pressedAt: s.pressedAt,
				releasedAt: s.down ? null : s.releasedAt,
				modifier,
				name: s.key,
			});
		}
	}
	return out;
}

/* ------------------------------------------------------------------------ */
/* Legends                                                                    */
/* ------------------------------------------------------------------------ */

/** Longest legend drawn from a legends message (graphemes). */
export const LEGEND_MAX = 3;

function clip(text: string): string {
	const chars = [...text.trim()];
	return chars.length <= LEGEND_MAX ? chars.join('') : chars.slice(0, LEGEND_MAX).join('');
}

/**
 * The legend a key shows. Character keys follow the legends message for the
 * active Windows layout; a character key the message leaves out shows no
 * character (that layout types nothing there). Every other key keeps its US
 * legend, as does every key before any legends message.
 */
export function legendFor(key: KeyDef, legends: Legends | null): KeyLegend {
	if (!key.char || !legends || key.legend.kind !== 'char') return key.legend;
	const raw = legends.legends[key.id];
	if (raw === undefined) return { text: '', kind: 'none' };
	const text = clip(raw);
	return text.length > 0 ? { text, kind: 'char' } : { text: '', kind: 'none' };
}

/** The small corner legend (the backtick on a 60 percent Esc), or null. */
export function subLegendFor(
	key: KeyDef,
	legends: Legends | null,
	usLegend: (id: string) => string | null
): string | null {
	if (!key.sub) return null;
	if (legends) {
		const raw = legends.legends[key.sub];
		return raw === undefined ? null : clip(raw) || null;
	}
	return usLegend(key.sub);
}

/* ------------------------------------------------------------------------ */
/* Decay                                                                      */
/* ------------------------------------------------------------------------ */

/**
 * How the lit layer of a key behaves from `now`:
 *   hold: fully lit (held down)
 *   off: not lit
 *   fade: lit at `from` now, fading linearly to 0 over `duration` ms
 * Matches keyIntensity (logic/inputs.ts): 1 while held, linear to 0 over
 * decayMs after the release.
 */
export type DecayPlan =
	{ kind: 'hold' } | { kind: 'off' } | { kind: 'fade'; from: number; duration: number };

export function decayPlan(
	light: Pick<KeyLight, 'down' | 'releasedAt'> | null | undefined,
	now: number,
	decayMs: number
): DecayPlan {
	if (!light) return { kind: 'off' };
	if (light.down) return { kind: 'hold' };
	if (light.releasedAt === null) return { kind: 'off' };
	if (!(decayMs > 0)) return { kind: 'off' };
	const elapsed = Math.max(0, now - light.releasedAt);
	if (elapsed >= decayMs) return { kind: 'off' };
	return { kind: 'fade', from: 1 - elapsed / decayMs, duration: decayMs - elapsed };
}

/** Opacity of the lit layer at `now` (what the fade shows at that moment). */
export function litOpacity(
	light: Pick<KeyLight, 'down' | 'releasedAt'> | null | undefined,
	now: number,
	decayMs: number
): number {
	const p = decayPlan(light, now, decayMs);
	return p.kind === 'hold' ? 1 : p.kind === 'off' ? 0 : p.from;
}

/** A signature of the inputs to decayPlan, so an unchanged key is not restarted. */
export function decaySignature(
	light: Pick<KeyLight, 'down' | 'releasedAt' | 'pressedAt'> | null | undefined,
	decayMs: number
): string {
	if (!light) return 'off';
	return `${light.down ? 'd' : 'u'}:${light.pressedAt}:${light.releasedAt ?? '-'}:${decayMs}`;
}

/* ------------------------------------------------------------------------ */
/* Ink on a modifier hue                                                      */
/* ------------------------------------------------------------------------ */

export const DARK_INK = '#0b0d0c';
export const LIGHT_INK = '#ffffff';

/**
 * The legend ink on a lit modifier key: dark or white, whichever reads
 * better on that modifier's hue. Null when the hue is not a color (the
 * caller then uses the theme's pressed ink).
 */
export function inkOn(hue: string): string | null {
	const c = parseColor(hue.trim());
	if (!c) return null;
	const opaque = { ...c, a: 1 };
	const dark = parseColor(DARK_INK);
	const light = parseColor(LIGHT_INK);
	if (!dark || !light) return null;
	return contrastRatio(dark, opaque) >= contrastRatio(light, opaque) ? DARK_INK : LIGHT_INK;
}

export const MODIFIER_TOKENS: Readonly<Record<Modifier, string>> = {
	ctrl: '--ks-ctrl',
	shift: '--ks-shift',
	alt: '--ks-alt',
	win: '--ks-win',
};
