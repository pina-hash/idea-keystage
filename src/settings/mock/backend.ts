/**
 * The in-memory mock backend, used when the page runs without the app (a
 * plain browser, screenshots, unit tests). It answers every command the IPC
 * client calls with realistic data and keeps state between calls, so the
 * whole window can be driven and screenshotted in Chromium.
 *
 * `?state=` on settings.html picks a scenario (see fixtures.ts). Live input
 * is simulated: listening to `keystage://overlay` plays the built-in demo
 * through the same event, and the preview says it is simulated.
 *
 * Nothing here reaches the network or the disk.
 */
import {
	APP_NAME,
	PROTOCOL_VERSION,
	ROUTES,
	type AppState,
	type ServerMsg,
	type SolidworksMode,
} from '../../lib/protocol';
import { obsSourceSize } from '../../lib/renderer/sizes';
import { DemoTransport, systemTimers, type Timers } from '../../lib/renderer/transport';
import type { Backend, CommandName } from '../ipc';
import { clone, normalizeSettings } from '../defaults';
import { normalizeAccelerator } from '../lib/accelerator';
import { normalizeExcludedApps } from '../lib/excluded';
import { chosenMonitor, FALLBACK_MONITOR } from '../lib/monitors';
import { routeUrl } from '../lib/obs';
import { HOTKEY_NAMES } from '../types';
import type {
	DialogFilter,
	ExportFormat,
	HotkeyState,
	ImportSource,
	Monitor,
	ProfileData,
	ServerUrls,
	Settings,
	Status,
} from '../types';
import {
	MOCK_DOCS,
	MOCK_DPAPI_BLOB,
	MOCK_EXPORT_WARNINGS,
	MOCK_VERSION,
	mockMonitors,
	mockSessions,
	mockSettings,
	mockStatus,
	sampleCopyList,
	scenarioFrom,
	sessionId,
	sessionJson,
	type MockSession,
	type Scenario,
} from './fixtures';
import { parseCopyList, previewImport, sldregResult, type MockImportResult } from './import';

export { scenarioFrom, type Scenario };

export interface MockOptions {
	scenario: Scenario;
	/** Loads the built-in SolidWorks profile; defaults to the data file. */
	loadProfile?: () => Promise<ProfileData>;
	/** Wall clock in ms; defaults to Date.now. */
	now?: () => number;
	timers?: Timers;
	/** Called by copyText (defaults to the browser clipboard). */
	clipboard?: (text: string) => Promise<void>;
	/** Called by openUrl (defaults to window.open). */
	open?: (url: string) => void;
	/** Simulated answer time in ms for commands (0 in tests). */
	latencyMs?: number;
	/** Play the demo on `keystage://overlay` listeners (on by default). */
	liveDemo?: boolean;
}

export interface MockBackend extends Backend {
	readonly scenario: Scenario;
	/** For tests: the current stored settings and status. */
	peek(): {
		settings: Settings;
		status: Status;
		sessions: MockSession[];
		files: Map<string, string>;
	};
	/**
	 * For tests: the app changes the stored settings itself, as the tray's
	 * SolidWorks mode and Native overlay items and the mode hotkey do. Emits
	 * the status and, unless `settingsEvent` is false (an app without
	 * `keystage://settings`), the settings event.
	 */
	changeFromApp(edit: (s: Settings) => void, opts?: { settingsEvent?: boolean }): void;
	/** For tests: replaces `Status.notices` and emits the status. */
	setNotices(notices: string[]): void;
}

async function loadBuiltinProfile(): Promise<ProfileData> {
	const mod = await import('$data/profiles/solidworks.json');
	return mod.default as unknown as ProfileData;
}

/** Hotkeys Windows itself owns: RegisterHotKey fails for them. */
const WINDOWS_RESERVED = new Set(['Ctrl+Alt+Delete', 'Win+L']);

class MockError extends Error {}

export function createMockBackend(opts: MockOptions): MockBackend {
	const scenario = opts.scenario;
	const now = opts.now ?? (() => Date.now());
	const timers = opts.timers ?? systemTimers;
	const latency = opts.latencyMs ?? 60;
	const startedAt = now();

	let settings = mockSettings(scenario);
	const monitors: Monitor[] = mockMonitors(scenario);
	let sessions: MockSession[] = mockSessions(scenario, startedAt);
	const files = new Map<string, string>();
	let builtin: ProfileData | null = null;
	const loadProfile = opts.loadProfile ?? loadBuiltinProfile;

	let status: Status = mockStatus(scenario, settings, startedAt);
	/** The recording timecode advances from here (ms of recording at `startedAt`). */
	const recordBase = 12 * 60_000 + 41_233;
	const recordingSince = startedAt;
	let sessionMarkers = status.session?.markers ?? 0;
	let liveSessionMarkers: { t_ms: number; name: string }[] = [];

	const listeners = new Map<string, Set<(payload: unknown) => void>>();
	let ticker: unknown = null;
	let demo: DemoTransport | null = null;

	async function profile(): Promise<ProfileData> {
		builtin ??= await loadProfile();
		return builtin;
	}

	function timecode(): string {
		const ms = Math.max(0, recordBase + (now() - recordingSince));
		const h = Math.floor(ms / 3_600_000);
		const m = Math.floor((ms % 3_600_000) / 60_000);
		const s = Math.floor((ms % 60_000) / 1000);
		const f = ms % 1000;
		return `${String(h).padStart(2, '0')}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}.${String(f).padStart(3, '0')}`;
	}

	function currentStatus(): Status {
		const s = clone(status);
		if (s.obs.recording?.active && !s.obs.recording.paused) s.obs.recording.timecode = timecode();
		if (s.session) s.session.markers = sessionMarkers;
		return s;
	}

	function emit(event: string, payload: unknown): void {
		for (const fn of listeners.get(event) ?? []) fn(payload);
	}

	function appState(): AppState {
		const s = currentStatus();
		const obsInUse = s.server.clients > 0 && settings.host.hide_native_when_obs;
		return {
			paused: s.paused,
			suppressed: s.suppressed,
			profile: s.profile,
			solidworks_mode: s.solidworks_mode,
			keyboard_visible: s.keyboard_visible,
			native_visible: settings.host.native_overlay && !obsInUse && !s.paused,
			obs_clients: s.server.clients,
			recording: s.obs.recording,
			session: s.session
				? {
						active: s.session.active,
						started_ms: 0,
						markers: s.session.markers,
					}
				: null,
		};
	}

	function hello(): ServerMsg {
		return {
			t: 'hello',
			v: PROTOCOL_VERSION,
			app: APP_NAME,
			version: MOCK_VERSION,
			config: clone(settings.overlay),
			state: appState(),
			legends: null,
		};
	}

	function statusChanged(): void {
		emit('keystage://status', currentStatus());
		emit('keystage://overlay', { t: 'state', state: appState() });
	}

	/** `keystage://settings`: the stored settings changed (the password blob is left out). */
	function settingsChanged(): void {
		const out = clone(settings);
		out.obs.password_dpapi = null;
		emit('keystage://settings', out);
	}

	function ensureTicker(): void {
		const want = (listeners.get('keystage://status')?.size ?? 0) > 0;
		if (want && ticker === null) {
			ticker = timers.setInterval(() => {
				if (status.obs.recording?.active) emit('keystage://status', currentStatus());
			}, 1000);
		} else if (!want && ticker !== null) {
			timers.clearInterval(ticker);
			ticker = null;
		}
	}

	function ensureDemo(): void {
		const want = opts.liveDemo !== false && (listeners.get('keystage://overlay')?.size ?? 0) > 0;
		if (want && demo === null) {
			demo = new DemoTransport({ script: 'tour', route: 'all', timers });
			demo.start({
				message: (msg) => {
					// The app sends its own config and state; while paused it sends no input.
					if (msg.t === 'hello' || msg.t === 'config' || msg.t === 'state') return;
					if (status.paused || status.suppressed) return;
					emit('keystage://overlay', msg);
				},
				status: () => {},
			});
		} else if (!want && demo !== null) {
			demo.stop();
			demo = null;
		}
	}

	function serverUrls(): ServerUrls {
		const m = chosenMonitor(monitors, settings.host.monitor) ?? FALLBACK_MONITOR;
		const port = status.server.port;
		return {
			port,
			running: status.server.running,
			error: status.server.error,
			routes: ROUTES.map((route) => {
				const size = obsSourceSize(route, settings.overlay, m);
				return { route, url: routeUrl(port, route), width: size.width, height: size.height };
			}),
		};
	}

	/** keystage-core-like validation of a settings object from the window. */
	function validate(raw: unknown): Settings {
		const s = normalizeSettings(raw);
		for (const name of HOTKEY_NAMES) {
			s.hotkeys[name] = normalizeAccelerator(s.hotkeys[name]) ?? settings.hotkeys[name];
		}
		s.privacy.excluded_apps = normalizeExcludedApps(s.privacy.excluded_apps);
		// The window never writes the password blob; keep the stored one.
		s.obs.password_dpapi = settings.obs.password_dpapi;
		if (s.server.port < 1024) s.server.port = settings.server.port;
		return s;
	}

	function applySettings(next: Settings): void {
		const portChanged = next.server.port !== settings.server.port;
		settings = next;
		status.solidworks_mode = next.profiles.solidworks_mode;
		status.obs.enabled = next.obs.enabled;
		if (!next.obs.enabled) {
			status.obs.state = 'off';
			status.obs.error = null;
			status.obs.recording = null;
		} else if (status.obs.state === 'off') {
			status.obs.state = 'connected';
		}
		if (portChanged) {
			status.server = {
				port: next.server.port,
				running: true,
				error: null,
				clients: 0,
			};
		}
		emit('keystage://overlay', { t: 'config', config: clone(settings.overlay) });
		statusChanged();
		settingsChanged();
	}

	async function importResult(source: ImportSource): Promise<MockImportResult> {
		if (source.kind === 'text') {
			if (/^\s*(Windows Registry Editor|REGEDIT4)/.test(source.text)) return sldregResult();
			return parseCopyList(source.text);
		}
		const path = source.path.toLowerCase();
		if (path.endsWith('.sldreg')) return sldregResult();
		if (!path.endsWith('.txt')) {
			return {
				format: 'unknown',
				entries: [],
				unparsed: 0,
				warnings: ['The file is not text. Paste the Copy List from the Keyboard tab instead.'],
			};
		}
		return parseCopyList(sampleCopyList(await profile(), scenario === 'import-warnings'));
	}

	async function mergedProfile(): Promise<ProfileData> {
		const base = await profile();
		const user = settings.profiles.solidworks_user_shortcuts.map((u, i) => ({
			id: `user-${String(i + 1).padStart(3, '0')}`,
			keys: [...u.keys],
			doc_keys: u.doc_keys,
			command: u.command,
			description: '',
			group: u.category,
			context: '',
			versions: [],
			source_ids: [],
			quote: '',
			source: 'user-import',
		}));
		return { ...base, shortcuts: [...user, ...base.shortcuts] };
	}

	function findSession(id: string): MockSession {
		const s = sessions.find((x) => x.summary.id === id);
		if (!s) throw new MockError('That session is no longer in the sessions folder.');
		return s;
	}

	async function handle(cmd: CommandName, args: Record<string, unknown>): Promise<unknown> {
		switch (cmd) {
			case 'get_settings':
				return clone(settings);
			case 'save_settings': {
				applySettings(validate(args.settings));
				return clone(settings);
			}
			case 'reset_settings': {
				const fresh = mockSettings('empty');
				fresh.first_run_done = true;
				fresh.obs.password_dpapi = null;
				applySettings(fresh);
				return clone(settings);
			}
			case 'get_status':
				return currentStatus();
			case 'list_monitors':
				return clone(monitors);
			case 'set_paused': {
				status.paused = args.paused === true;
				status.hook.installed = !status.paused;
				statusChanged();
				return currentStatus();
			}
			case 'set_solidworks_mode': {
				const mode = args.mode as SolidworksMode;
				if (mode !== 'auto' && mode !== 'on' && mode !== 'off')
					throw new MockError('Unknown mode.');
				settings.profiles.solidworks_mode = mode;
				status.solidworks_mode = mode;
				status.profile = mode === 'off' ? null : { id: 'solidworks', name: 'SolidWorks' };
				statusChanged();
				settingsChanged();
				return currentStatus();
			}
			case 'set_keyboard_visible': {
				status.keyboard_visible = args.visible === true;
				statusChanged();
				return currentStatus();
			}
			case 'obs_test': {
				const host = String(args.host ?? '').trim();
				const port = Number(args.port);
				if (!host || !Number.isInteger(port) || port < 1 || port > 65535) {
					return { ok: false, error: 'Type a host and a port from 1 to 65535.' };
				}
				if (
					scenario === 'obs-error' &&
					(typeof args.password !== 'string' || args.password === '')
				) {
					return { ok: false, error: 'OBS did not accept the password.' };
				}
				if (scenario === 'empty' || scenario === 'first-run') {
					return {
						ok: false,
						error: `Nothing answered at ${host}:${port}. Is OBS running with its WebSocket server enabled?`,
					};
				}
				return { ok: true, obs_version: '32.0.1', ws_version: '5.6.2' };
			}
			case 'obs_set_password': {
				const pw = args.password;
				settings.obs.password_dpapi =
					typeof pw === 'string' && pw.length > 0 ? MOCK_DPAPI_BLOB : null;
				if (scenario === 'obs-error' && settings.obs.password_dpapi) {
					status.obs.state = 'connected';
					status.obs.error = null;
				}
				statusChanged();
				settingsChanged();
				return { stored: settings.obs.password_dpapi !== null };
			}
			case 'shortcuts_import_preview': {
				const result = await importResult(args.source as ImportSource);
				return previewImport(result, await profile());
			}
			case 'shortcuts_import_apply': {
				const result = await importResult(args.source as ImportSource);
				if (result.entries.length === 0)
					throw new MockError('There is nothing to apply in that import.');
				settings.profiles.solidworks_user_shortcuts = result.entries.map((e) => ({ ...e }));
				statusChanged();
				settingsChanged();
				return clone(settings);
			}
			case 'shortcuts_import_clear': {
				settings.profiles.solidworks_user_shortcuts = [];
				settingsChanged();
				return clone(settings);
			}
			case 'profile_shortcuts': {
				if (args.id !== 'solidworks')
					throw new MockError(`There is no profile "${String(args.id)}".`);
				return mergedProfile();
			}
			case 'session_start': {
				if (!status.session?.active) {
					status.session = {
						id: 'live',
						active: true,
						started: new Date(now()).toISOString(),
						callouts: 0,
						markers: 0,
					};
					sessionMarkers = 0;
					liveSessionMarkers = [];
				}
				statusChanged();
				return currentStatus();
			}
			case 'session_stop': {
				if (status.session?.active) {
					const started = status.session.started;
					const duration = Math.max(1000, now() - new Date(started).getTime());
					const id = sessionId(started);
					sessions = [
						{
							summary: {
								id,
								started,
								duration_ms: duration,
								callouts: status.session.callouts,
								markers: sessionMarkers,
							},
							markers: liveSessionMarkers,
						},
						...sessions,
					];
					status.session = null;
				}
				statusChanged();
				return currentStatus();
			}
			case 'session_marker': {
				if (!status.session?.active) throw new MockError('Start a session log first.');
				sessionMarkers += 1;
				const name =
					typeof args.name === 'string' && args.name.trim()
						? args.name.trim()
						: `Marker ${sessionMarkers}`;
				liveSessionMarkers.push({ t_ms: now() - new Date(status.session.started).getTime(), name });
				statusChanged();
				return currentStatus();
			}
			case 'sessions_list':
				return sessions.map((s) => ({ ...s.summary }));
			case 'session_get':
				return sessionJson(findSession(String(args.id)));
			case 'session_rename_marker': {
				const s = findSession(String(args.id));
				const index = Number(args.index);
				const m = s.markers[index];
				if (!m) throw new MockError('That marker no longer exists.');
				m.name = String(args.name ?? '').trim() || m.name;
				return sessionJson(s);
			}
			case 'session_export': {
				const session = findSession(String(args.id));
				const format = args.format as ExportFormat;
				if (!['srt', 'md', 'json', 'chapters'].includes(format))
					throw new MockError('Unknown format.');
				const path = String(args.path ?? '');
				if (!path) throw new MockError('Choose where to save the file.');
				files.set(path, format);
				// As keystage-session: the mock sessions have no OBS timing.
				const warnings: string[] = [];
				if (format !== 'json') warnings.push(MOCK_EXPORT_WARNINGS.sessionClock);
				if (format === 'chapters') {
					const named00 = session.markers.some((m) => m.t_ms < 1000);
					const count = session.markers.length + (named00 ? 0 : 1);
					if (count < 3) warnings.push(MOCK_EXPORT_WARNINGS.fewChapters(count));
				}
				return { path, warnings };
			}
			case 'session_delete': {
				findSession(String(args.id));
				sessions = sessions.filter((s) => s.summary.id !== args.id);
				return sessions.map((s) => ({ ...s.summary }));
			}
			case 'settings_export': {
				const path = String(args.path ?? '');
				// The DPAPI blob only opens on this account and computer; it is left out.
				const out = clone(settings);
				out.obs.password_dpapi = null;
				files.set(path, JSON.stringify(out, null, 2));
				return clone(settings);
			}
			case 'settings_import': {
				const path = String(args.path ?? '');
				const text = files.get(path);
				if (text === undefined)
					throw new MockError(`Could not read ${path}: the file does not exist.`);
				let parsed: unknown;
				try {
					parsed = JSON.parse(text);
				} catch {
					throw new MockError(`${path} is not a settings file.`);
				}
				applySettings(validate(parsed));
				return clone(settings);
			}
			case 'server_urls':
				return serverUrls();
			case 'play_sound_preview':
			case 'open_external_folder':
			case 'hotkeys_capture':
				return null;
			case 'overlay_hello':
				return hello();
			case 'hotkeys_status':
				return HOTKEY_NAMES.map((name): HotkeyState => {
					const accelerator = settings.hotkeys[name];
					const reserved = WINDOWS_RESERVED.has(accelerator);
					return {
						name,
						accelerator,
						registered: !reserved,
						error: reserved ? 'Windows reserves this shortcut.' : null,
					};
				});
			case 'platform_info':
				return { os_build: 26100, exclude_from_capture_supported: true };
		}
	}

	const backend: MockBackend = {
		kind: 'mock',
		scenario,
		async invoke(cmd, args = {}) {
			if (latency > 0) await new Promise((r) => timers.setTimeout(() => r(null), latency));
			try {
				return await handle(cmd, args);
			} catch (e) {
				// Tauri rejects with the Rust error as a string.
				throw e instanceof Error ? e.message : String(e);
			}
		},
		async listen(event, handler) {
			let set = listeners.get(event);
			if (!set) {
				set = new Set();
				listeners.set(event, set);
			}
			set.add(handler);
			ensureTicker();
			ensureDemo();
			return () => {
				set.delete(handler);
				ensureTicker();
				ensureDemo();
			};
		},
		async copyText(text) {
			if (opts.clipboard) return opts.clipboard(text);
			const nav = globalThis.navigator as Navigator | undefined;
			if (!nav?.clipboard?.writeText) throw 'The browser has no clipboard access here.';
			await nav.clipboard.writeText(text);
		},
		async openUrl(url) {
			if (opts.open) return opts.open(url);
			globalThis.open?.(url, '_blank', 'noopener,noreferrer');
		},
		async pickOpenFile({ filters }: { title: string; filters: DialogFilter[] }) {
			const exts = filters.flatMap((f) => f.extensions);
			if (exts.includes('json')) return `${MOCK_DOCS}\\keystage-settings.json`;
			return `${MOCK_DOCS}\\keyboard-copy-list.txt`;
		},
		async pickFolder() {
			return 'C:\\Users\\Teacher\\Videos\\Keystage sessions';
		},
		async pickSaveFile({ defaultPath }) {
			return `${MOCK_DOCS}\\${defaultPath}`;
		},
		peek() {
			return { settings: clone(settings), status: currentStatus(), sessions, files };
		},
		changeFromApp(edit, o = {}) {
			const next = clone(settings);
			edit(next);
			settings = next;
			status.solidworks_mode = next.profiles.solidworks_mode;
			statusChanged();
			if (o.settingsEvent !== false) settingsChanged();
		},
		setNotices(notices) {
			status.notices = [...notices];
			statusChanged();
		},
	};
	return backend;
}
