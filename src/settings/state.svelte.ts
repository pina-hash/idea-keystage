/**
 * The settings window's state (Svelte 5 runes): the settings being edited,
 * the app's live status, the monitors, the server addresses and the
 * SolidWorks profile, plus a debounced save that writes every edit back
 * through `save_settings` and adopts what the app stored.
 *
 * Pages read and bind to `ctl.settings` directly; App.svelte feeds a deep
 * snapshot of it to `draftChanged` from one effect, so any edit anywhere is
 * saved a moment later and shown in the live preview at once.
 *
 * Privacy: nothing here logs. Toasts name settings, never typed content.
 */
import { getContext, setContext } from 'svelte';
import { clone, sameJson } from './defaults';
import { errorMessage, type KeystageIpc } from './ipc';
import { SaveQueue, type SaveState } from './lib/save-queue';
import { isPageId, type PageId } from './lib/nav';
import { applyAppearance, saveAppearance, type Appearance } from './lib/appearance';
import type {
	HotkeyState,
	Monitor,
	PlatformInfo,
	ProfileData,
	ServerUrls,
	Settings,
	Status,
} from './types';

export interface Toast {
	id: number;
	tone: 'ok' | 'info' | 'danger';
	text: string;
}

export const SAVE_DELAY_MS = 350;
const TOAST_MS = 4200;

export class SettingsController {
	readonly ipc: KeystageIpc;

	settings = $state<Settings | null>(null);
	status = $state.raw<Status | null>(null);
	monitors = $state.raw<Monitor[]>([]);
	urls = $state.raw<ServerUrls | null>(null);
	platform = $state.raw<PlatformInfo | null>(null);
	hotkeyStates = $state.raw<HotkeyState[]>([]);
	profile = $state.raw<ProfileData | null>(null);
	profileError = $state<string | null>(null);
	loadError = $state<string | null>(null);
	saveState = $state<SaveState>('idle');
	saveError = $state<string | null>(null);
	page = $state<PageId>('home');
	toasts = $state<Toast[]>([]);
	/** Wall clock, ticking once a second for elapsed times. */
	now = $state(Date.now());
	/** This window's own look (not an overlay setting). */
	appearance = $state<Appearance>('dark');

	private readonly queue: SaveQueue<Settings>;
	private unlisten: (() => void)[] = [];
	private clock: ReturnType<typeof setInterval> | null = null;
	private nextToast = 1;
	private lastPort: number | null = null;
	private profileLoading: Promise<void> | null = null;

	constructor(ipc: KeystageIpc, opts: { saveDelayMs?: number } = {}) {
		this.ipc = ipc;
		this.queue = new SaveQueue<Settings>({
			delayMs: opts.saveDelayMs ?? SAVE_DELAY_MS,
			save: (s) => this.ipc.saveSettings(s),
			equal: sameJson,
			describeError: errorMessage,
			onState: (state, error) => {
				this.saveState = state;
				this.saveError = error;
			},
			onSaved: (sent, stored) => this.onSaved(sent, stored),
		});
	}

	/** Loads everything the window shows and subscribes to status changes. */
	async init(): Promise<void> {
		try {
			const [settings, status] = await Promise.all([this.ipc.getSettings(), this.ipc.getStatus()]);
			this.queue.setBaseline(clone(settings));
			this.lastPort = settings.server.port;
			this.settings = settings;
			this.status = status;
			this.loadError = null;
		} catch (e) {
			this.loadError = errorMessage(e);
			return;
		}
		const results = await Promise.allSettled([
			this.ipc.listMonitors().then((m) => (this.monitors = m)),
			this.ipc.serverUrls().then((u) => (this.urls = u)),
			this.ipc.platformInfo().then((p) => (this.platform = p)),
			this.ipc.hotkeysStatus().then((h) => (this.hotkeyStates = h)),
			this.ipc.onStatus((s) => this.onStatus(s)).then((un) => this.unlisten.push(un)),
		]);
		for (const r of results) {
			if (r.status === 'rejected') this.toast(errorMessage(r.reason), 'danger');
		}
		this.clock = setInterval(() => (this.now = Date.now()), 1000);
		void this.loadProfile();
	}

	destroy(): void {
		for (const un of this.unlisten) {
			try {
				un();
			} catch {
				/* the window is closing */
			}
		}
		this.unlisten = [];
		if (this.clock) clearInterval(this.clock);
		this.clock = null;
	}

	/* --- Saving ------------------------------------------------------------ */

	/** Every edit lands here (a deep snapshot of `settings`). */
	draftChanged(snapshot: Settings): void {
		this.queue.push(snapshot);
	}

	/** Sends a waiting edit now (before a command that replaces the settings). */
	flush(): Promise<void> {
		return this.queue.flush();
	}

	retrySave(): Promise<void> {
		return this.queue.retry();
	}

	private onSaved(sent: Settings, stored: Settings): void {
		const current = this.settings ? $state.snapshot(this.settings) : null;
		if (current && sameJson(current, sent) && !sameJson(sent, stored)) {
			// The app corrected something (a clamp, a hotkey it could not use),
			// or holds a newer OBS password blob, which this window never writes.
			this.settings = clone(stored);
			const a = clone(sent);
			const b = clone(stored);
			a.obs.password_dpapi = b.obs.password_dpapi = null;
			if (!sameJson(a, b)) this.toast('The app adjusted a value to fit its range.', 'info');
		}
		if (stored.server.port !== this.lastPort) {
			this.lastPort = stored.server.port;
			void this.refreshUrls();
		}
		void this.refreshHotkeys();
	}

	/** Replaces the settings with ones the app returned (import, reset, apply). */
	adopt(settings: Settings): void {
		this.queue.cancel();
		this.queue.setBaseline(clone(settings));
		this.settings = clone(settings);
		if (settings.server.port !== this.lastPort) {
			this.lastPort = settings.server.port;
			void this.refreshUrls();
		}
		void this.refreshHotkeys();
	}

	/* --- Status ------------------------------------------------------------ */

	private onStatus(s: Status): void {
		const before = this.status;
		this.status = s;
		if (
			before &&
			(before.server.port !== s.server.port || before.server.running !== s.server.running)
		) {
			void this.refreshUrls();
		}
	}

	setStatus(s: Status): void {
		this.onStatus(s);
	}

	async refreshUrls(): Promise<void> {
		try {
			this.urls = await this.ipc.serverUrls();
		} catch (e) {
			this.toast(errorMessage(e), 'danger');
		}
	}

	async refreshMonitors(): Promise<void> {
		try {
			this.monitors = await this.ipc.listMonitors();
		} catch (e) {
			this.toast(errorMessage(e), 'danger');
		}
	}

	async refreshHotkeys(): Promise<void> {
		try {
			this.hotkeyStates = await this.ipc.hotkeysStatus();
		} catch {
			// Registration state is extra information; the hotkeys still save.
		}
	}

	/** The merged SolidWorks profile (data file plus imports); loaded once, reloaded on import. */
	loadProfile(force = false): Promise<void> {
		if (this.profileLoading && !force) return this.profileLoading;
		this.profileLoading = (async () => {
			try {
				this.profile = await this.ipc.profileShortcuts('solidworks');
				this.profileError = null;
			} catch (e) {
				this.profileError = errorMessage(e);
			}
		})();
		return this.profileLoading;
	}

	/* --- Actions ----------------------------------------------------------- */

	/**
	 * Runs a command and reports a failure as a toast. Returns the result, or
	 * null when it failed.
	 */
	async run<T>(fn: () => Promise<T>, done?: string): Promise<T | null> {
		try {
			const r = await fn();
			if (done) this.toast(done, 'ok');
			return r;
		} catch (e) {
			this.toast(errorMessage(e), 'danger');
			return null;
		}
	}

	/** Runs a command that returns Status and shows the new status. */
	async statusCommand(fn: () => Promise<Status>, done?: string): Promise<boolean> {
		const s = await this.run(fn, done);
		if (s) this.setStatus(s);
		return s !== null;
	}

	toast(text: string, tone: Toast['tone'] = 'info'): void {
		const id = this.nextToast++;
		this.toasts = [...this.toasts.filter((t) => t.text !== text), { id, tone, text }].slice(-3);
		setTimeout(() => this.dismiss(id), tone === 'danger' ? TOAST_MS * 2 : TOAST_MS);
	}

	dismiss(id: number): void {
		this.toasts = this.toasts.filter((t) => t.id !== id);
	}

	setAppearance(a: Appearance): void {
		this.appearance = a;
		if (typeof document !== 'undefined') applyAppearance(a);
		saveAppearance(a);
	}

	go(page: PageId): void {
		this.page = page;
		if (typeof history !== 'undefined' && typeof location !== 'undefined') {
			history.replaceState(null, '', `${location.pathname}${location.search}#${page}`);
		}
	}

	goFromHash(hash: string): void {
		const h = hash.replace(/^#/, '');
		if (isPageId(h)) this.page = h;
	}
}

const KEY = Symbol('keystage-settings');

export function provideController(ctl: SettingsController): void {
	setContext(KEY, ctl);
}

export function useController(): SettingsController {
	const ctl = getContext<SettingsController | undefined>(KEY);
	if (!ctl) throw new Error('The settings controller is missing.');
	return ctl;
}
