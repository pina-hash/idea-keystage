import { describe, expect, it } from 'vitest';
import { KEYBOARD_FOOTPRINTS, KEYBOARD_UNIT_PX } from '../metrics';
import { KEYBOARD_SIZES } from '../../protocol';
import { LAYOUT_IDS, getLayout } from './layouts';
import { KEY, LEGEND, drawKeyboard, legendDraw, subLegendDraw } from './draw';
import { legendFor } from './model';

/** Points of a path (pairs after M, L, H/V handled loosely): checks bounds only. */
function bbox(d: string): { x0: number; y0: number; x1: number; y1: number } {
	const pts: [number, number][] = [];
	for (const m of d.matchAll(/([ML])(-?[\d.]+) (-?[\d.]+)/g))
		pts.push([Number(m[2]), Number(m[3])]);
	for (const m of d.matchAll(/A[\d.]+ [\d.]+ 0 0 [01] (-?[\d.]+) (-?[\d.]+)/g)) {
		pts.push([Number(m[1]), Number(m[2])]);
	}
	return {
		x0: Math.min(...pts.map((p) => p[0])),
		y0: Math.min(...pts.map((p) => p[1])),
		x1: Math.max(...pts.map((p) => p[0])),
		y1: Math.max(...pts.map((p) => p[1])),
	};
}

describe('drawKeyboard', () => {
	for (const id of LAYOUT_IDS) {
		for (const size of KEYBOARD_SIZES) {
			for (const scale of [1, 2]) {
				const unit = KEYBOARD_UNIT_PX[size] * scale;
				it(`${id} at ${size} x${scale}: everything inside the footprint box`, () => {
					const d = drawKeyboard(getLayout(id), unit);
					const fp = KEYBOARD_FOOTPRINTS[id];
					expect(d.width).toBeCloseTo(fp.w * unit, 9);
					expect(d.height).toBeCloseTo(fp.h * unit, 9);
					const h = bbox(d.housing);
					expect(h.x0).toBeGreaterThanOrEqual(0);
					expect(h.y0).toBeGreaterThanOrEqual(0);
					expect(h.x1).toBeLessThanOrEqual(d.width);
					expect(h.y1).toBeLessThanOrEqual(d.height);
					// A margin is left for the plate's shadow.
					expect(h.x0).toBeGreaterThanOrEqual(Math.floor(0.1 * unit));
					for (const k of d.keys) {
						const b = bbox(k.outer);
						expect(b.x0, k.def.id).toBeGreaterThanOrEqual(h.x0);
						expect(b.y0, k.def.id).toBeGreaterThanOrEqual(h.y0);
						expect(b.x1, k.def.id).toBeLessThanOrEqual(h.x1);
						expect(b.y1, k.def.id).toBeLessThanOrEqual(h.y1);
					}
				});
			}
		}
	}

	it('puts key edges on whole pixels and hairlines on half pixels', () => {
		const d = drawKeyboard(getLayout('full'), 38, 1);
		for (const k of d.keys) {
			const o = bbox(k.outer);
			for (const n of [o.x0, o.y0, o.x1, o.y1])
				expect(Number.isInteger(n), `${k.def.id} ${n}`).toBe(true);
			const edge = bbox(k.edge);
			expect(edge.x0 % 1).toBeCloseTo(0.5, 9);
			expect(edge.y0 % 1).toBeCloseTo(0.5, 9);
		}
	});

	it('leaves a gap between neighbors and a wall band at the foot of every key', () => {
		const unit = 38;
		const d = drawKeyboard(getLayout('full'), unit);
		const q = d.keys.find((k) => k.def.id === 'sc:10')!;
		const w = d.keys.find((k) => k.def.id === 'sc:11')!;
		const gap = bbox(w.outer).x0 - bbox(q.outer).x1;
		expect(gap).toBeGreaterThanOrEqual(Math.floor(KEY.gap * unit) - 1);
		expect(gap).toBeLessThanOrEqual(Math.ceil(KEY.gap * unit) + 1);
		const outer = bbox(q.outer);
		const face = bbox(q.face);
		expect(outer.y1 - face.y1).toBe(Math.round(KEY.skirt * unit));
		expect(face.y0).toBe(outer.y0);
	});

	it('sinks a held key into a socket: an even gap between its outline and its face', () => {
		for (const [unit, edge] of [
			[28, 1],
			[38, 1],
			[50, 1],
			[38, 2],
			[76, 1],
		] as const) {
			const d = drawKeyboard(getLayout('full'), unit, edge);
			const k = d.keys.find((x) => x.def.id === 'sc:1E')!;
			const o = bbox(k.outer);
			const f = bbox(k.faceDown);
			const left = f.x0 - o.x0;
			// At least 2px of socket shows inside the lit outline, at every size and edge width.
			expect(left - k.ringWidth, `${unit}px edge ${edge}`).toBeGreaterThanOrEqual(2);
			expect(f.y0 - o.y0).toBeCloseTo(left, 9);
			expect(o.x1 - f.x1).toBeCloseTo(left, 9);
			expect(o.y1 - f.y1).toBeCloseTo(left, 9);
			expect(k.ringWidth).toBeGreaterThanOrEqual(2 * edge);
			expect(k.downDy).toBeGreaterThan(0); // the legend follows the face down
		}
	});

	it('marks only modifier keys', () => {
		const d = drawKeyboard(getLayout('full'), 38);
		for (const k of d.keys) expect(k.marker.length > 0, k.def.id).toBe(k.def.mod !== null);
		expect(d.keys.filter((k) => k.def.mod !== null).length).toBe(8);
	});

	it('draws a stepped housing for the left-hand zone', () => {
		const d = drawKeyboard(getLayout('left'), 38);
		expect(d.housing.match(/A/g)?.length).toBe(8);
		expect(d.housingHi.length).toBeGreaterThan(0);
		expect(d.tab.startsWith('M')).toBe(true);
	});

	it('survives a nonsense unit', () => {
		const d = drawKeyboard(getLayout('60'), Number.NaN);
		expect(d.width).toBeCloseTo(KEYBOARD_FOOTPRINTS['60'].w * 38, 9);
	});
});

describe('legends on keys', () => {
	const at = (unit: number, layout: 'full' | '60' | 'laptop' = 'full') => {
		const d = drawKeyboard(getLayout(layout), unit);
		return (id: string) => d.keys.find((k) => k.def.id === id)!;
	};

	it('fits every legend inside its face at every size', () => {
		for (const id of LAYOUT_IDS) {
			for (const size of KEYBOARD_SIZES) {
				const unit = KEYBOARD_UNIT_PX[size];
				const d = drawKeyboard(getLayout(id), unit);
				for (const k of d.keys) {
					const l = legendDraw(k, legendFor(k.def, null), unit);
					if (!l) continue;
					const box = k.faceBox;
					expect(l.size, k.def.id).toBeGreaterThan(0);
					expect(l.size, k.def.id).toBeLessThanOrEqual(box.h);
					if (l.kind !== 'glyph') {
						const adv = l.mono ? LEGEND.advance.mono + 0.06 : LEGEND.advance.sans;
						expect([...l.text].length * l.size * adv, `${id} ${k.def.id}`).toBeLessThanOrEqual(
							box.w
						);
					}
					expect(l.x).toBeCloseTo(box.x + box.w / 2, 9);
					expect(l.y).toBeCloseTo(box.y + box.h / 2, 9);
				}
			}
		}
	});

	it('draws letters large, function keys smaller and words smallest', () => {
		const k = at(38);
		const a = legendDraw(k('sc:1E'), { text: 'A', kind: 'char' }, 38)!;
		const f1 = legendDraw(k('sc:3B'), { text: 'F1', kind: 'fn' }, 38)!;
		const ctrl = legendDraw(k('sc:1D'), { text: 'Ctrl', kind: 'word' }, 38)!;
		expect(a.size).toBeGreaterThan(f1.size);
		expect(f1.size).toBeGreaterThan(ctrl.size);
		expect(ctrl.mono).toBe(true);
		expect(a.mono).toBe(false);
	});

	it('hides word legends too small to read, never letters or glyphs', () => {
		const k = at(KEYBOARD_UNIT_PX.s);
		expect(legendDraw(k('sc:1D'), { text: 'Ctrl', kind: 'word' }, KEYBOARD_UNIT_PX.s)).toBeNull();
		expect(legendDraw(k('sc:1E'), { text: 'A', kind: 'char' }, KEYBOARD_UNIT_PX.s)).not.toBeNull();
		expect(
			legendDraw(
				k('sc:0E'),
				{ text: 'Backspace', kind: 'glyph', glyph: 'backspace' },
				KEYBOARD_UNIT_PX.s
			)
		).not.toBeNull();
		// At scale 2 the same words are drawn.
		const k2 = at(KEYBOARD_UNIT_PX.s * 2);
		expect(
			legendDraw(k2('sc:1D'), { text: 'Ctrl', kind: 'word' }, KEYBOARD_UNIT_PX.s * 2)
		).not.toBeNull();
	});

	it('draws nothing for an empty legend', () => {
		const k = at(38);
		expect(legendDraw(k('sc:28'), { text: '', kind: 'none' }, 38)).toBeNull();
		expect(legendDraw(k('sc:28'), { text: '', kind: 'char' }, 38)).toBeNull();
	});

	it('keeps half-height laptop arrows inside their face', () => {
		const k = at(38, 'laptop');
		const up = k('sc:E048');
		const l = legendDraw(up, { text: 'Up', kind: 'glyph', glyph: 'up' }, 38)!;
		expect(l.size).toBeLessThanOrEqual(up.faceBox.h);
	});

	it('puts the corner legend at the top right of the face', () => {
		const k = at(38, '60');
		const esc = k('sc:01');
		const s = subLegendDraw(esc, '`', 38)!;
		expect(s.x).toBeGreaterThan(esc.faceBox.x + esc.faceBox.w / 2);
		expect(s.y).toBeLessThan(esc.faceBox.y + esc.faceBox.h / 2);
		expect(subLegendDraw(esc, '', 38)).toBeNull();
		expect(subLegendDraw(esc, '`', 20)).toBeNull();
	});
});
