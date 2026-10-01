import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { KeystageIpc } from './ipc';
import { createMockBackend } from './mock/backend';
import { SettingsController } from './state.svelte';
import { clone } from './defaults';
import type { ProfileData } from './types';

const DATA = JSON.parse(
	readFileSync(resolve(__dirname, '../../data/profiles/solidworks.json'), 'utf8')
) as ProfileData;

const live: SettingsController[] = [];

function controller(scenario: Parameters<typeof createMockBackend>[0]['scenario'] = 'normal') {
	const backend = createMockBackend({
		scenario,
		latencyMs: 0,
		liveDemo: false,
		loadProfile: async () => DATA,
	});
	const ctl = new SettingsController(new KeystageIpc(backend), { saveDelayMs: 0 });
	live.push(ctl);
	return { ctl, backend };
}

afterEach(() => {
	for (const c of live.splice(0)) c.destroy();
});

describe('SettingsController', () => {
	it('loads settings, status, monitors, addresses and the profile', async () => {
		const { ctl } = controller();
		await ctl.init();
		await ctl.loadProfile();
		expect(ctl.loadError).toBeNull();
		expect(ctl.settings?.first_run_done).toBe(true);
		expect(ctl.status?.server.running).toBe(true);
		expect(ctl.monitors).toHaveLength(2);
		expect(ctl.urls?.routes).toHaveLength(5);
		expect(ctl.platform?.exclude_from_capture_supported).toBe(true);
		expect(ctl.hotkeyStates).toHaveLength(5);
		expect(ctl.profile?.shortcuts.length).toBe(DATA.shortcuts.length);
	});

	it('saves an edit and adopts what the app stored', async () => {
		const { ctl, backend } = controller();
		await ctl.init();
		const draft = clone(ctl.settings!);
		draft.overlay.theme = 'blueprint';
		ctl.settings = draft;
		ctl.draftChanged(clone(draft));
		await ctl.flush();
		expect(backend.peek().settings.overlay.theme).toBe('blueprint');
		expect(ctl.saveState).toBe('saved');

		const big = clone(ctl.settings!);
		big.overlay.scale = 99;
		ctl.settings = big;
		ctl.draftChanged(clone(big));
		await ctl.flush();
		expect(ctl.settings!.overlay.scale).toBe(4);
		expect(ctl.toasts.some((t) => t.text.includes('adjusted'))).toBe(true);
	});

	it('does not save while nothing changed', async () => {
		const { ctl } = controller();
		await ctl.init();
		ctl.draftChanged(clone(ctl.settings!));
		await ctl.flush();
		expect(ctl.saveState).toBe('idle');
	});

	it('adopts settings a command returned and refreshes the addresses on a new port', async () => {
		const { ctl } = controller();
		await ctl.init();
		const s = clone(ctl.settings!);
		s.server.port = 47900;
		ctl.adopt(s);
		expect(ctl.settings!.server.port).toBe(47900);
		ctl.draftChanged(clone(s));
		await ctl.flush();
		expect(ctl.saveState).toBe('idle');
	});

	it('runs status commands and reports failures as toasts', async () => {
		const { ctl } = controller();
		await ctl.init();
		expect(await ctl.statusCommand(() => ctl.ipc.setPaused(true))).toBe(true);
		expect(ctl.status?.paused).toBe(true);
		expect(await ctl.statusCommand(() => ctl.ipc.sessionMarker('x'))).toBe(true);
		const failed = await ctl.run(() => ctl.ipc.sessionGet('missing'));
		expect(failed).toBeNull();
		const toast = ctl.toasts.at(-1)!;
		expect(toast.tone).toBe('danger');
		ctl.dismiss(toast.id);
		expect(ctl.toasts.find((t) => t.id === toast.id)).toBeUndefined();
	});

	it('follows status events', async () => {
		const { ctl } = controller();
		await ctl.init();
		await ctl.ipc.setKeyboardVisible(false);
		expect(ctl.status?.keyboard_visible).toBe(false);
	});

	it('reports a backend that cannot load the settings', async () => {
		const backend = createMockBackend({ scenario: 'normal', latencyMs: 0, liveDemo: false });
		const ipc = new KeystageIpc({
			...backend,
			invoke: async () => Promise.reject('The app is not running.'),
		});
		const ctl = new SettingsController(ipc);
		await ctl.init();
		expect(ctl.loadError).toBe('The app is not running.');
		expect(ctl.settings).toBeNull();
	});

	it('moves between pages', () => {
		const { ctl } = controller();
		ctl.go('obs');
		expect(ctl.page).toBe('obs');
		ctl.goFromHash('#sound');
		expect(ctl.page).toBe('sound');
		ctl.goFromHash('#nothing');
		expect(ctl.page).toBe('sound');
	});
});
