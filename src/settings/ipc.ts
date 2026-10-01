/**
 * The settings window's typed IPC client (ARCHITECTURE.md section 12).
 *
 * Every method calls exactly one Tauri command with the argument shape the
 * contract lists (Tauri turns camelCase argument keys into snake_case Rust
 * parameters; every key here is already one word or snake_case, so nothing
 * is renamed). src-tauri/src/app/commands.rs implements these names.
 *
 * Outside the app (a plain browser, screenshots, unit tests) there is no
 * `window.__TAURI_INTERNALS__`, and `connectIpc()` returns the same client
 * over the in-memory mock backend in ./mock/backend.ts instead.
 *
 * Commands this window needs that the section 12 table does not list are
 * marked "Addition" below and are requested from the app shell in
 * .work/notes/settings.md.
 *
 * Privacy: nothing here logs. Arguments may hold an OBS password (only in
 * `obsTest` and `obsSetPassword`) or pasted shortcut text; they go to the
 * app and nowhere else.
 */
import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { listen as tauriListen } from '@tauri-apps/api/event';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import { open as dialogOpen, save as dialogSave } from '@tauri-apps/plugin-dialog';
import { openUrl as openerOpenUrl } from '@tauri-apps/plugin-opener';
import type { ServerMsg, SolidworksMode } from '../lib/protocol';
import { normalizeExportResult, normalizeSettings, normalizeStatus } from './defaults';
import type {
	DialogFilter,
	ExportFormat,
	ExportResult,
	FolderKind,
	HotkeyState,
	ImportPreview,
	ImportSource,
	Monitor,
	ObsPasswordResult,
	ObsTestResult,
	PlatformInfo,
	ProfileData,
	ServerUrls,
	SessionSummary,
	Settings,
	Status,
} from './types';

export * from './types';

/** Command names, exactly as the app registers them. */
export const COMMANDS = {
	getSettings: 'get_settings',
	saveSettings: 'save_settings',
	resetSettings: 'reset_settings',
	getStatus: 'get_status',
	listMonitors: 'list_monitors',
	setPaused: 'set_paused',
	setSolidworksMode: 'set_solidworks_mode',
	setKeyboardVisible: 'set_keyboard_visible',
	obsTest: 'obs_test',
	obsSetPassword: 'obs_set_password',
	shortcutsImportPreview: 'shortcuts_import_preview',
	shortcutsImportApply: 'shortcuts_import_apply',
	shortcutsImportClear: 'shortcuts_import_clear',
	profileShortcuts: 'profile_shortcuts',
	sessionStart: 'session_start',
	sessionStop: 'session_stop',
	sessionMarker: 'session_marker',
	sessionsList: 'sessions_list',
	sessionGet: 'session_get',
	sessionRenameMarker: 'session_rename_marker',
	sessionExport: 'session_export',
	sessionDelete: 'session_delete',
	settingsExport: 'settings_export',
	settingsImport: 'settings_import',
	serverUrls: 'server_urls',
	playSoundPreview: 'play_sound_preview',
	openExternalFolder: 'open_external_folder',
	/** Addition (also used by the renderer's TauriTransport). */
	overlayHello: 'overlay_hello',
	/** Addition. */
	hotkeysCapture: 'hotkeys_capture',
	/** Addition. */
	hotkeysStatus: 'hotkeys_status',
	/** Addition. */
	platformInfo: 'platform_info',
} as const;
export type CommandName = (typeof COMMANDS)[keyof typeof COMMANDS];

/** Events the app emits to this window. */
export const STATUS_EVENT = 'keystage://status';
export const OVERLAY_EVENT = 'keystage://overlay';
/**
 * Addition: the stored settings changed (from the tray, a hotkey, a command
 * or a save). Requested from the app shell in .work/notes/fix-requests.md.
 * The payload is not read: the window reads `get_settings` in order with its
 * own saves, so a late event can never roll it back.
 */
export const SETTINGS_EVENT = 'keystage://settings';

/**
 * What the client needs from its host: commands, events, and the three
 * plugins the window uses (clipboard, opener, dialog).
 */
export interface Backend {
	readonly kind: 'tauri' | 'mock';
	invoke(cmd: CommandName, args?: Record<string, unknown>): Promise<unknown>;
	/** Resolves to an unlisten function. */
	listen(event: string, handler: (payload: unknown) => void): Promise<() => void>;
	copyText(text: string): Promise<void>;
	openUrl(url: string): Promise<void>;
	pickOpenFile(opts: { title: string; filters: DialogFilter[] }): Promise<string | null>;
	pickFolder(opts: { title: string; defaultPath?: string }): Promise<string | null>;
	pickSaveFile(opts: {
		title: string;
		defaultPath: string;
		filters: DialogFilter[];
	}): Promise<string | null>;
}

/** An IPC failure with a readable message. */
export class IpcError extends Error {
	readonly command: string;
	constructor(command: string, message: string) {
		super(message);
		this.name = 'IpcError';
		this.command = command;
	}
}

/**
 * A readable message from anything a command can reject with: Tauri rejects
 * with the Rust error serialized (usually a string, sometimes an object with
 * `message`).
 */
export function errorMessage(e: unknown): string {
	if (typeof e === 'string' && e.trim().length > 0) return e.trim();
	if (e instanceof Error && e.message.trim().length > 0) return e.message.trim();
	if (typeof e === 'object' && e !== null) {
		const m =
			(e as { message?: unknown; error?: unknown }).message ?? (e as { error?: unknown }).error;
		if (typeof m === 'string' && m.trim().length > 0) return m.trim();
	}
	return 'The app did not answer. Try again.';
}

/** Only https links from the profile data are opened in the browser. */
export function isSafeExternalUrl(url: string): boolean {
	try {
		const u = new URL(url);
		return (
			u.protocol === 'https:' && u.hostname.length > 0 && u.username === '' && u.password === ''
		);
	} catch {
		return false;
	}
}

export class KeystageIpc {
	readonly backend: Backend;

	constructor(backend: Backend) {
		this.backend = backend;
	}

	get kind(): 'tauri' | 'mock' {
		return this.backend.kind;
	}

	private async call<T>(cmd: CommandName, args?: Record<string, unknown>): Promise<T> {
		try {
			return (await this.backend.invoke(cmd, args)) as T;
		} catch (e) {
			throw new IpcError(cmd, errorMessage(e));
		}
	}

	/* --- Settings --------------------------------------------------------- */

	async getSettings(): Promise<Settings> {
		return normalizeSettings(await this.call<unknown>(COMMANDS.getSettings));
	}
	/** Validated, persisted and pushed to every host; returns what was stored. */
	async saveSettings(settings: Settings): Promise<Settings> {
		return normalizeSettings(await this.call<unknown>(COMMANDS.saveSettings, { settings }));
	}
	async resetSettings(): Promise<Settings> {
		return normalizeSettings(await this.call<unknown>(COMMANDS.resetSettings));
	}
	async settingsExport(path: string): Promise<Settings> {
		return normalizeSettings(await this.call<unknown>(COMMANDS.settingsExport, { path }));
	}
	async settingsImport(path: string): Promise<Settings> {
		return normalizeSettings(await this.call<unknown>(COMMANDS.settingsImport, { path }));
	}

	/* --- Status and live switches ----------------------------------------- */

	async getStatus(): Promise<Status> {
		return normalizeStatus(await this.call<unknown>(COMMANDS.getStatus));
	}
	async setPaused(paused: boolean): Promise<Status> {
		return normalizeStatus(await this.call<unknown>(COMMANDS.setPaused, { paused }));
	}
	async setSolidworksMode(mode: SolidworksMode): Promise<Status> {
		return normalizeStatus(await this.call<unknown>(COMMANDS.setSolidworksMode, { mode }));
	}
	async setKeyboardVisible(visible: boolean): Promise<Status> {
		return normalizeStatus(await this.call<unknown>(COMMANDS.setKeyboardVisible, { visible }));
	}
	async listMonitors(): Promise<Monitor[]> {
		const list = await this.call<unknown>(COMMANDS.listMonitors);
		return Array.isArray(list) ? (list as Monitor[]) : [];
	}
	async serverUrls(): Promise<ServerUrls> {
		return this.call<ServerUrls>(COMMANDS.serverUrls);
	}

	/* --- OBS -------------------------------------------------------------- */

	/** `password` null uses the stored one (or none). Never stored by this call. */
	async obsTest(host: string, port: number, password: string | null): Promise<ObsTestResult> {
		return this.call<ObsTestResult>(COMMANDS.obsTest, { host, port, password });
	}
	/** Stores the password as a DPAPI blob; null clears it. */
	async obsSetPassword(password: string | null): Promise<ObsPasswordResult> {
		return this.call<ObsPasswordResult>(COMMANDS.obsSetPassword, { password });
	}

	/* --- SolidWorks shortcuts --------------------------------------------- */

	async shortcutsImportPreview(source: ImportSource): Promise<ImportPreview> {
		return this.call<ImportPreview>(COMMANDS.shortcutsImportPreview, { source });
	}
	async shortcutsImportApply(source: ImportSource): Promise<Settings> {
		return normalizeSettings(await this.call<unknown>(COMMANDS.shortcutsImportApply, { source }));
	}
	async shortcutsImportClear(): Promise<Settings> {
		return normalizeSettings(await this.call<unknown>(COMMANDS.shortcutsImportClear));
	}
	/** The merged profile (built-in data plus user overrides) for display. */
	async profileShortcuts(id: string): Promise<ProfileData> {
		return this.call<ProfileData>(COMMANDS.profileShortcuts, { id });
	}

	/* --- Sessions --------------------------------------------------------- */

	async sessionStart(): Promise<Status> {
		return normalizeStatus(await this.call<unknown>(COMMANDS.sessionStart));
	}
	async sessionStop(): Promise<Status> {
		return normalizeStatus(await this.call<unknown>(COMMANDS.sessionStop));
	}
	async sessionMarker(name: string | null): Promise<Status> {
		return normalizeStatus(await this.call<unknown>(COMMANDS.sessionMarker, { name }));
	}
	async sessionsList(): Promise<SessionSummary[]> {
		const list = await this.call<unknown>(COMMANDS.sessionsList);
		return Array.isArray(list) ? (list as SessionSummary[]) : [];
	}
	/** The session JSON (keystage.session/1); read it with session.ts. */
	async sessionGet(id: string): Promise<unknown> {
		return this.call<unknown>(COMMANDS.sessionGet, { id });
	}
	async sessionRenameMarker(id: string, index: number, name: string): Promise<unknown> {
		return this.call<unknown>(COMMANDS.sessionRenameMarker, { id, index, name });
	}
	/** Writes the export; `warnings` say what the user should know about it (often none). */
	async sessionExport(id: string, format: ExportFormat, path: string): Promise<ExportResult> {
		const r = await this.call<unknown>(COMMANDS.sessionExport, { id, format, path });
		return normalizeExportResult(r, path);
	}
	async sessionDelete(id: string): Promise<SessionSummary[]> {
		const list = await this.call<unknown>(COMMANDS.sessionDelete, { id });
		return Array.isArray(list) ? (list as SessionSummary[]) : [];
	}

	/* --- Sound, folders --------------------------------------------------- */

	async playSoundPreview(pack: string): Promise<void> {
		await this.call<null>(COMMANDS.playSoundPreview, { pack });
	}
	async openExternalFolder(which: FolderKind): Promise<void> {
		await this.call<null>(COMMANDS.openExternalFolder, { which });
	}

	/* --- Additions -------------------------------------------------------- */

	/**
	 * Addition: the hello snapshot for the live preview, the same command the
	 * renderer's TauriTransport calls. Returns the `hello` message.
	 */
	async overlayHello(): Promise<ServerMsg> {
		return this.call<ServerMsg>(COMMANDS.overlayHello);
	}
	/**
	 * Addition: while a hotkey capture field is waiting for a shortcut, the
	 * app unregisters its global hotkeys (`active: true`) so the shortcut
	 * being typed reaches this window instead of firing; `active: false`
	 * registers them again. The app should also re-register after 30 s
	 * without a call, in case this window closes mid-capture.
	 */
	async hotkeysCapture(active: boolean): Promise<void> {
		await this.call<null>(COMMANDS.hotkeysCapture, { active });
	}
	/** Addition: whether Windows accepted each global hotkey (registration conflicts). */
	async hotkeysStatus(): Promise<HotkeyState[]> {
		const list = await this.call<unknown>(COMMANDS.hotkeysStatus);
		return Array.isArray(list) ? (list as HotkeyState[]) : [];
	}
	/** Addition: the Windows build, for options that need a minimum build. */
	async platformInfo(): Promise<PlatformInfo> {
		return this.call<PlatformInfo>(COMMANDS.platformInfo);
	}

	/* --- Events ----------------------------------------------------------- */

	/** `keystage://status`, emitted on every change. */
	onStatus(handler: (status: Status) => void): Promise<() => void> {
		return this.backend.listen(STATUS_EVENT, (payload) => handler(normalizeStatus(payload)));
	}
	/** Addition: `keystage://settings`, the stored settings changed. */
	onSettingsChanged(handler: () => void): Promise<() => void> {
		return this.backend.listen(SETTINGS_EVENT, () => handler());
	}

	/* --- Plugins ---------------------------------------------------------- */

	copyText(text: string): Promise<void> {
		return this.backend.copyText(text);
	}
	/** Opens an https link in the default browser; anything else is refused. */
	async openUrl(url: string): Promise<void> {
		if (!isSafeExternalUrl(url)) throw new IpcError('open_url', 'Only https links can be opened.');
		await this.backend.openUrl(url);
	}
	pickOpenFile(title: string, filters: DialogFilter[]): Promise<string | null> {
		return this.backend.pickOpenFile({ title, filters });
	}
	pickFolder(title: string, defaultPath?: string): Promise<string | null> {
		return this.backend.pickFolder({ title, defaultPath });
	}
	pickSaveFile(
		title: string,
		defaultPath: string,
		filters: DialogFilter[]
	): Promise<string | null> {
		return this.backend.pickSaveFile({ title, defaultPath, filters });
	}
}

/* ------------------------------------------------------------------------ */
/* Backends                                                                  */
/* ------------------------------------------------------------------------ */

export function hasTauriRuntime(w: unknown = globalThis): boolean {
	return typeof w === 'object' && w !== null && '__TAURI_INTERNALS__' in w;
}

/** The app's backend: Tauri commands, events and plugins. */
export function tauriBackend(): Backend {
	return {
		kind: 'tauri',
		invoke: (cmd, args) => tauriInvoke(cmd, args),
		listen: async (event, handler) => tauriListen(event, (e) => handler(e.payload)),
		copyText: (text) => writeText(text),
		openUrl: (url) => openerOpenUrl(url),
		pickOpenFile: async ({ title, filters }) => {
			const r = await dialogOpen({ title, filters, multiple: false, directory: false });
			return typeof r === 'string' ? r : null;
		},
		pickFolder: async ({ title, defaultPath }) => {
			const r = await dialogOpen({ title, defaultPath, multiple: false, directory: true });
			return typeof r === 'string' ? r : null;
		},
		pickSaveFile: async ({ title, defaultPath, filters }) => {
			const r = await dialogSave({ title, defaultPath, filters });
			return typeof r === 'string' ? r : null;
		},
	};
}

/**
 * The client for this page: Tauri inside the app, the mock backend
 * elsewhere (`search` picks the mock's scenario: `?state=paused` and so on).
 */
export async function connectIpc(search = ''): Promise<KeystageIpc> {
	if (hasTauriRuntime()) return new KeystageIpc(tauriBackend());
	const mock = await import('./mock/backend');
	return new KeystageIpc(mock.createMockBackend({ scenario: mock.scenarioFrom(search) }));
}
