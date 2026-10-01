import { describe, expect, it } from 'vitest';
import { defaultSettings } from '../defaults';
import { merge3 } from './merge';

describe('merge3', () => {
	it("keeps this window's edits and takes every other value from the app", () => {
		const base = defaultSettings();
		const mine = defaultSettings();
		mine.overlay.callouts.size = 'xl';
		const theirs = defaultSettings();
		theirs.profiles.solidworks_mode = 'on';
		theirs.host.native_overlay = false;
		const out = merge3(base, mine, theirs);
		expect(out.overlay.callouts.size).toBe('xl');
		expect(out.profiles.solidworks_mode).toBe('on');
		expect(out.host.native_overlay).toBe(false);
	});

	it('lets an edit win over the same value changed elsewhere', () => {
		const base = defaultSettings();
		const mine = defaultSettings();
		mine.host.native_overlay = false;
		const theirs = defaultSettings();
		theirs.host.native_overlay = true;
		theirs.profiles.solidworks_mode = 'off';
		const out = merge3(base, mine, theirs);
		expect(out.host.native_overlay).toBe(false);
		expect(out.profiles.solidworks_mode).toBe('off');
	});

	it('treats a list as one value', () => {
		const base = { apps: ['a.exe', 'b.exe'], n: 1 };
		const mine = { apps: ['a.exe'], n: 1 };
		const theirs = { apps: ['a.exe', 'b.exe', 'c.exe'], n: 2 };
		expect(merge3(base, mine, theirs)).toEqual({ apps: ['a.exe'], n: 2 });
		expect(merge3(base, base, theirs)).toEqual(theirs);
	});

	it('handles null, added and removed keys', () => {
		const base: Record<string, unknown> = { custom: null, gone: 1, kept: { a: 1 } };
		const mine: Record<string, unknown> = {
			custom: { accent: '#fff' },
			gone: 1,
			kept: { a: 1 },
			extra: true,
		};
		const theirs: Record<string, unknown> = { custom: null, kept: { a: 2 } };
		expect(merge3(base, mine, theirs)).toEqual({
			custom: { accent: '#fff' },
			kept: { a: 2 },
			extra: true,
		});
	});

	it('returns a copy that shares nothing with its inputs', () => {
		const base = { a: { b: [1] } };
		const theirs = { a: { b: [2] } };
		const out = merge3(base, base, theirs);
		out.a.b.push(3);
		expect(theirs.a.b).toEqual([2]);
	});
});
