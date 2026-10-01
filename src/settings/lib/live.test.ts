import { afterEach, describe, expect, it } from 'vitest';
import { clearMocks, mockIPC, mockWindows } from '@tauri-apps/api/mocks';
import { KeystageIpc, STATUS_EVENT, tauriBackend } from '../ipc';
import { createMockBackend } from '../mock/backend';
import type { ServerMsg } from '../../lib/protocol';
import { liveTransport } from './live';

interface Call {
	cmd: string;
	args: Record<string, unknown>;
}

/** Mocks the settings window's Tauri IPC and records every call. */
function mockSettingsWindow(): Call[] {
	mockWindows('settings');
	const calls: Call[] = [];
	let nextId = 1;
	mockIPC((cmd, args) => {
		calls.push({ cmd, args: (args ?? {}) as Record<string, unknown> });
		if (cmd === 'plugin:event|listen') return nextId++;
		return null;
	});
	return calls;
}

async function until(cond: () => boolean): Promise<void> {
	for (let i = 0; i < 50 && !cond(); i++) await new Promise((r) => setTimeout(r, 0));
}

const SETTINGS_WINDOW = { kind: 'WebviewWindow', label: 'settings' };

describe('settings window events stay on this window', () => {
	afterEach(() => {
		clearMocks();
		const g = globalThis as Record<string, unknown>;
		delete g.__TAURI_INTERNALS__;
		delete g.__TAURI_EVENT_PLUGIN_INTERNALS__;
	});

	it('live input listens for keystage://overlay on the settings window, never Any', async () => {
		const calls = mockSettingsWindow();
		const t = liveTransport(new KeystageIpc(tauriBackend()));
		t.start({ message: () => {}, status: () => {} });
		await until(() => calls.some((c) => c.cmd === 'plugin:event|listen'));
		t.stop();

		const listen = calls.find((c) => c.cmd === 'plugin:event|listen');
		expect(listen?.args.event).toBe('keystage://overlay');
		// Tauri delivers every emit to an `Any` listener whatever filter the
		// shell passes to emit_str_filter, so the minimized-preview gate
		// (wants_overlay_event) only works for a window-scoped listener.
		expect(listen?.args.target).toEqual(SETTINGS_WINDOW);
	});

	it('the backend listens on the settings window too (status, settings)', async () => {
		const calls = mockSettingsWindow();
		const ipc = new KeystageIpc(tauriBackend());
		const un = await ipc.onStatus(() => {});
		const listen = calls.find((c) => c.cmd === 'plugin:event|listen');
		expect(listen?.args.event).toBe(STATUS_EVENT);
		expect(listen?.args.target).toEqual(SETTINGS_WINDOW);
		un();
	});

	it('in a browser the live input still runs on the mock backend', async () => {
		const backend = createMockBackend({ scenario: 'normal', latencyMs: 0, liveDemo: false });
		const t = liveTransport(new KeystageIpc(backend));
		const messages: ServerMsg[] = [];
		t.start({ message: (m) => messages.push(m), status: () => {} });
		await until(() => messages.some((m) => m.t === 'hello'));
		t.stop();
		expect(messages.some((m) => m.t === 'hello')).toBe(true);
	});
});
