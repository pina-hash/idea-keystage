import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import type { Hotkeys, ProfileData } from '../types';
import { DEFAULT_HOTKEYS } from '../defaults';
import { parseAccelerator } from './accelerator';
import { HOTKEY_INFO, blockingProblems, hotkeyProblems, solidworksConflict } from './hotkeys';

const DATA = JSON.parse(
	readFileSync(resolve(__dirname, '../../../data/profiles/solidworks.json'), 'utf8')
) as ProfileData;

function accel(s: string) {
	const r = parseAccelerator(s);
	if (!r.ok) throw new Error(r.error);
	return r.accel;
}

const hk = (over: Partial<Hotkeys> = {}): Hotkeys => ({ ...DEFAULT_HOTKEYS, ...over });

describe('blockingProblems', () => {
	it('accepts the defaults and a lone function key', () => {
		expect(blockingProblems('pause', accel('Ctrl+Alt+Shift+P'), hk())).toEqual([]);
		expect(blockingProblems('pause', accel('F9'), hk())).toEqual([]);
	});

	it('needs Ctrl, Alt or Win (Shift alone is typing)', () => {
		expect(blockingProblems('pause', accel('Shift+P'), hk())[0]?.message).toMatch(
			/Ctrl, Alt or Win/
		);
		expect(blockingProblems('pause', accel('P'), hk())).toHaveLength(1);
	});

	it('rejects keys the global-shortcut plugin cannot register', () => {
		expect(blockingProblems('pause', accel('Ctrl+Menu'), hk())[0]?.message).toBe(
			'Menu cannot be used in a global hotkey.'
		);
		expect(blockingProblems('pause', accel('Ctrl+AltGr+P'), hk())[0]?.message).toBe(
			'AltGr cannot be part of a global hotkey.'
		);
	});

	it('names the hotkey that already uses the shortcut', () => {
		const p = blockingProblems('pause', accel('ctrl+shift+alt+k'), hk());
		expect(p).toEqual([
			{ level: 'error', message: `Already used by ${HOTKEY_INFO.toggle_keyboard.title}.` },
		]);
	});
});

describe('hotkeyProblems', () => {
	it('reports a shortcut Windows refused', () => {
		const p = hotkeyProblems('pause', {
			hotkeys: hk(),
			states: [
				{
					name: 'pause',
					accelerator: 'Ctrl+Alt+Shift+P',
					registered: false,
					error: 'another program has it',
				},
			],
		});
		expect(p).toEqual([
			{ level: 'error', message: 'Windows did not accept it: another program has it' },
		]);
	});

	it('reports a stored value it cannot read', () => {
		expect(hotkeyProblems('pause', { hotkeys: hk({ pause: 'Ctrl+Banana' }) })[0]?.level).toBe(
			'error'
		);
	});

	it('warns with the data file caption when SolidWorks lists the same keys', () => {
		const entry = DATA.shortcuts.find(
			(s) => s.keys.length === 2 && s.keys[0] === 'Ctrl' && /^[A-Z]$/.test(s.keys[1]!) && !s.context
		)!;
		const shortcut = entry.keys.join('+');
		const msg = solidworksConflict(accel(shortcut), DATA);
		expect(msg).toContain(entry.command.trim());
		const p = hotkeyProblems('chapter_marker', {
			hotkeys: hk({ chapter_marker: shortcut }),
			profile: DATA,
		});
		expect(p.some((x) => x.level === 'warning' && x.message.includes(entry.command.trim()))).toBe(
			true
		);
		expect(solidworksConflict(accel('Ctrl+Alt+Shift+F24'), DATA)).toBeNull();
		expect(solidworksConflict(accel('Ctrl+B'), null)).toBeNull();
	});

	it('the default hotkeys have no problems with the data', () => {
		for (const name of Object.keys(DEFAULT_HOTKEYS) as (keyof Hotkeys)[]) {
			expect(hotkeyProblems(name, { hotkeys: hk(), profile: DATA })).toEqual([]);
		}
	});
});
