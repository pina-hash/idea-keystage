import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import {
	COMMANDS,
	IpcError,
	KeystageIpc,
	SETTINGS_EVENT,
	connectIpc,
	errorMessage,
	hasTauriRuntime,
	isSafeExternalUrl,
	type Backend,
	type CommandName,
} from './ipc';
import { defaultSettings } from './defaults';

interface Call {
	cmd: string;
	args: Record<string, unknown> | undefined;
}

function recorder(reply: (cmd: string) => unknown = () => null) {
	const calls: Call[] = [];
	const opened: string[] = [];
	const backend: Backend = {
		kind: 'mock',
		invoke: async (cmd, args) => {
			calls.push({ cmd, args });
			return reply(cmd);
		},
		listen: async () => () => {},
		copyText: async () => {},
		openUrl: async (url) => {
			opened.push(url);
		},
		pickOpenFile: async () => null,
		pickFolder: async () => null,
		pickSaveFile: async () => null,
	};
	return { ipc: new KeystageIpc(backend), calls, opened };
}

describe('command names and argument shapes (ARCHITECTURE.md section 12)', () => {
	it('every command in the contract table is in COMMANDS', () => {
		const doc = readFileSync(resolve(__dirname, '../../docs/ARCHITECTURE.md'), 'utf8');
		const section = doc.slice(doc.indexOf('## 12.'));
		const names = new Set<string>();
		for (const line of section.split('\n')) {
			if (!line.startsWith('| `')) continue;
			const first = line.split('|')[1] ?? '';
			for (const m of first.matchAll(/`([a-z_]+)`/g)) names.add(m[1]!);
		}
		// The settings_export row names settings_import in its second column.
		names.add('settings_import');
		expect(names.size).toBeGreaterThan(25);
		const ours = new Set<string>(Object.values(COMMANDS));
		for (const n of names) expect(ours.has(n), n).toBe(true);
	});

	it('sends exactly the documented arguments', async () => {
		const settings = defaultSettings();
		const { ipc, calls } = recorder((cmd) => {
			if (
				cmd.endsWith('list') ||
				cmd === 'list_monitors' ||
				cmd === 'session_delete' ||
				cmd === 'hotkeys_status'
			)
				return [];
			return {};
		});
		await ipc.getSettings();
		await ipc.saveSettings(settings);
		await ipc.resetSettings();
		await ipc.getStatus();
		await ipc.listMonitors();
		await ipc.setPaused(true);
		await ipc.setSolidworksMode('on');
		await ipc.setKeyboardVisible(false);
		await ipc.obsTest('127.0.0.1', 4455, null);
		await ipc.obsSetPassword('secret');
		await ipc.shortcutsImportPreview({ kind: 'text', text: 'a' });
		await ipc.shortcutsImportApply({ kind: 'file', path: 'C:\\x.txt' });
		await ipc.shortcutsImportClear();
		await ipc.profileShortcuts('solidworks');
		await ipc.sessionStart();
		await ipc.sessionStop();
		await ipc.sessionMarker(null);
		await ipc.sessionsList();
		await ipc.sessionGet('s1');
		await ipc.sessionRenameMarker('s1', 2, 'Intro');
		await ipc.sessionExport('s1', 'chapters', 'C:\\c.txt');
		await ipc.sessionDelete('s1');
		await ipc.settingsExport('C:\\s.json');
		await ipc.settingsImport('C:\\s.json');
		await ipc.serverUrls();
		await ipc.playSoundPreview('tactile');
		await ipc.openExternalFolder('config');
		await ipc.overlayHello();
		await ipc.hotkeysCapture(true);
		await ipc.hotkeysStatus();
		await ipc.platformInfo();
		const expected: [CommandName, Record<string, unknown> | undefined][] = [
			['get_settings', undefined],
			['save_settings', { settings }],
			['reset_settings', undefined],
			['get_status', undefined],
			['list_monitors', undefined],
			['set_paused', { paused: true }],
			['set_solidworks_mode', { mode: 'on' }],
			['set_keyboard_visible', { visible: false }],
			['obs_test', { host: '127.0.0.1', port: 4455, password: null }],
			['obs_set_password', { password: 'secret' }],
			['shortcuts_import_preview', { source: { kind: 'text', text: 'a' } }],
			['shortcuts_import_apply', { source: { kind: 'file', path: 'C:\\x.txt' } }],
			['shortcuts_import_clear', undefined],
			['profile_shortcuts', { id: 'solidworks' }],
			['session_start', undefined],
			['session_stop', undefined],
			['session_marker', { name: null }],
			['sessions_list', undefined],
			['session_get', { id: 's1' }],
			['session_rename_marker', { id: 's1', index: 2, name: 'Intro' }],
			['session_export', { id: 's1', format: 'chapters', path: 'C:\\c.txt' }],
			['session_delete', { id: 's1' }],
			['settings_export', { path: 'C:\\s.json' }],
			['settings_import', { path: 'C:\\s.json' }],
			['server_urls', undefined],
			['play_sound_preview', { pack: 'tactile' }],
			['open_external_folder', { which: 'config' }],
			['overlay_hello', undefined],
			['hotkeys_capture', { active: true }],
			['hotkeys_status', undefined],
			['platform_info', undefined],
		];
		expect(calls.map((c) => [c.cmd, c.args])).toEqual(expected);
	});

	it('normalizes settings and status replies', async () => {
		const { ipc } = recorder(() => ({ server: { port: 50000 } }));
		const s = await ipc.getSettings();
		expect(s.server.port).toBe(50000);
		expect(s.overlay.theme).toBe('idea');
		const st = await ipc.getStatus();
		expect(st.obs.state).toBe('off');
	});

	it('treats a non-array list reply as empty', async () => {
		const { ipc } = recorder(() => null);
		expect(await ipc.listMonitors()).toEqual([]);
		expect(await ipc.sessionsList()).toEqual([]);
	});

	it('keeps the notices of a status and the warnings of an export', async () => {
		const { ipc } = recorder((cmd) =>
			cmd === 'session_export'
				? { path: 'C:\\c.txt', warnings: ['This session holds 2 recordings.'] }
				: { notices: ['Starting with Windows could not be turned on.'] }
		);
		expect((await ipc.getStatus()).notices).toEqual([
			'Starting with Windows could not be turned on.',
		]);
		expect(await ipc.sessionExport('s1', 'chapters', 'C:\\c.txt')).toEqual({
			path: 'C:\\c.txt',
			warnings: ['This session holds 2 recordings.'],
		});
	});

	it('listens for keystage://settings', async () => {
		const events: string[] = [];
		let fire: ((p: unknown) => void) | null = null;
		const backend: Backend = {
			...recorder().ipc.backend,
			listen: async (event, handler) => {
				events.push(event);
				fire = handler;
				return () => {};
			},
		};
		let heard = 0;
		await new KeystageIpc(backend).onSettingsChanged(() => (heard += 1));
		expect(events).toEqual([SETTINGS_EVENT]);
		expect(SETTINGS_EVENT).toBe('keystage://settings');
		fire!({ anything: true });
		expect(heard).toBe(1);
	});
});

describe('errors', () => {
	it('turns any rejection into an IpcError with a readable message', async () => {
		const backend: Backend = {
			...recorder().ipc.backend,
			invoke: async () => {
				throw 'Port 47820 is in use.';
			},
		};
		const ipc = new KeystageIpc(backend);
		await expect(ipc.getStatus()).rejects.toBeInstanceOf(IpcError);
		await expect(ipc.getStatus()).rejects.toThrow('Port 47820 is in use.');
	});

	it('reads strings, errors and objects', () => {
		expect(errorMessage(' nope ')).toBe('nope');
		expect(errorMessage(new Error('bad'))).toBe('bad');
		expect(errorMessage({ message: 'from rust' })).toBe('from rust');
		expect(errorMessage({ error: 'other' })).toBe('other');
		expect(errorMessage(undefined)).toBe('The app did not answer. Try again.');
	});
});

describe('links', () => {
	it('opens only https links', async () => {
		expect(isSafeExternalUrl('https://help.solidworks.com/2025/english/x.htm')).toBe(true);
		expect(isSafeExternalUrl('http://example.com')).toBe(false);
		expect(isSafeExternalUrl('file:///C:/x')).toBe(false);
		expect(isSafeExternalUrl('javascript:alert(1)')).toBe(false);
		expect(isSafeExternalUrl('https://user:pw@example.com')).toBe(false);
		expect(isSafeExternalUrl('not a url')).toBe(false);
		const { ipc, opened } = recorder();
		await ipc.openUrl('https://help.solidworks.com/2026/english/x.htm');
		await expect(ipc.openUrl('ftp://x')).rejects.toBeInstanceOf(IpcError);
		expect(opened).toEqual(['https://help.solidworks.com/2026/english/x.htm']);
	});
});

describe('backend selection', () => {
	it('uses the mock backend outside the app', async () => {
		expect(hasTauriRuntime({})).toBe(false);
		expect(hasTauriRuntime({ __TAURI_INTERNALS__: {} })).toBe(true);
		const ipc = await connectIpc('?state=paused');
		expect(ipc.kind).toBe('mock');
		const st = await ipc.getStatus();
		expect(st.paused).toBe(true);
	});
});
