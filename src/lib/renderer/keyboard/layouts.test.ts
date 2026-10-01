import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { KEYBOARD_FOOTPRINTS } from '../metrics';
import { DEMO_SCAN_CODES } from '../../demo/scancodes';
import { GLYPHS } from '../glyphs';
import {
	LAYOUT_IDS,
	LEFT_ZONE,
	SHARED_CODES,
	getLayout,
	usLegendOf,
	type KeyDef,
	type LayoutDef,
} from './layouts';
import { HOUSING_PAD, polygonArea, polygonRects } from './geometry';

const ROOT = resolve(__dirname, '../../../..');
const WIRE_ID = /^sc:(E0)?[0-9A-F]{2}$/;

function rectsOf(k: KeyDef) {
	return polygonRects(k.outline);
}

function overlap(a: KeyDef, b: KeyDef): number {
	let area = 0;
	for (const r of rectsOf(a)) {
		for (const s of rectsOf(b)) {
			const w = Math.min(r.x + r.w, s.x + s.w) - Math.max(r.x, s.x);
			const h = Math.min(r.y + r.h, s.y + s.h) - Math.max(r.y, s.y);
			if (w > 1e-9 && h > 1e-9) area += w * h;
		}
	}
	return area;
}

function byId(l: LayoutDef, id: string): KeyDef {
	const k = l.keys.find((x) => x.id === id);
	if (!k) throw new Error(`${l.id}: no key ${id}`);
	return k;
}

/** The character block scan codes the core names (keys.rs CHARACTER_SCANS). */
function coreCharacterScans(): string[] {
	const path = resolve(ROOT, 'crates/keystage-core/src/keys.rs');
	if (!existsSync(path)) return [];
	const src = readFileSync(path, 'utf8');
	const start = src.indexOf('pub const CHARACTER_SCANS');
	if (start < 0) return [];
	const body = src.slice(src.indexOf('&[', start + 30), src.indexOf('];', start));
	return [...body.matchAll(/0x([0-9A-Fa-f]{2})\b/g)].map(
		(m) => `sc:${(m[1] as string).toUpperCase()}`
	);
}

const VARIANTS = LAYOUT_IDS.flatMap((id) => [false, true].map((iso) => ({ id, iso })));

describe('every layout', () => {
	for (const { id, iso } of VARIANTS) {
		const name = `${id} ${iso ? 'ISO' : 'ANSI'}`;
		const layout = getLayout(id, iso);

		it(`${name}: no two keys overlap`, () => {
			const keys = layout.keys;
			for (let i = 0; i < keys.length; i++) {
				for (let j = i + 1; j < keys.length; j++) {
					const a = keys[i] as KeyDef;
					const b = keys[j] as KeyDef;
					expect(overlap(a, b), `${a.id} and ${b.id}`).toBe(0);
				}
			}
		});

		it(`${name}: every key has a scan code, a name and a legend`, () => {
			expect(layout.keys.length).toBeGreaterThan(30);
			for (const k of layout.keys) {
				expect(k.codes.length, k.id).toBeGreaterThan(0);
				expect(k.codes[0]).toBe(k.id);
				for (const c of k.codes) expect(c, k.id).toMatch(WIRE_ID);
				expect(k.names.length, k.id).toBeGreaterThan(0);
				expect(k.legend.text.trim().length, k.id).toBeGreaterThan(0);
				if (k.legend.kind === 'glyph') expect(GLYPHS[k.legend.glyph ?? 'up']).toBeDefined();
				expect(k.w).toBeGreaterThan(0);
				expect(k.h).toBeGreaterThan(0);
				expect(polygonArea(k.outline), `${k.id} winds clockwise`).toBeGreaterThan(0);
			}
		});

		it(`${name}: ids are unique and a code lights one key (the shared code is told apart by name)`, () => {
			const ids = layout.keys.map((k) => k.id);
			expect(new Set(ids).size).toBe(ids.length);
			const owners = new Map<string, KeyDef[]>();
			for (const k of layout.keys) {
				for (const c of k.codes) owners.set(c, [...(owners.get(c) ?? []), k]);
			}
			for (const [code, keys] of owners) {
				if (keys.length === 1) continue;
				expect(
					SHARED_CODES[code],
					`${code} is on ${keys.map((k) => k.id).join(', ')}`
				).toBeDefined();
				const names = keys.flatMap((k) => k.names);
				expect(new Set(names).size, `${code} names`).toBe(names.length);
			}
		});

		it(`${name}: fits its footprint with the housing`, () => {
			const fp = KEYBOARD_FOOTPRINTS[id];
			expect(layout.width + 2 * HOUSING_PAD).toBeLessThanOrEqual(fp.w);
			expect(layout.height + 2 * HOUSING_PAD).toBeLessThanOrEqual(fp.h);
			for (const k of layout.keys) {
				expect(k.x).toBeGreaterThanOrEqual(0);
				expect(k.y).toBeGreaterThanOrEqual(0);
			}
		});
	}
});

describe('geometry', () => {
	it('uses the standard ANSI widths and row offsets', () => {
		const full = getLayout('full');
		const w = (id: string) => byId(full, id).w;
		expect(w('sc:0F')).toBe(1.5); // Tab
		expect(w('sc:3A')).toBe(1.75); // Caps Lock
		expect(w('sc:2A')).toBe(2.25); // left Shift
		expect(w('sc:36')).toBe(2.75); // right Shift
		expect(w('sc:1C')).toBe(2.25); // Enter
		expect(w('sc:0E')).toBe(2); // Backspace
		expect(w('sc:39')).toBe(6.25); // Space
		expect(w('sc:1D')).toBe(1.25);
		expect(byId(full, 'sc:10').x).toBe(1.5); // Q after Tab
		expect(byId(full, 'sc:1E').x).toBe(1.75); // A after Caps Lock
		expect(byId(full, 'sc:2C').x).toBe(2.25); // Z after Shift
		expect(byId(full, 'sc:4E').h).toBe(2); // numpad +
		expect(byId(full, 'sc:E01C').h).toBe(2); // numpad Enter
		expect(byId(full, 'sc:52').w).toBe(2); // numpad 0
		expect(full.width).toBe(22.5);
		expect(full.height).toBe(6.5);
		// Every row of the alphanumeric block is 15u wide.
		for (const y of [1.5, 2.5, 3.5, 4.5, 5.5]) {
			const row = full.keys.filter((k) => k.y === y && k.x < 15);
			const right = Math.max(...row.map((k) => k.x + k.w));
			const sum = row.reduce((s, k) => s + k.w, 0);
			expect(right, `row ${y}`).toBe(15);
			expect(sum, `row ${y}`).toBe(15);
		}
	});

	it('has 104 keys on the full ANSI layout and 105 on ISO', () => {
		expect(getLayout('full').keys.length).toBe(104);
		expect(getLayout('full', true).keys.length).toBe(105);
		expect(getLayout('tkl').keys.length).toBe(87);
	});

	it('draws the ISO Enter over two rows and adds the 102nd key', () => {
		const iso = getLayout('full', true);
		const enter = byId(iso, 'sc:1C');
		expect(enter.outline.length).toBe(6);
		expect(enter.h).toBe(2);
		expect(polygonArea(enter.outline)).toBeCloseTo(1.5 + 1.25, 6);
		expect(byId(iso, 'sc:2A').w).toBe(1.25);
		expect(byId(iso, 'sc:56').x).toBe(1.25);
		expect(byId(iso, 'sc:2B').y).toBe(byId(iso, 'sc:1E').y); // beside Enter on the home row
		expect(getLayout('full').keys.some((k) => k.id === 'sc:56')).toBe(false);
	});

	it('builds each layout once', () => {
		expect(getLayout('75')).toBe(getLayout('75'));
		expect(getLayout('75', true)).not.toBe(getLayout('75'));
	});
});

describe('the left-hand zone', () => {
	it('is a subset of the full layout, key for key', () => {
		for (const iso of [false, true]) {
			const full = getLayout('full', iso);
			const left = getLayout('left', iso);
			for (const k of left.keys) {
				const f = byId(full, k.id);
				expect([k.x, k.y, k.w, k.h], k.id).toEqual([f.x, f.y, f.w, f.h]);
				expect(k.outline).toEqual(f.outline);
				expect(k.legend).toEqual(f.legend);
				for (const c of f.codes) expect(k.codes).toContain(c);
			}
		}
	});

	it('holds Esc through B, the modifiers, F1 to F8 and Space', () => {
		const left = getLayout('left');
		const ids = new Set(left.keys.map((k) => k.id));
		for (const id of ['sc:01', 'sc:30', 'sc:2C', 'sc:14', 'sc:22', 'sc:07', 'sc:39']) {
			expect(ids.has(id), id).toBe(true);
		}
		for (let s = 0x3b; s <= 0x42; s++)
			expect(ids.has(`sc:${s.toString(16).toUpperCase()}`)).toBe(true);
		for (const id of ['sc:1D', 'sc:2A', 'sc:38', 'sc:E05B', 'sc:3A', 'sc:0F'])
			expect(ids.has(id)).toBe(true);
		expect(ids.has('sc:43')).toBe(false); // F9
		expect(ids.has('sc:31')).toBe(false); // N
		expect(left.keys.length).toBe(LEFT_ZONE.length - 1); // no 102nd key on ANSI
		expect(getLayout('left', true).keys.length).toBe(LEFT_ZONE.length);
	});

	it('lights the left modifiers for the right ones', () => {
		const left = getLayout('left');
		expect(byId(left, 'sc:1D').codes).toContain('sc:E01D');
		expect(byId(left, 'sc:2A').codes).toContain('sc:36');
		expect(byId(left, 'sc:38').codes).toContain('sc:E038');
		expect(byId(left, 'sc:E05B').codes).toContain('sc:E05C');
	});
});

describe('scan codes the core emits', () => {
	const full = getLayout('full');
	const fullIso = getLayout('full', true);
	const codes = new Set(full.keys.flatMap((k) => k.codes));
	const isoCodes = new Set(fullIso.keys.flatMap((k) => k.codes));

	it('covers the core character block (keys.rs CHARACTER_SCANS)', () => {
		const scans = coreCharacterScans();
		if (scans.length === 0) return; // core source not present in this checkout
		expect(scans.length).toBe(49);
		for (const s of scans) {
			if (s === 'sc:56') expect(isoCodes.has(s), s).toBe(true);
			else expect(codes.has(s), s).toBe(true);
		}
	});

	it('covers every modifier the core reports (modifier_phys)', () => {
		for (const id of [
			'sc:1D',
			'sc:E01D',
			'sc:2A',
			'sc:36',
			'sc:38',
			'sc:E038',
			'sc:E05B',
			'sc:E05C',
		]) {
			expect(codes.has(id), id).toBe(true);
		}
	});

	it('covers the function keys, navigation, numpad and lock keys from the Microsoft table', () => {
		const table = [
			'sc:01',
			'sc:3B',
			'sc:3C',
			'sc:3D',
			'sc:3E',
			'sc:3F',
			'sc:40',
			'sc:41',
			'sc:42',
			'sc:43',
			'sc:44',
			'sc:57',
			'sc:58',
			'sc:E037',
			'sc:54',
			'sc:46',
			'sc:45',
			'sc:E046',
			'sc:E052',
			'sc:E047',
			'sc:E049',
			'sc:E053',
			'sc:E04F',
			'sc:E051',
			'sc:E048',
			'sc:E04B',
			'sc:E050',
			'sc:E04D',
			'sc:E045',
			'sc:E035',
			'sc:37',
			'sc:4A',
			'sc:4E',
			'sc:E01C',
			'sc:47',
			'sc:48',
			'sc:49',
			'sc:4B',
			'sc:4C',
			'sc:4D',
			'sc:4F',
			'sc:50',
			'sc:51',
			'sc:52',
			'sc:53',
			'sc:1C',
			'sc:0E',
			'sc:0F',
			'sc:3A',
			'sc:39',
			'sc:E05D',
		];
		for (const id of table) expect(codes.has(id), id).toBe(true);
	});

	it('covers every key the demo scripts press', () => {
		for (const [name, id] of Object.entries(DEMO_SCAN_CODES)) {
			expect(codes.has(id), `${name} ${id}`).toBe(true);
		}
	});

	it('covers the core protocol fixtures', () => {
		const dir = resolve(ROOT, 'crates/keystage-core/tests/fixtures/protocol');
		if (!existsSync(dir)) return;
		for (const f of readdirSync(dir).filter((n) => n.endsWith('.json'))) {
			const msg = JSON.parse(readFileSync(resolve(dir, f), 'utf8')) as {
				t?: string;
				phys?: string;
			};
			if (msg.t === 'key' && typeof msg.phys === 'string')
				expect(codes.has(msg.phys), f).toBe(true);
		}
	});
});

describe('US legends', () => {
	it('default to the US layout', () => {
		const full = getLayout('full');
		expect(byId(full, 'sc:1E').legend.text).toBe('A');
		expect(byId(full, 'sc:27').legend.text).toBe(';');
		expect(byId(full, 'sc:29').legend.text).toBe('`');
		expect(byId(full, 'sc:2B').legend.text).toBe('\\');
		expect(byId(full, 'sc:1E').char).toBe(true);
		expect(byId(full, 'sc:3B').char).toBe(false);
		expect(usLegendOf('sc:56')).toBe('\\');
		expect(usLegendOf('sc:E0FF')).toBeNull();
	});

	it('puts the backtick in the corner of the 60 percent Esc key', () => {
		const sixty = getLayout('60');
		const esc = byId(sixty, 'sc:01');
		expect(esc.codes).toEqual(['sc:01', 'sc:29']);
		expect(esc.sub).toBe('sc:29');
		expect(esc.x).toBe(0);
		expect(sixty.keys.some((k) => k.id === 'sc:29')).toBe(false);
	});
});
