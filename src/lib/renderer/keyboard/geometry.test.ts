import { describe, expect, it } from 'vitest';
import { KEYBOARD_FOOTPRINTS } from '../metrics';
import { LAYOUT_IDS, getLayout } from './layouts';
import {
	HOUSING_PAD,
	contentOffset,
	housingOutline,
	insetOrtho,
	placePoly,
	polygonArea,
	polygonRects,
	roundedPath,
	uniform,
	type Pt,
} from './geometry';

const rect: Pt[] = [
	[0, 0],
	[10, 0],
	[10, 4],
	[0, 4],
];

/** The ISO Enter shape: 1.5 wide on top, 1.25 below, flush right. */
const isoEnter: Pt[] = [
	[0, 0],
	[1.5, 0],
	[1.5, 2],
	[0.25, 2],
	[0.25, 1],
	[0, 1],
];

function inside(rects: { x: number; y: number; w: number; h: number }[], x: number, y: number) {
	return rects.some(
		(r) => x >= r.x - 1e-9 && x <= r.x + r.w + 1e-9 && y >= r.y - 1e-9 && y <= r.y + r.h + 1e-9
	);
}

describe('insetOrtho', () => {
	it('moves each side of a rectangle by its own amount', () => {
		expect(insetOrtho(rect, { top: 1, right: 2, bottom: 0.5, left: 3 })).toEqual([
			[3, 1],
			[8, 1],
			[8, 3.5],
			[3, 3.5],
		]);
		expect(insetOrtho(rect, uniform(-1))).toEqual([
			[-1, -1],
			[11, -1],
			[11, 5],
			[-1, 5],
		]);
	});

	it('insets the ISO Enter, concave corner included', () => {
		const d = 0.1;
		const out = insetOrtho(isoEnter, uniform(d));
		const expected: Pt[] = [
			[0.1, 0.1],
			[1.4, 0.1],
			[1.4, 1.9],
			[0.35, 1.9],
			[0.35, 0.9],
			[0.1, 0.9],
		];
		out.forEach((p, i) => {
			expect(p[0]).toBeCloseTo((expected[i] as Pt)[0], 9);
			expect(p[1]).toBeCloseTo((expected[i] as Pt)[1], 9);
		});
	});

	it('treats the bottom of an overhang as a bottom edge', () => {
		// The underside of the ISO Enter's top part takes the bottom inset (its front wall).
		const out = insetOrtho(isoEnter, { top: 0, right: 0, bottom: 0.2, left: 0 });
		expect(out[4]).toEqual([0.25, 0.8]);
		expect(out[5]).toEqual([0, 0.8]);
		expect(out[2]).toEqual([1.5, 1.8]);
	});
});

describe('roundedPath', () => {
	it('rounds every corner with an arc and closes', () => {
		const d = roundedPath(rect, 1);
		expect(d.startsWith('M')).toBe(true);
		expect(d.endsWith('Z')).toBe(true);
		expect(d.match(/A/g)?.length).toBe(4);
		expect(d).toContain('M0 1');
		expect(d).toContain('A1 1 0 0 1 1 0');
	});

	it('clamps the radius to half the shortest edge', () => {
		expect(roundedPath(rect, 50)).toContain('A2 2 0 0 1');
		expect(roundedPath(rect, 0)).not.toContain('A');
	});

	it('sweeps concave corners the other way', () => {
		const d = roundedPath(isoEnter, 0.1);
		const sweeps = [...d.matchAll(/A[\d.]+ [\d.]+ 0 0 ([01])/g)].map((m) => m[1]);
		expect(sweeps).toEqual(['1', '1', '1', '1', '0', '1']);
	});

	it('returns nothing for a degenerate polygon', () => {
		expect(roundedPath([[0, 0]], 1)).toBe('');
	});
});

describe('polygon helpers', () => {
	it('measures area and tiles the polygon with rectangles', () => {
		expect(polygonArea(rect)).toBe(40);
		expect(polygonArea(isoEnter)).toBeCloseTo(2.75, 9);
		const rs = polygonRects(isoEnter);
		expect(rs).toEqual([
			{ x: 0, y: 0, w: 1.5, h: 1 },
			{ x: 0.25, y: 1, w: 1.25, h: 1 },
		]);
		expect(placePoly(rect, 2, 5, 6)[2]).toEqual([25, 14]);
	});
});

describe('the housing', () => {
	it('is a padded rectangle for every layout but the left-hand zone', () => {
		for (const id of LAYOUT_IDS.filter((l) => l !== 'left')) {
			const l = getLayout(id);
			const h = housingOutline(l);
			expect(h).toEqual([
				[-HOUSING_PAD, -HOUSING_PAD],
				[l.width + HOUSING_PAD, -HOUSING_PAD],
				[l.width + HOUSING_PAD, l.height + HOUSING_PAD],
				[-HOUSING_PAD, l.height + HOUSING_PAD],
			]);
		}
	});

	it('follows the rows of the left-hand zone and encloses every key', () => {
		for (const iso of [false, true]) {
			const l = getLayout('left', iso);
			const h = housingOutline(l);
			expect(h.length).toBe(8);
			expect(polygonArea(h)).toBeGreaterThan(0);
			const rs = polygonRects(h);
			for (const k of l.keys) {
				for (const [x, y] of k.outline) expect(inside(rs, x, y), `${k.id} ${x},${y}`).toBe(true);
			}
			// The middle rows are narrower than the function row and the Space row.
			const xs = h.map((p) => p[0]);
			expect(Math.max(...xs)).toBeCloseTo(10.5 + HOUSING_PAD, 9);
			expect(new Set(xs.map((x) => x.toFixed(3))).size).toBe(4);
		}
	});

	it('centers the keys in the footprint', () => {
		for (const id of LAYOUT_IDS) {
			const l = getLayout(id);
			const fp = KEYBOARD_FOOTPRINTS[id];
			const off = contentOffset(l, fp);
			expect(off.x * 2 + l.width).toBeCloseTo(fp.w, 9);
			expect(off.y * 2 + l.height).toBeCloseTo(fp.h, 9);
			expect(off.x).toBeGreaterThanOrEqual(HOUSING_PAD);
			expect(off.y).toBeGreaterThanOrEqual(HOUSING_PAD);
		}
	});
});
