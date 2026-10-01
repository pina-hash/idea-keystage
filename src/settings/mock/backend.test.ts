import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import type { Timers } from '../../lib/renderer/transport';
import { KeystageIpc } from '../ipc';
import type { ProfileData } from '../types';
import { createMockBackend, scenarioFrom, type MockOptions } from './backend';
import { SCENARIOS, sessionId } from './fixtures';

const DATA = JSON.parse(
	readFileSync(resolve(__dirname, '../../../data/profiles/solidworks.json'), 'utf8')
) as ProfileData;

class ManualTimers implements Timers {
	t = 0;
	private id = 1;
	private jobs = new Map<number, { at: number; every: number | null; fn: () => void }>();
	now(): number {
		return this.t;
	}
	setTimeout(fn: () => void, ms: number): unknown {
		const id = this.id++;
		this.jobs.set(id, { at: this.t + ms, every: null, fn });
		return id;
	}
	clearTimeout(h: unknown): void {
		this.jobs.delete(h as number);
	}
	setInterval(fn: () => void, ms: number): unknown {
		const id = this.id++;
		this.jobs.set(id, { at: this.t + ms, every: ms, fn });
		return id;
	}
	clearInterval(h: unknown): void {
		this.jobs.delete(h as number);
	}
	advance(ms: number): void {
		const end = this.t + ms;
		for (;;) {
			const next = [...this.jobs.entries()]
				.filter(([, j]) => j.at <= end)
				.sort((a, b) => a[1].at - b[1].at)[0];
			if (!next) break;
			const [id, job] = next;
			this.t = job.at;
			if (job.every) job.at += job.every;
			else this.jobs.delete(id);
			job.fn();
		}
		this.t = end;
	}
	get pending(): number {
		return this.jobs.size;
	}
}

function make(scenario: MockOptions['scenario'] = 'normal', extra: Partial<MockOptions> = {}) {
	let wall = Date.UTC(2026, 9, 1, 15, 0, 0);
	const timers = new ManualTimers();
	const backend = createMockBackend({
		scenario,
		latencyMs: 0,
		liveDemo: false,
		loadProfile: async () => DATA,
		now: () => wall,
		timers,
		...extra,
	});
	return { backend, ipc: new KeystageIpc(backend), timers, tick: (ms: number) => (wall += ms) };
}

describe('scenarios', () => {
	it('reads ?state= and falls back to normal', () => {
		expect(scenarioFrom('?state=paused')).toBe('paused');
		expect(scenarioFrom('?state=whatever')).toBe('normal');
		expect(scenarioFrom('')).toBe('normal');
	});

	it('every scenario answers every read command', async () => {
		for (const sc of SCENARIOS) {
			const { ipc } = make(sc);
			const s = await ipc.getSettings();
			const st = await ipc.getStatus();
			const urls = await ipc.serverUrls();
			expect(urls.routes).toHaveLength(5);
			expect(await ipc.platformInfo()).toMatchObject({ exclude_from_capture_supported: true });
			expect(await ipc.hotkeysStatus()).toHaveLength(5);
			expect(Array.isArray(await ipc.listMonitors())).toBe(true);
			expect(Array.isArray(await ipc.sessionsList())).toBe(true);
			expect(s.first_run_done).toBe(sc !== 'first-run');
			expect(st.version).toBe('0.1.0');
		}
	});

	it('shapes each scenario the way its name says', async () => {
		expect((await make('paused').ipc.getStatus()).paused).toBe(true);
		expect((await make('obs-error').ipc.getStatus()).obs.state).toBe('auth_failed');
		const busy = await make('port-busy').ipc.getStatus();
		expect(busy.server.running).toBe(false);
		expect(busy.server.error).toContain('in use');
		const empty = make('empty').ipc;
		expect(await empty.listMonitors()).toEqual([]);
		expect(await empty.sessionsList()).toEqual([]);
		expect((await empty.getStatus()).obs.state).toBe('off');
		expect((await empty.getStatus()).notices).toEqual([]);
		const notices = await make('notices').ipc.getStatus();
		expect(notices.notices).toHaveLength(3);
		expect(notices.notices[0]).toContain('did not start');
		expect(notices.suppressed).toBe('excluded');
	});
});

describe('settings', () => {
	it('validates a save the way the app does and keeps the stored password', async () => {
		const { ipc } = make();
		const s = await ipc.getSettings();
		const blob = s.obs.password_dpapi;
		expect(blob).not.toBeNull();
		s.hotkeys.pause = 'shift+alt+ctrl+q';
		s.privacy.excluded_apps = ['C:\\Tools\\Vault.exe', 'vault.exe'];
		s.obs.password_dpapi = null;
		s.overlay.scale = 99;
		const stored = await ipc.saveSettings(s);
		expect(stored.hotkeys.pause).toBe('Ctrl+Alt+Shift+Q');
		expect(stored.privacy.excluded_apps).toEqual(['Vault.exe']);
		expect(stored.obs.password_dpapi).toBe(blob);
		expect(stored.overlay.scale).toBe(4);
		expect((await ipc.getSettings()).hotkeys.pause).toBe('Ctrl+Alt+Shift+Q');
	});

	it('a new port restarts the server on it', async () => {
		const { ipc } = make('port-busy');
		const s = await ipc.getSettings();
		s.server.port = 47821;
		await ipc.saveSettings(s);
		const st = await ipc.getStatus();
		expect(st.server).toMatchObject({ port: 47821, running: true, error: null });
		expect((await ipc.serverUrls()).routes[0]!.url).toBe('http://127.0.0.1:47821/overlay/keys');
	});

	it('exports without the password and imports it back', async () => {
		const { ipc, backend } = make();
		await ipc.settingsExport('C:\\a.json');
		expect(JSON.parse(backend.peek().files.get('C:\\a.json')!).obs.password_dpapi).toBeNull();
		const s = await ipc.getSettings();
		s.overlay.theme = 'carbon';
		await ipc.saveSettings(s);
		const back = await ipc.settingsImport('C:\\a.json');
		expect(back.overlay.theme).toBe('idea');
		await expect(ipc.settingsImport('C:\\missing.json')).rejects.toThrow('does not exist');
	});

	it('reset keeps setup finished and clears imports', async () => {
		const { ipc } = make();
		const s = await ipc.resetSettings();
		expect(s.first_run_done).toBe(true);
		expect(s.obs.password_dpapi).toBeNull();
		expect(s.profiles.solidworks_user_shortcuts).toEqual([]);
	});
});

describe('status commands', () => {
	it('pauses, switches the SolidWorks mode and the keyboard visual', async () => {
		const { ipc } = make();
		expect((await ipc.setPaused(true)).hook.installed).toBe(false);
		const off = await ipc.setSolidworksMode('off');
		expect(off.solidworks_mode).toBe('off');
		expect(off.profile).toBeNull();
		expect((await ipc.getSettings()).profiles.solidworks_mode).toBe('off');
		expect((await ipc.setKeyboardVisible(false)).keyboard_visible).toBe(false);
	});

	it('emits status on change and ticks the recording timecode', async () => {
		const { ipc, timers, tick } = make();
		const seen: string[] = [];
		const un = await ipc.onStatus((s) => seen.push(s.obs.recording?.timecode ?? ''));
		tick(1000);
		timers.advance(1000);
		expect(seen.at(-1)).toBe('00:12:42.233');
		await ipc.setPaused(true);
		expect(seen.length).toBeGreaterThanOrEqual(2);
		un();
		expect(timers.pending).toBe(0);
	});

	it('OBS test and password follow the scenario', async () => {
		const bad = make('obs-error').ipc;
		expect(await bad.obsTest('127.0.0.1', 4455, null)).toEqual({
			ok: false,
			error: 'OBS did not accept the password.',
		});
		expect(await bad.obsSetPassword('pw')).toEqual({ stored: true });
		expect((await bad.getStatus()).obs.state).toBe('connected');
		const good = make().ipc;
		expect(await good.obsTest('127.0.0.1', 4455, null)).toMatchObject({
			ok: true,
			obs_version: '32.0.1',
		});
		expect(await good.obsTest('', 4455, null)).toMatchObject({ ok: false });
		expect(await good.obsSetPassword(null)).toEqual({ stored: false });
		expect((await good.getSettings()).obs.password_dpapi).toBeNull();
	});
});

describe('SolidWorks import', () => {
	it('previews, applies and clears, and the merged profile lists the import first', async () => {
		const { ipc } = make('import-warnings');
		const src = { kind: 'file' as const, path: 'C:\\x\\keyboard-copy-list.txt' };
		const p = await ipc.shortcutsImportPreview(src);
		expect(p.validated).toBe(false);
		expect(p.warnings.length).toBeGreaterThanOrEqual(4);
		const s = await ipc.shortcutsImportApply(src);
		expect(s.profiles.solidworks_user_shortcuts.length).toBe(p.entries.length);
		const merged = await ipc.profileShortcuts('solidworks');
		expect(merged.shortcuts[0]!.source).toBe('user-import');
		expect(merged.shortcuts.length).toBe(DATA.shortcuts.length + p.entries.length);
		expect((await ipc.shortcutsImportClear()).profiles.solidworks_user_shortcuts).toEqual([]);
		await expect(ipc.profileShortcuts('other')).rejects.toThrow('There is no profile');
	});

	it('reports a .sldreg file and pasted registry text without guessing', async () => {
		const { ipc } = make();
		const sld = await ipc.shortcutsImportPreview({ kind: 'file', path: 'C:\\x.sldreg' });
		expect(sld.format).toBe('sldreg');
		expect(sld.entries).toEqual([]);
		const reg = await ipc.shortcutsImportPreview({
			kind: 'text',
			text: 'Windows Registry Editor Version 5.00\r\n',
		});
		expect(reg.format).toBe('sldreg');
		await expect(ipc.shortcutsImportApply({ kind: 'file', path: 'C:\\x.sldreg' })).rejects.toThrow(
			'nothing to apply'
		);
	});
});

describe('sessions', () => {
	it('starts, marks, stops and lists a session, then exports, renames and deletes', async () => {
		const { ipc, tick } = make('empty');
		await expect(ipc.sessionMarker('x')).rejects.toThrow('Start a session log first.');
		const started = await ipc.sessionStart();
		expect(started.session?.active).toBe(true);
		tick(90_000);
		expect((await ipc.sessionMarker('Intro')).session?.markers).toBe(1);
		await ipc.sessionMarker(null);
		tick(30_000);
		expect((await ipc.sessionStop()).session).toBeNull();
		const list = await ipc.sessionsList();
		expect(list).toHaveLength(1);
		expect(list[0]!.id).toBe(sessionId(list[0]!.started));
		expect(list[0]!.duration_ms).toBe(120_000);
		const raw = (await ipc.sessionGet(list[0]!.id)) as {
			schema: string;
			markers: { name: string; t_ms: number }[];
			start_ms: number;
		};
		expect(raw.schema).toBe('keystage.session/1');
		expect(raw.markers.map((m) => m.name)).toEqual(['Intro', 'Marker 2']);
		expect(raw.markers[0]!.t_ms - raw.start_ms).toBe(90_000);
		const renamed = (await ipc.sessionRenameMarker(list[0]!.id, 1, 'Wrap up')) as {
			markers: { name: string }[];
		};
		expect(renamed.markers[1]!.name).toBe('Wrap up');
		const exported = await ipc.sessionExport(list[0]!.id, 'srt', 'C:\\x.srt');
		expect(exported.path).toBe('C:\\x.srt');
		// No OBS timing in the mock sessions: the app says the times count from the session log.
		expect(exported.warnings).toEqual([expect.stringContaining('OBS timing was not available')]);
		const chapters = await ipc.sessionExport(list[0]!.id, 'chapters', 'C:\\c.txt');
		expect(chapters.warnings).toHaveLength(1);
		const two = await ipc.sessionExport(list[0]!.id, 'json', 'C:\\x.json');
		expect(two.warnings).toEqual([]);
		expect(await ipc.sessionDelete(list[0]!.id)).toEqual([]);
		await expect(ipc.sessionGet('gone')).rejects.toThrow('no longer');
	});

	it('the normal scenario has saved sessions with markers', async () => {
		const { ipc } = make();
		const list = await ipc.sessionsList();
		expect(list.length).toBe(3);
		const raw = (await ipc.sessionGet(list[0]!.id)) as { callouts: unknown[]; markers: unknown[] };
		expect(raw.callouts).toHaveLength(list[0]!.callouts);
		expect(raw.markers).toHaveLength(list[0]!.markers);
	});
});

describe('changes the app makes itself', () => {
	it('emits the status and keystage://settings, and notices on demand', async () => {
		const { ipc, backend } = make();
		const statuses: string[] = [];
		const settingsEvents: unknown[] = [];
		await ipc.onStatus((s) => statuses.push(s.solidworks_mode));
		await ipc.onSettingsChanged(() => settingsEvents.push(1));
		backend.changeFromApp((s) => (s.profiles.solidworks_mode = 'on'));
		expect(statuses.at(-1)).toBe('on');
		expect(settingsEvents).toHaveLength(1);
		backend.changeFromApp((s) => (s.host.native_overlay = false), { settingsEvent: false });
		expect(settingsEvents).toHaveLength(1);
		expect((await ipc.getSettings()).host.native_overlay).toBe(false);
		await ipc.setSolidworksMode('off');
		expect(settingsEvents).toHaveLength(2);
		const seen: string[][] = [];
		await ipc.onStatus((s) => seen.push(s.notices));
		backend.setNotices(['Starting with Windows could not be turned on.']);
		expect(seen.at(-1)).toEqual(['Starting with Windows could not be turned on.']);
	});

	it('leaves the password blob out of the settings event', async () => {
		const { backend } = make();
		const got: unknown[] = [];
		await backend.listen('keystage://settings', (p) => got.push(p));
		backend.changeFromApp((s) => (s.overlay.scale = 1.5));
		expect((got[0] as { obs: { password_dpapi: unknown } }).obs.password_dpapi).toBeNull();
		expect(backend.peek().settings.obs.password_dpapi).not.toBeNull();
	});
});

describe('live input simulation', () => {
	it('answers overlay_hello with the current config and plays the demo to listeners', async () => {
		const { ipc, backend, timers } = make('normal', { liveDemo: true });
		const hello = await ipc.overlayHello();
		expect(hello.t).toBe('hello');
		const got: string[] = [];
		const un = await backend.listen('keystage://overlay', (m) => got.push((m as { t: string }).t));
		// The demo script loads lazily (a dynamic import of the demo and the data file).
		for (let i = 0; i < 100 && !got.some((t) => t === 'callout' || t === 'key'); i++) {
			await new Promise((r) => setTimeout(r, 20));
			timers.advance(250);
		}
		expect(got.some((t) => t === 'callout' || t === 'key')).toBe(true);
		expect(got.includes('hello')).toBe(false);
		un();
	});

	it('plugins: dialogs return Windows paths, the clipboard and links go to the given hooks', async () => {
		const copied: string[] = [];
		const opened: string[] = [];
		const { ipc } = make('normal', {
			clipboard: async (t) => void copied.push(t),
			open: (u) => void opened.push(u),
		});
		await ipc.copyText('http://127.0.0.1:47820/overlay/keys');
		await ipc.openUrl('https://help.solidworks.com/2025/english/x.htm');
		expect(copied).toEqual(['http://127.0.0.1:47820/overlay/keys']);
		expect(opened).toEqual(['https://help.solidworks.com/2025/english/x.htm']);
		expect(await ipc.pickOpenFile('x', [{ name: 'Settings', extensions: ['json'] }])).toMatch(
			/\.json$/
		);
		expect(await ipc.pickOpenFile('x', [{ name: 'Copy List', extensions: ['txt'] }])).toMatch(
			/\.txt$/
		);
		expect(await ipc.pickSaveFile('x', 'a.srt', [])).toBe('C:\\Users\\Teacher\\Documents\\a.srt');
		expect(await ipc.pickFolder('x')).toContain('Keystage sessions');
	});
});
