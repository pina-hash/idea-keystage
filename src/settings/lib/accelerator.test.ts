import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import {
	NAMED_KEYS,
	acceleratorParts,
	captureStep,
	formatAccelerator,
	hasCommandModifier,
	heldModifiers,
	isCanonical,
	isFunctionKey,
	isRegistrable,
	keyFromCode,
	normalizeAccelerator,
	parseAccelerator,
	parseKeyName,
} from './accelerator';

const keysRs = readFileSync(
	resolve(__dirname, '../../../crates/keystage-core/src/keys.rs'),
	'utf8'
);

function ev(
	code: string,
	mods: Partial<Record<'ctrlKey' | 'altKey' | 'shiftKey' | 'metaKey', boolean>> = {}
) {
	return { code, ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, ...mods };
}

describe('canonical names', () => {
	it('NAMED_KEYS equals keys.rs NAMED', () => {
		const block = /const NAMED: &\[&str\] = &\[([\s\S]*?)\];/.exec(keysRs)?.[1] ?? '';
		const names = [...block.matchAll(/"((?:[^"\\]|\\.)*)"/g)].map((m) =>
			m[1]!.replace(/\\\\/g, '\\')
		);
		expect(names.length).toBeGreaterThan(60);
		expect([...NAMED_KEYS]).toEqual(names);
	});

	it('accepts letters, digits, F1 to F24 and VK_xx', () => {
		expect(isCanonical('A')).toBe(true);
		expect(isCanonical('7')).toBe(true);
		expect(isCanonical('F24')).toBe(true);
		expect(isCanonical('F25')).toBe(false);
		expect(isCanonical('F01')).toBe(false);
		expect(isCanonical('VK_FF')).toBe(true);
		expect(isCanonical('a')).toBe(false);
		expect(isCanonical('Banana')).toBe(false);
	});
});

describe('parseKeyName (keystage-core parse_key_name)', () => {
	it.each([
		['Escape', 'Esc'],
		['PgUp', 'PageUp'],
		['ArrowLeft', 'Left'],
		['KeyP', 'P'],
		['Digit1', '1'],
		['Numpad7', 'Num7'],
		['num+', 'NumAdd'],
		['p', 'P'],
		['f12', 'F12'],
		['Spacebar', 'Space'],
		['Control', 'Ctrl'],
		['Super', 'Win'],
		['Print Screen', 'PrintScreen'],
		['Page_Down', 'PageDown'],
	])('%s -> %s', (raw, name) => {
		expect(parseKeyName(raw)).toBe(name);
	});

	it('rejects what it does not know', () => {
		expect(parseKeyName('')).toBeNull();
		expect(parseKeyName('Fn')).toBeNull();
		expect(parseKeyName('???')).toBeNull();
		expect(parseKeyName('F0')).toBeNull();
	});
});

describe('accelerators', () => {
	it('stores modifiers in the order Ctrl, Alt, AltGr, Shift, Win (core ACCEL_ORDER)', () => {
		const order = /const ACCEL_ORDER: \[ModKey; 5\] = \[([\s\S]*?)\];/.exec(keysRs)?.[1] ?? '';
		expect([...order.matchAll(/ModKey::(\w+)/g)].map((m) => m[1])).toEqual([
			'Ctrl',
			'Alt',
			'AltGr',
			'Shift',
			'Win',
		]);
		expect(normalizeAccelerator('shift + alt + ctrl + p')).toBe('Ctrl+Alt+Shift+P');
		expect(normalizeAccelerator('Win+Ctrl+F5')).toBe('Ctrl+Win+F5');
	});

	it('reports why a string cannot be read', () => {
		expect(parseAccelerator('')).toEqual({ ok: false, error: 'The shortcut is empty.' });
		expect(parseAccelerator('Ctrl+Alt')).toEqual({
			ok: false,
			error: 'The shortcut needs a key besides the modifiers.',
		});
		expect(parseAccelerator('Ctrl+A+B')).toEqual({
			ok: false,
			error: 'The shortcut has more than one key.',
		});
		expect(parseAccelerator('Ctrl+Banana')).toEqual({
			ok: false,
			error: '"Banana" is not a key name.',
		});
		expect(parseAccelerator('Ctrl++')).toEqual({
			ok: false,
			error: 'The shortcut has an empty part.',
		});
	});

	it('lists parts in display order for keycaps', () => {
		expect(acceleratorParts('Ctrl+Alt+Shift+P')).toEqual(['Ctrl', 'Shift', 'Alt', 'P']);
		expect(acceleratorParts('nonsense key')).toEqual(['nonsense key']);
		expect(acceleratorParts('')).toEqual([]);
	});

	it('knows command modifiers and function keys', () => {
		const a = (s: string) => {
			const r = parseAccelerator(s);
			if (!r.ok) throw new Error(r.error);
			return r.accel;
		};
		expect(hasCommandModifier(a('Shift+P'))).toBe(false);
		expect(hasCommandModifier(a('Alt+P'))).toBe(true);
		expect(isFunctionKey('F9')).toBe(true);
		expect(isFunctionKey('F25')).toBe(false);
		expect(formatAccelerator(a('ctrl+shift+s'))).toBe('Ctrl+Shift+S');
	});

	it('isRegistrable follows to_global_shortcut in keys.rs', () => {
		const fn = /pub fn to_global_shortcut[\s\S]*?\n {4}\}\n/.exec(keysRs)?.[0] ?? '';
		const singles = /"([^"]*)"\.contains\(k\)/.exec(fn)?.[1]?.replace(/\\\\/g, '\\') ?? '';
		const named = [...(/k @ \(([\s\S]*?)\) => k/.exec(fn)?.[1] ?? '').matchAll(/"(\w+)"/g)].map(
			(m) => m[1]!
		);
		expect(singles.length).toBeGreaterThan(40);
		expect(named.length).toBeGreaterThan(20);
		const reg = (key: string, mods: ('Ctrl' | 'AltGr')[] = ['Ctrl']) =>
			isRegistrable({ mods, key });
		for (const c of singles) expect(reg(c), c).toBe(true);
		for (const n of named) expect(reg(n), n).toBe(true);
		expect(reg('MediaNext')).toBe(true);
		expect(reg('F13')).toBe(true);
		expect(reg('Num5')).toBe(true);
		expect(reg('Menu')).toBe(false);
		expect(reg('BrowserBack')).toBe(false);
		expect(reg('P', ['AltGr'])).toBe(false);
	});
});

describe('capture', () => {
	it('maps KeyboardEvent codes to canonical names', () => {
		expect(keyFromCode('KeyQ')).toBe('Q');
		expect(keyFromCode('Digit0')).toBe('0');
		expect(keyFromCode('Numpad3')).toBe('Num3');
		expect(keyFromCode('NumpadEnter')).toBe('NumEnter');
		expect(keyFromCode('F12')).toBe('F12');
		expect(keyFromCode('Backquote')).toBe('`');
		expect(keyFromCode('ArrowUp')).toBe('Up');
		expect(keyFromCode('ControlLeft')).toBeNull();
		expect(keyFromCode('Lang1')).toBeNull();
	});

	it('waits on modifiers, cancels on a bare Escape and finishes on a key', () => {
		expect(captureStep(ev('ControlLeft', { ctrlKey: true }))).toEqual({
			kind: 'modifiers',
			mods: ['Ctrl'],
		});
		expect(captureStep(ev('ShiftLeft', { ctrlKey: true, shiftKey: true }))).toEqual({
			kind: 'modifiers',
			mods: ['Ctrl', 'Shift'],
		});
		expect(captureStep(ev('Escape'))).toEqual({ kind: 'cancel' });
		expect(captureStep(ev('Escape', { ctrlKey: true }))).toEqual({
			kind: 'done',
			accel: { mods: ['Ctrl'], key: 'Esc' },
		});
		expect(captureStep(ev('KeyP', { ctrlKey: true, altKey: true, shiftKey: true }))).toEqual({
			kind: 'done',
			accel: { mods: ['Ctrl', 'Alt', 'Shift'], key: 'P' },
		});
		expect(captureStep(ev('Lang2'))).toEqual({ kind: 'unknown', code: 'Lang2' });
	});

	it('reads held modifiers from flags', () => {
		expect(heldModifiers({ ctrlKey: true, altKey: false, shiftKey: true, metaKey: true })).toEqual([
			'Ctrl',
			'Shift',
			'Win',
		]);
	});
});
