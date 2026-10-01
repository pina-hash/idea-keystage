import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import type { Legends, Modifier } from '../../protocol';
import { MODIFIERS } from '../../protocol';
import { contrastRatio, parseColor } from '../../themes/color';
import { FILE_THEMES, customThemeVars } from '../../themes/themes';
import { applyKey, keyIntensity, type KeyState, type KeyStates } from '../logic/inputs';
import { getLayout, usLegendOf } from './layouts';
import {
	DARK_INK,
	LIGHT_INK,
	MODIFIER_TOKENS,
	decayPlan,
	decaySignature,
	indexLayout,
	inkOn,
	legendFor,
	lightsFor,
	litOpacity,
	normalizePhys,
	resolveKey,
	subLegendFor,
} from './model';

const full = indexLayout(getLayout('full'));
const tkl = indexLayout(getLayout('tkl'));

function id(index = full, phys: string, name: string, legends: Legends | null = null) {
	return resolveKey(index, phys, name, legends)?.id ?? null;
}

function state(
	phys: string,
	key: string,
	down: boolean,
	at: number,
	mod: Modifier | null = null
): KeyState {
	return { phys, key, modifier: mod, down, pressedAt: at, releasedAt: down ? null : at + 50 };
}

describe('resolving key messages to drawn keys', () => {
	it('lights the physical key the scan code names', () => {
		expect(id(full, 'sc:1E', 'A')).toBe('sc:1E');
		expect(id(full, 'sc:1D', 'Ctrl')).toBe('sc:1D');
		expect(id(full, 'sc:E01D', 'Ctrl')).toBe('sc:E01D');
		expect(id(full, 'sc:e01d', 'Ctrl')).toBe('sc:E01D');
		expect(id(full, 'sc:36', 'Shift')).toBe('sc:36');
		expect(id(full, 'sc:E038', 'AltGr')).toBe('sc:E038');
		expect(id(full, 'sc:E047', 'Home')).toBe('sc:E047');
		expect(id(full, 'sc:47', 'NumHome')).toBe('sc:47');
		expect(id(full, 'sc:47', 'Num7')).toBe('sc:47');
		expect(id(full, 'sc:E01C', 'NumEnter')).toBe('sc:E01C');
	});

	it('lights the physical key on a non-US layout whatever the key name says', () => {
		// AZERTY: the key at the US Q position types A (VK_A).
		expect(id(full, 'sc:10', 'A')).toBe('sc:10');
		expect(id(full, 'sc:1E', 'Q')).toBe('sc:1E');
	});

	it('tells Pause from Num Lock on the shared code by name', () => {
		expect(id(full, 'sc:45', 'Pause')).toBe('sc:45');
		expect(id(full, 'sc:45', 'NumLock')).toBe('sc:E045');
		expect(id(full, 'sc:E045', 'NumLock')).toBe('sc:E045');
		expect(id(full, 'sc:E046', 'Pause')).toBe('sc:45');
		expect(id(full, 'sc:45', 'Esc')).toBeNull();
		// No numpad on a tenkeyless drawing: Num Lock lights nothing, never Pause.
		expect(id(tkl, 'sc:45', 'NumLock')).toBeNull();
		expect(id(tkl, 'sc:45', 'Pause')).toBe('sc:45');
	});

	it('lights Print Screen for its Alt+Print Screen code too', () => {
		expect(id(full, 'sc:E037', 'PrintScreen')).toBe('sc:E037');
		expect(id(full, 'sc:54', 'PrintScreen')).toBe('sc:E037');
		expect(id(full, 'sc:37', 'NumMultiply')).toBe('sc:37');
	});

	it('lights nothing for keys the layout does not draw', () => {
		expect(id(tkl, 'sc:4F', 'Num1')).toBeNull();
		expect(id(full, 'sc:E02A', 'Shift')).toBeNull(); // a fake shift
		expect(id(full, 'sc:76', 'F24')).toBeNull();
		expect(id(full, 'garbage', 'A')).toBeNull();
	});

	it('falls back to the name for input with no scan code', () => {
		expect(id(full, 'sc:00', 'A')).toBe('sc:1E');
		expect(id(full, 'sc:00', 'Ctrl')).toBe('sc:1D');
		expect(id(full, 'sc:E000', 'Up')).toBe('sc:E048');
		expect(id(full, 'sc:00', 'F5')).toBe('sc:3F');
		// On AZERTY an injected A belongs on the key whose legend is A.
		const azerty: Legends = { layout: '0000040C', legends: { 'sc:10': 'A', 'sc:1E': 'Q' } };
		expect(id(full, 'sc:00', 'A', azerty)).toBe('sc:10');
		expect(id(full, 'sc:00', 'Banana')).toBeNull();
	});

	it('folds the right modifiers onto the left ones in the left-hand zone', () => {
		const left = indexLayout(getLayout('left'));
		expect(id(left, 'sc:E01D', 'Ctrl')).toBe('sc:1D');
		expect(id(left, 'sc:36', 'Shift')).toBe('sc:2A');
		expect(id(left, 'sc:E038', 'AltGr')).toBe('sc:38');
		expect(id(left, 'sc:E05C', 'Win')).toBe('sc:E05B');
		expect(id(left, 'sc:31', 'N')).toBeNull();
	});

	it('lights the 60 percent Esc key for the backtick', () => {
		const sixty = indexLayout(getLayout('60'));
		expect(id(sixty, 'sc:29', '`')).toBe('sc:01');
		expect(id(sixty, 'sc:01', 'Esc')).toBe('sc:01');
	});

	it('normalizes wire ids', () => {
		expect(normalizePhys('sc:e05b')).toBe('sc:E05B');
		expect(normalizePhys(' sc:1e ')).toBe('sc:1E');
		expect(normalizePhys('nonsense')).toBe('nonsense');
	});
});

describe('lights', () => {
	it('maps live key states to drawn keys with their hue', () => {
		let keys: KeyStates = {};
		keys = applyKey(
			keys,
			{ t: 'key', phys: 'sc:1D', key: 'Ctrl', down: true, modifier: 'ctrl', t_ms: 1 },
			100
		);
		keys = applyKey(
			keys,
			{ t: 'key', phys: 'sc:2C', key: 'Z', down: true, modifier: null, t_ms: 2 },
			120
		);
		keys = applyKey(
			keys,
			{ t: 'key', phys: 'sc:2C', key: 'Z', down: false, modifier: null, t_ms: 3 },
			200
		);
		const l = lightsFor(full, keys, null);
		expect(l.get('sc:1D')).toEqual({
			down: true,
			pressedAt: 100,
			releasedAt: null,
			modifier: 'ctrl',
			name: 'Ctrl',
		});
		expect(l.get('sc:2C')).toEqual({
			down: false,
			pressedAt: 120,
			releasedAt: 200,
			modifier: null,
			name: 'Z',
		});
		expect(l.size).toBe(2);
	});

	it('takes the modifier from the layout when the message has none', () => {
		const l = lightsFor(full, { 'sc:36': state('sc:36', 'Shift', true, 5) }, null);
		expect(l.get('sc:36')?.modifier).toBe('shift');
	});

	it('merges two messages on one key: held wins, then the latest release', () => {
		const left = indexLayout(getLayout('left'));
		const held = lightsFor(
			left,
			{
				'sc:1D': state('sc:1D', 'Ctrl', false, 10, 'ctrl'),
				'sc:E01D': state('sc:E01D', 'Ctrl', true, 20, 'ctrl'),
			},
			null
		);
		expect(held.get('sc:1D')?.down).toBe(true);
		const released = lightsFor(
			left,
			{
				'sc:1D': state('sc:1D', 'Ctrl', false, 10, 'ctrl'),
				'sc:E01D': state('sc:E01D', 'Ctrl', false, 40, 'ctrl'),
			},
			null
		);
		expect(released.get('sc:1D')?.releasedAt).toBe(90);
		const order2 = lightsFor(
			left,
			{
				'sc:E01D': state('sc:E01D', 'Ctrl', true, 40, 'ctrl'),
				'sc:1D': state('sc:1D', 'Ctrl', false, 10, 'ctrl'),
			},
			null
		);
		expect(order2.get('sc:1D')?.down).toBe(true);
	});

	it('drops messages for keys the layout does not draw', () => {
		expect(lightsFor(tkl, { 'sc:4F': state('sc:4F', 'Num1', true, 1) }, null).size).toBe(0);
	});
});

describe('legends', () => {
	const key = (phys: string) => getLayout('full').keys.find((k) => k.id === phys)!;

	it('uses US legends before a legends message arrives', () => {
		expect(legendFor(key('sc:1E'), null)).toEqual({ text: 'A', kind: 'char' });
		expect(legendFor(key('sc:3B'), null)).toEqual({ text: 'F1', kind: 'fn' });
	});

	it('follows the legends message on character keys (a German layout)', () => {
		const de: Legends = {
			layout: '00000407',
			legends: { 'sc:15': 'Z', 'sc:2C': 'Y', 'sc:0C': 'ß', 'sc:27': 'Ö', 'sc:1E': 'A' },
		};
		expect(legendFor(key('sc:15'), de).text).toBe('Z');
		expect(legendFor(key('sc:2C'), de).text).toBe('Y');
		expect(legendFor(key('sc:0C'), de).text).toBe('ß');
		expect(legendFor(key('sc:27'), de).text).toBe('Ö');
		// A character key the layout has no character for shows none.
		expect(legendFor(key('sc:28'), de)).toEqual({ text: '', kind: 'none' });
		// Non-character keys keep their legend.
		expect(legendFor(key('sc:1D'), de).text).toBe('Ctrl');
		expect(legendFor(key('sc:4F'), de).text).toBe('1');
		expect(legendFor(key('sc:39'), de).text).toBe('Space');
	});

	it('clips long legends and drops empty ones', () => {
		const l: Legends = { layout: 'x', legends: { 'sc:1E': 'ABCDEFG', 'sc:1F': '  ' } };
		expect(legendFor(key('sc:1E'), l).text).toBe('ABC');
		expect(legendFor(key('sc:1F'), l).kind).toBe('none');
	});

	it('draws the corner legend from the message or the US default', () => {
		const esc = getLayout('60').keys.find((k) => k.id === 'sc:01')!;
		expect(subLegendFor(esc, null, usLegendOf)).toBe('`');
		expect(subLegendFor(esc, { layout: 'x', legends: { 'sc:29': '^' } }, usLegendOf)).toBe('^');
		expect(subLegendFor(esc, { layout: 'x', legends: {} }, usLegendOf)).toBeNull();
		expect(subLegendFor(key('sc:1E'), null, usLegendOf)).toBeNull();
	});
});

describe('decay', () => {
	it('holds while down, fades after the release, then is off', () => {
		expect(decayPlan(null, 0, 700)).toEqual({ kind: 'off' });
		expect(decayPlan({ down: true, releasedAt: null }, 0, 700)).toEqual({ kind: 'hold' });
		expect(decayPlan({ down: false, releasedAt: null }, 0, 700)).toEqual({ kind: 'off' });
		expect(decayPlan({ down: false, releasedAt: 1000 }, 1000, 700)).toEqual({
			kind: 'fade',
			from: 1,
			duration: 700,
		});
		const mid = decayPlan({ down: false, releasedAt: 1000 }, 1175, 700);
		expect(mid.kind).toBe('fade');
		if (mid.kind === 'fade') {
			expect(mid.from).toBeCloseTo(0.75, 9);
			expect(mid.duration).toBe(525);
		}
		expect(decayPlan({ down: false, releasedAt: 1000 }, 1700, 700)).toEqual({ kind: 'off' });
		expect(decayPlan({ down: false, releasedAt: 1000 }, 900, 700)).toEqual({
			kind: 'fade',
			from: 1,
			duration: 700,
		});
	});

	it('snaps off at once with no decay time', () => {
		expect(decayPlan({ down: false, releasedAt: 10 }, 10, 0)).toEqual({ kind: 'off' });
		expect(decayPlan({ down: false, releasedAt: 10 }, 10, Number.NaN)).toEqual({ kind: 'off' });
		expect(decayPlan({ down: true, releasedAt: null }, 10, 0)).toEqual({ kind: 'hold' });
	});

	it('matches the keyIntensity of the renderer store at every moment', () => {
		const decay = 700;
		for (let t = -100; t <= 900; t += 25) {
			const k: KeyState = {
				phys: 'sc:1E',
				key: 'A',
				modifier: null,
				down: false,
				pressedAt: 0,
				releasedAt: 0,
			};
			expect(litOpacity(k, t, decay), `t=${t}`).toBeCloseTo(keyIntensity(k, t, decay), 9);
		}
		const held: KeyState = {
			phys: 'sc:1E',
			key: 'A',
			modifier: null,
			down: true,
			pressedAt: 0,
			releasedAt: null,
		};
		expect(litOpacity(held, 5000, decay)).toBe(keyIntensity(held, 5000, decay));
	});

	it('restarts only when the press, the release or the decay time changes', () => {
		const a = { down: false, pressedAt: 5, releasedAt: 9 };
		expect(decaySignature(a, 700)).toBe(decaySignature({ ...a }, 700));
		expect(decaySignature(a, 700)).not.toBe(decaySignature({ ...a, releasedAt: 10 }, 700));
		expect(decaySignature(a, 700)).not.toBe(decaySignature(a, 500));
		expect(decaySignature({ ...a, down: true, releasedAt: null }, 700)).not.toBe(
			decaySignature(a, 700)
		);
		expect(decaySignature(null, 700)).toBe('off');
	});
});

describe('ink on a modifier hue', () => {
	it('picks dark ink on light hues and white on dark ones', () => {
		expect(inkOn('#78b870')).toBe(DARK_INK);
		expect(inkOn('#ffe14d')).toBe(DARK_INK);
		expect(inkOn('#3b6c36')).toBe(LIGHT_INK);
		expect(inkOn('#624697')).toBe(LIGHT_INK);
		expect(inkOn(' rgb(255, 255, 255) ')).toBe(DARK_INK);
		expect(inkOn('not a color')).toBeNull();
		expect(inkOn('')).toBeNull();
	});

	function tokens(theme: string): Record<string, string> {
		const css = readFileSync(resolve(__dirname, `../../themes/${theme}.css`), 'utf8');
		const out: Record<string, string> = {};
		for (const m of css.matchAll(/(--ks-[a-z-]+):\s*([^;]+);/g))
			out[m[1] as string] = (m[2] as string).trim();
		return out;
	}

	it('keeps every lit modifier legend readable (4.5:1) in every theme', () => {
		const sets: [string, Record<string, string>][] = FILE_THEMES.map((t) => [t, tokens(t)]);
		sets.push(['custom', customThemeVars(null).vars]);
		sets.push([
			'custom-light',
			customThemeVars({ accent: '#2266cc', face: '#f4f4f4', ink: '#111111', glow: '#2266cc' }).vars,
		]);
		for (const [name, vars] of sets) {
			for (const m of MODIFIERS) {
				const hue = vars[MODIFIER_TOKENS[m]];
				expect(hue, `${name} ${m}`).toBeDefined();
				const ink = inkOn(hue as string);
				expect(ink, `${name} ${m}`).not.toBeNull();
				const ratio = contrastRatio(parseColor(ink as string)!, {
					...parseColor(hue as string)!,
					a: 1,
				});
				expect(ratio, `${name} ${m} ${hue}`).toBeGreaterThanOrEqual(4.5);
			}
		}
	});

	it('gives each modifier its own hue in every theme', () => {
		for (const t of FILE_THEMES) {
			const v = tokens(t);
			const hues = MODIFIERS.map((m) => (v[MODIFIER_TOKENS[m]] ?? '').toLowerCase());
			expect(new Set(hues).size, t).toBe(4);
		}
	});
});
