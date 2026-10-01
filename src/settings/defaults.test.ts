import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { defaultOverlayConfig } from '../lib/protocol';
import {
	DEFAULT_EXCLUDED_APPS,
	DEFAULT_HOTKEYS,
	RULE_LIMITS,
	SOUND_PACKS,
	clone,
	defaultDisplayRules,
	defaultSettings,
	normalizeSettings,
	normalizeStatus,
	sameJson,
	stableStringify,
} from './defaults';

const ROOT = resolve(__dirname, '../..');
const core = (file: string) =>
	readFileSync(resolve(ROOT, 'crates/keystage-core/src', file), 'utf8');

describe('defaults match keystage-core', () => {
	it('excluded apps equal DEFAULT_EXCLUDED_APPS in settings.rs', () => {
		const src = core('settings.rs');
		const block = /DEFAULT_EXCLUDED_APPS: &\[&str\] = &\[([\s\S]*?)\];/.exec(src)?.[1] ?? '';
		const names = [...block.matchAll(/"([^"]+)"/g)].map((m) => m[1]);
		expect(names.length).toBeGreaterThan(5);
		expect([...DEFAULT_EXCLUDED_APPS]).toEqual(names);
	});

	it('hotkeys equal Hotkeys::default in settings.rs', () => {
		const src = core('settings.rs');
		const block = /impl Default for Hotkeys \{[\s\S]*?Hotkeys \{([\s\S]*?)\}/.exec(src)?.[1] ?? '';
		const pairs = Object.fromEntries(
			[...block.matchAll(/(\w+): "([^"]+)"\.into\(\)/g)].map((m) => [m[1], m[2]])
		);
		expect(pairs).toEqual(DEFAULT_HOTKEYS);
	});

	it('sound packs equal synth::PACKS in order, with the same names and descriptions', () => {
		const src = core('synth.rs');
		const packs = [
			...src.matchAll(
				/PackInfo \{\s*id: "([^"]+)",\s*name: "([^"]+)",\s*description: "([^"]+)",\s*\}/g
			),
		].map((m) => ({ id: m[1], name: m[2], description: m[3] }));
		expect(packs.length).toBe(12);
		expect(SOUND_PACKS).toEqual(packs);
		expect(/DEFAULT_PACK: &str = "([^"]+)"/.exec(src)?.[1]).toBe(defaultSettings().sound.pack);
	});

	it('display rules equal DisplayRules::default in rules.rs', () => {
		const src = core('rules.rs');
		const block =
			/impl Default for DisplayRules \{[\s\S]*?DisplayRules \{([\s\S]*?)\n\s*\}\n\s*\}/.exec(
				src
			)?.[1] ?? '';
		const d = defaultDisplayRules() as unknown as Record<string, unknown>;
		for (const m of block.matchAll(/(\w+): ([^,\n]+),/g)) {
			const [, field, raw] = m;
			const value = raw!.trim();
			if (value === 'TypingMode::ShortcutsOnly') expect(d[field!]).toBe('shortcuts_only');
			else if (value === 'Vec::new()') expect(d[field!]).toEqual([]);
			else if (value === 'true' || value === 'false') expect(d[field!]).toBe(value === 'true');
			else expect(d[field!]).toBe(Number(value.replace(/_/g, '')));
		}
		expect(Object.keys(d).length).toBe([...block.matchAll(/(\w+): ([^,\n]+),/g)].length);
	});

	it('rule limits equal DisplayRules::validate in rules.rs', () => {
		const src = core('rules.rs');
		for (const m of src.matchAll(
			/clamp(?:32)?\(\s*"(\w+)",\s*&mut self\.\w+,\s*([\d_]+),\s*([\d_]+),?\s*\)/g
		)) {
			const name = m[1] as keyof typeof RULE_LIMITS;
			if (!(name in RULE_LIMITS)) continue;
			expect(RULE_LIMITS[name]).toEqual({
				min: Number(m[2]!.replace(/_/g, '')),
				max: Number(m[3]!.replace(/_/g, '')),
			});
		}
	});

	it('the default settings follow ARCHITECTURE.md section 7', () => {
		const s = defaultSettings();
		expect(s.version).toBe(1);
		expect(s.first_run_done).toBe(false);
		expect(s.server.port).toBe(47820);
		expect(s.obs).toMatchObject({
			enabled: false,
			host: '127.0.0.1',
			port: 4455,
			password_dpapi: null,
			auto_session: true,
		});
		expect(s.sound.enabled).toBe(false);
		expect(s.startup.start_with_windows).toBe(false);
		expect(s.overlay).toEqual(defaultOverlayConfig());
		expect(s.profiles.solidworks_mode).toBe('auto');
	});
});

describe('normalizeSettings', () => {
	it('returns the defaults for anything that is not an object', () => {
		expect(normalizeSettings(null)).toEqual(defaultSettings());
		expect(normalizeSettings('x')).toEqual(defaultSettings());
	});

	it('keeps good values and fills the rest', () => {
		const s = normalizeSettings({
			first_run_done: true,
			server: { port: 50000 },
			overlay: { theme: 'studio', callouts: { linger_ms: 3000 } },
			hotkeys: { pause: 'F9' },
			privacy: { excluded_apps: ['Vault.exe', 3] },
		});
		expect(s.first_run_done).toBe(true);
		expect(s.server.port).toBe(50000);
		expect(s.overlay.theme).toBe('studio');
		expect(s.overlay.callouts.linger_ms).toBe(3000);
		expect(s.overlay.callouts.max_stack).toBe(4);
		expect(s.hotkeys.pause).toBe('F9');
		expect(s.hotkeys.toggle_keyboard).toBe(DEFAULT_HOTKEYS.toggle_keyboard);
		expect(s.privacy.excluded_apps).toEqual(['Vault.exe']);
	});

	it('clamps numbers and rejects unknown enums', () => {
		const s = normalizeSettings({
			server: { port: 99999 },
			sound: { volume: 4, pack: 'nope' },
			display: { typing: 'everything', repeat_window_ms: -5, chord_max_steps: 40 },
			profiles: {
				solidworks_mode: 'sometimes',
				solidworks_display: { typed_values: false, mode: 'x' },
			},
		});
		expect(s.server.port).toBe(65535);
		expect(s.sound.volume).toBe(1);
		expect(s.sound.pack).toBe('soft-linear');
		expect(s.display.typing).toBe('shortcuts_only');
		expect(s.display.repeat_window_ms).toBe(0);
		expect(s.display.chord_max_steps).toBe(8);
		expect(s.profiles.solidworks_mode).toBe('auto');
		expect(s.profiles.solidworks_display).toEqual({ typed_values: false });
	});

	it('drops unreadable imported shortcuts and marks the rest as user imports', () => {
		const s = normalizeSettings({
			profiles: {
				solidworks_user_shortcuts: [
					{ keys: ['Ctrl', 'B'], command: 'Fixture Command One', category: 'A', source: 'other' },
					{ keys: [], command: 'Fixture Command Two' },
					{ keys: ['Tab'], command: '  ' },
				],
			},
		});
		expect(s.profiles.solidworks_user_shortcuts).toEqual([
			{
				keys: ['Ctrl', 'B'],
				doc_keys: 'Ctrl+B',
				command: 'Fixture Command One',
				category: 'A',
				source: 'user-import',
			},
		]);
	});

	it('reads show_settings as true when the app does not send it', () => {
		expect(normalizeSettings({ startup: { start_with_windows: true } }).startup).toEqual({
			start_with_windows: true,
			show_settings: true,
		});
		expect(normalizeSettings({ startup: { show_settings: false } }).startup.show_settings).toBe(
			false
		);
	});
});

describe('normalizeStatus', () => {
	it('fills a partial status and keeps known values', () => {
		const s = normalizeStatus({
			paused: true,
			suppressed: 'password',
			profile: { id: 'solidworks', name: 'SolidWorks' },
			obs: {
				enabled: true,
				state: 'auth_failed',
				error: 'no',
				recording: { active: true, paused: false, timecode: '00:00:01.000' },
			},
			server: { port: 47820, running: true, clients: 2 },
			hook: { installed: true, dropped: 3 },
			version: '0.1.0',
		});
		expect(s.paused).toBe(true);
		expect(s.suppressed).toBe('password');
		expect(s.obs.state).toBe('auth_failed');
		expect(s.obs.recording?.timecode).toBe('00:00:01.000');
		expect(s.server.clients).toBe(2);
		expect(s.hook.dropped).toBe(3);
		expect(s.session).toBeNull();
	});

	it('never trusts an unknown suppression or OBS state', () => {
		const s = normalizeStatus({ suppressed: 'KeePass.exe', obs: { state: 'weird' } });
		expect(s.suppressed).toBeNull();
		expect(s.obs.state).toBe('off');
	});
});

describe('json helpers', () => {
	it('compares objects regardless of key order and ignores undefined', () => {
		expect(sameJson({ a: 1, b: { c: 2, d: 3 } }, { b: { d: 3, c: 2 }, a: 1 })).toBe(true);
		expect(sameJson({ a: 1, x: undefined }, { a: 1 })).toBe(true);
		expect(sameJson({ a: [1, 2] }, { a: [2, 1] })).toBe(false);
		expect(stableStringify({ b: 1, a: 2 })).toBe('{"a":2,"b":1}');
	});

	it('clones deeply', () => {
		const s = defaultSettings();
		const c = clone(s);
		c.overlay.callouts.max_stack = 9;
		expect(s.overlay.callouts.max_stack).toBe(4);
	});
});
