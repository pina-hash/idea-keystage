import { describe, expect, it } from 'vitest';
import {
	composite,
	contrastOf,
	contrastRatio,
	ensureContrast,
	flatten,
	mix,
	parseColor,
	shiftLightness,
	toCss,
	toHex,
} from './color';

describe('color', () => {
	it('parses hex and rgb forms', () => {
		expect(parseColor('#fff')).toEqual({ r: 255, g: 255, b: 255, a: 1 });
		expect(parseColor('#78b870')).toEqual({ r: 120, g: 184, b: 112, a: 1 });
		expect(parseColor('#00000080')?.a).toBeCloseTo(0.502, 2);
		expect(parseColor('rgba(255, 255, 255, 0.5)')).toEqual({ r: 255, g: 255, b: 255, a: 0.5 });
		expect(parseColor('rgb(10 20 30 / 50%)')).toEqual({ r: 10, g: 20, b: 30, a: 0.5 });
		expect(parseColor('green')).toBeNull();
		expect(parseColor('#12')).toBeNull();
		expect(parseColor('rgb(1,2)')).toBeNull();
	});

	it('computes WCAG contrast', () => {
		const w = parseColor('#ffffff');
		const b = parseColor('#000000');
		if (!w || !b) throw new Error('parse');
		expect(contrastRatio(w, b)).toBeCloseTo(21, 5);
		expect(contrastRatio(w, w)).toBe(1);
		expect(contrastOf('#767676', '#ffffff')).toBeCloseTo(4.54, 2);
		expect(contrastOf('bogus', '#fff')).toBe(1);
	});

	it('composites translucent layers', () => {
		const white = { r: 255, g: 255, b: 255, a: 1 };
		const half = { r: 0, g: 0, b: 0, a: 0.5 };
		expect(toHex(composite(half, white))).toBe('#808080');
		expect(toHex(flatten([white, half, half]))).toBe('#404040');
		expect(toCss({ r: 1, g: 2, b: 3, a: 0.25 })).toBe('rgba(1, 2, 3, 0.25)');
		expect(toHex(mix(white, { r: 0, g: 0, b: 0, a: 1 }, 0.5))).toBe('#808080');
	});

	it('moves lightness only, until the contrast is enough', () => {
		const ink = parseColor('#888888');
		const ground = parseColor('#808080');
		if (!ink || !ground) throw new Error('parse');
		const out = ensureContrast(ink, [ground], 4.5);
		expect(contrastRatio(out, ground)).toBeGreaterThanOrEqual(4.5);
		const green = parseColor('#78b870');
		if (!green) throw new Error('parse');
		const darker = shiftLightness(green, -0.2);
		expect(darker.g).toBeGreaterThan(darker.r);
		expect(toHex(ensureContrast(green, [parseColor('#000000') ?? green], 4.5))).toBe('#78b870');
	});
});
