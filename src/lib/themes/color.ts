/**
 * Small color toolkit: parsing, WCAG 2.x contrast, compositing and
 * lightness-only adjustment (the house rule ported from IDEA Classroom: a
 * derived ink keeps its hue and saturation and moves in lightness only).
 *
 * Used by the custom theme (src/lib/themes/themes.ts), by the contrast tests
 * and by the settings window to warn about low-contrast custom colors. No
 * CSS color-mix() is used anywhere because OBS 30 embeds Chromium 103.
 */

export interface Rgba {
	r: number;
	g: number;
	b: number;
	/** 0..1 */
	a: number;
}

const HEX = /^#([0-9a-f]{3,4}|[0-9a-f]{6}|[0-9a-f]{8})$/i;
const RGB = /^rgba?\(\s*([^)]+)\)$/i;

function clamp(n: number, lo: number, hi: number): number {
	return Math.min(hi, Math.max(lo, n));
}

/** Parses #rgb, #rgba, #rrggbb, #rrggbbaa, rgb() and rgba(). Null if invalid. */
export function parseColor(input: string): Rgba | null {
	const s = input.trim();
	const hex = HEX.exec(s);
	if (hex && hex[1]) {
		let h = hex[1];
		if (h.length <= 4) h = [...h].map((c) => c + c).join('');
		const n = (i: number) => parseInt(h.slice(i, i + 2), 16);
		return { r: n(0), g: n(2), b: n(4), a: h.length === 8 ? n(6) / 255 : 1 };
	}
	const rgb = RGB.exec(s);
	if (rgb && rgb[1]) {
		const parts = rgb[1].split(/[\s,/]+/).filter((p) => p.length > 0);
		if (parts.length < 3 || parts.length > 4) return null;
		const channel = (p: string) => (p.endsWith('%') ? (parseFloat(p) / 100) * 255 : parseFloat(p));
		const [rs, gs, bs, as] = parts as [string, string, string, string | undefined];
		const r = channel(rs);
		const g = channel(gs);
		const b = channel(bs);
		let a = 1;
		if (as !== undefined) a = as.endsWith('%') ? parseFloat(as) / 100 : parseFloat(as);
		if ([r, g, b, a].some((v) => !Number.isFinite(v))) return null;
		return { r: clamp(r, 0, 255), g: clamp(g, 0, 255), b: clamp(b, 0, 255), a: clamp(a, 0, 1) };
	}
	return null;
}

export function toHex(c: Rgba): string {
	const h = (v: number) =>
		Math.round(clamp(v, 0, 255))
			.toString(16)
			.padStart(2, '0');
	return `#${h(c.r)}${h(c.g)}${h(c.b)}`;
}

export function toCss(c: Rgba): string {
	if (c.a >= 1) return toHex(c);
	const r = (v: number) => Math.round(clamp(v, 0, 255));
	return `rgba(${r(c.r)}, ${r(c.g)}, ${r(c.b)}, ${Math.round(c.a * 1000) / 1000})`;
}

/** Source-over compositing of `top` onto an opaque `bottom`. */
export function composite(top: Rgba, bottom: Rgba): Rgba {
	const a = top.a;
	return {
		r: top.r * a + bottom.r * (1 - a),
		g: top.g * a + bottom.g * (1 - a),
		b: top.b * a + bottom.b * (1 - a),
		a: 1,
	};
}

/** Composites a stack of layers (bottom first) into one opaque color. */
export function flatten(layers: readonly Rgba[]): Rgba {
	let out: Rgba = { r: 0, g: 0, b: 0, a: 1 };
	let first = true;
	for (const layer of layers) {
		if (first) {
			out = layer.a >= 1 ? layer : composite(layer, { r: 0, g: 0, b: 0, a: 1 });
			first = false;
		} else {
			out = composite(layer, out);
		}
	}
	return out;
}

function lin(v: number): number {
	const s = v / 255;
	return s <= 0.04045 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
}

/** WCAG 2.x relative luminance of an opaque color. */
export function luminance(c: Rgba): number {
	return 0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b);
}

/** WCAG 2.x contrast ratio of two opaque colors (1..21). */
export function contrastRatio(a: Rgba, b: Rgba): number {
	const la = luminance(a);
	const lb = luminance(b);
	const hi = Math.max(la, lb);
	const lo = Math.min(la, lb);
	return (hi + 0.05) / (lo + 0.05);
}

/** Contrast of two CSS color strings; translucent inputs are composited on `ground`. */
export function contrastOf(fg: string, bg: string, ground = '#000000'): number {
	const g = parseColor(ground);
	const f = parseColor(fg);
	const b = parseColor(bg);
	if (!g || !f || !b) return 1;
	const bgFlat = composite(b, g);
	const fgFlat = composite(f, bgFlat);
	return contrastRatio(fgFlat, bgFlat);
}

/* ------------------------------------------------------------------------ */
/* HSL, lightness-only moves                                                 */
/* ------------------------------------------------------------------------ */

interface Hsl {
	h: number;
	s: number;
	l: number;
}

function toHsl(c: Rgba): Hsl {
	const r = c.r / 255;
	const g = c.g / 255;
	const b = c.b / 255;
	const max = Math.max(r, g, b);
	const min = Math.min(r, g, b);
	const l = (max + min) / 2;
	if (max === min) return { h: 0, s: 0, l };
	const d = max - min;
	const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
	let h: number;
	if (max === r) h = (g - b) / d + (g < b ? 6 : 0);
	else if (max === g) h = (b - r) / d + 2;
	else h = (r - g) / d + 4;
	return { h: h * 60, s, l };
}

function fromHsl(hsl: Hsl, a = 1): Rgba {
	const { h, s, l } = hsl;
	if (s === 0) return { r: l * 255, g: l * 255, b: l * 255, a };
	const q = l < 0.5 ? l * (1 + s) : l + s - l * s;
	const p = 2 * l - q;
	const hue = (t: number) => {
		let x = t;
		if (x < 0) x += 1;
		if (x > 1) x -= 1;
		if (x < 1 / 6) return p + (q - p) * 6 * x;
		if (x < 1 / 2) return q;
		if (x < 2 / 3) return p + (q - p) * (2 / 3 - x) * 6;
		return p;
	};
	const hh = h / 360;
	return { r: hue(hh + 1 / 3) * 255, g: hue(hh) * 255, b: hue(hh - 1 / 3) * 255, a };
}

/** Moves lightness by `delta` (-1..1), keeping hue and saturation. */
export function shiftLightness(c: Rgba, delta: number): Rgba {
	const hsl = toHsl(c);
	return fromHsl({ ...hsl, l: clamp(hsl.l + delta, 0, 1) }, c.a);
}

/** Linear mix in sRGB, `t` = share of `b`. */
export function mix(a: Rgba, b: Rgba, t: number): Rgba {
	const k = clamp(t, 0, 1);
	return {
		r: a.r + (b.r - a.r) * k,
		g: a.g + (b.g - a.g) * k,
		b: a.b + (b.b - a.b) * k,
		a: a.a + (b.a - a.a) * k,
	};
}

export function withAlpha(c: Rgba, a: number): Rgba {
	return { ...c, a: clamp(a, 0, 1) };
}

/**
 * Returns `fg` with its lightness moved (hue and saturation held) until it
 * reaches `min` contrast against every color in `grounds`. Tries the lighter
 * and the darker direction and keeps the smaller move; falls back to white or
 * black when no lightness of that hue is enough.
 */
export function ensureContrast(fg: Rgba, grounds: readonly Rgba[], min: number): Rgba {
	const worst = (c: Rgba) => Math.min(...grounds.map((g) => contrastRatio(c, g)));
	const opaque = { ...fg, a: 1 };
	if (worst(opaque) >= min) return opaque;
	const base = toHsl(opaque);
	let best: Rgba | null = null;
	let bestMove = Infinity;
	for (const dir of [1, -1]) {
		for (let step = 1; step <= 100; step++) {
			const l = base.l + dir * step * 0.01;
			if (l < 0 || l > 1) break;
			const cand = fromHsl({ ...base, l });
			if (worst(cand) >= min) {
				if (step < bestMove) {
					best = cand;
					bestMove = step;
				}
				break;
			}
		}
	}
	if (best) return best;
	const white: Rgba = { r: 255, g: 255, b: 255, a: 1 };
	const black: Rgba = { r: 0, g: 0, b: 0, a: 1 };
	return worst(white) >= worst(black) ? white : black;
}
