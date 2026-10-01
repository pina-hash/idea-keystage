/**
 * RendererEngine: every renderer decision that is not drawing, as plain
 * TypeScript with an injected clock, so it is tested without Svelte or a DOM.
 * store.svelte.ts wraps it in runes and a single timer.
 */
import {
	defaultAppState,
	defaultOverlayConfig,
	type AppState,
	type Legends,
	type OverlayConfig,
	type Route,
	type ServerMsg,
} from '../protocol';
import { CalloutStack, type CalloutItem } from './logic/callouts';
import {
	applyButton,
	applyKey,
	applyMove,
	applyWheel,
	emptyMouse,
	nextKeyDeadline,
	nextMouseDeadline,
	pruneKeys,
	pruneMouse,
	releaseAllButtons,
	releaseAllKeys,
	type KeyStates,
	type MouseState,
} from './logic/inputs';
import { applyOverrides, type ConfigOverrides } from './options';

/**
 * idle: not started. connecting: first attempt. open: socket open, waiting
 * for the hello. live: hello received. retrying: lost, reconnecting.
 * failed: this transport cannot work here (no Tauri, for example).
 */
export type ConnectionStatus = 'idle' | 'connecting' | 'open' | 'live' | 'retrying' | 'failed';

/** What changed in one call, so the Svelte wrapper copies only that. */
export interface Changes {
	config: boolean;
	state: boolean;
	legends: boolean;
	callouts: boolean;
	keys: boolean;
	mouse: boolean;
	connection: boolean;
}

export function noChanges(): Changes {
	return {
		config: false,
		state: false,
		legends: false,
		callouts: false,
		keys: false,
		mouse: false,
		connection: false,
	};
}

export interface EngineInfo {
	app: string;
	version: string;
	protocol: number;
}

export class RendererEngine {
	readonly route: Route;
	private overrides: ConfigOverrides;
	/** The config the app sent last. */
	private remote: OverlayConfig;
	/** A config pinned by the host page (settings preview), or null. */
	private pinned: OverlayConfig | null = null;
	config: OverlayConfig;
	state: AppState = defaultAppState();
	legends: Legends | null = null;
	keys: KeyStates = {};
	mouse: MouseState = emptyMouse();
	connection: ConnectionStatus = 'idle';
	info: EngineInfo | null = null;
	readonly stack: CalloutStack;

	constructor(route: Route, overrides: ConfigOverrides = {}, base?: OverlayConfig) {
		this.route = route;
		this.overrides = { ...overrides };
		this.remote = base ?? defaultOverlayConfig();
		this.config = applyOverrides(this.remote, this.overrides, route);
		this.stack = new CalloutStack(this.timing());
	}

	get callouts(): readonly CalloutItem[] {
		return this.stack.items;
	}

	private timing() {
		return {
			maxStack: this.config.callouts.max_stack,
			lingerMs: this.config.callouts.linger_ms,
			fadeMs: this.config.callouts.fade_ms,
		};
	}

	/** Replaces the app's config (URL overrides still apply on top). */
	setBaseConfig(config: OverlayConfig, now: number, ch: Changes = noChanges()): Changes {
		this.remote = config;
		return this.pinned ? ch : this.recompute(now, ch);
	}

	/**
	 * Pins a config that wins over what the app sends (the settings preview
	 * shows unsaved settings this way). null returns to the app's config.
	 */
	pinConfig(config: OverlayConfig | null, now: number, ch: Changes = noChanges()): Changes {
		this.pinned = config;
		return this.recompute(now, ch);
	}

	/** Replaces the URL overrides (the settings preview uses this). */
	setOverrides(overrides: ConfigOverrides, now: number, ch: Changes = noChanges()): Changes {
		this.overrides = { ...overrides };
		return this.recompute(now, ch);
	}

	private recompute(now: number, ch: Changes): Changes {
		this.config = applyOverrides(this.pinned ?? this.remote, this.overrides, this.route);
		this.stack.setTiming(this.timing(), now);
		ch.config = true;
		ch.callouts = true;
		return ch;
	}

	setConnection(status: ConnectionStatus, now: number, ch: Changes = noChanges()): Changes {
		if (status === this.connection) return ch;
		const wasLive = this.connection === 'live';
		this.connection = status;
		ch.connection = true;
		// Key-ups may have been missed while disconnected: nothing stays lit.
		if (wasLive && status !== 'live') this.releaseAll(now, ch);
		return ch;
	}

	private releaseAll(now: number, ch: Changes): void {
		const keys = releaseAllKeys(this.keys, now);
		if (keys !== this.keys) {
			this.keys = keys;
			ch.keys = true;
		}
		const mouse = releaseAllButtons(this.mouse, now);
		if (mouse !== this.mouse) {
			this.mouse = mouse;
			ch.mouse = true;
		}
	}

	handle(msg: ServerMsg, now: number, ch: Changes = noChanges()): Changes {
		switch (msg.t) {
			case 'hello':
				this.info = { app: msg.app, version: msg.version, protocol: msg.v };
				this.state = msg.state;
				ch.state = true;
				this.legends = msg.legends;
				ch.legends = true;
				this.releaseAll(now, ch);
				this.setBaseConfig(msg.config, now, ch);
				this.setConnection('live', now, ch);
				break;
			case 'config':
				this.setBaseConfig(msg.config, now, ch);
				break;
			case 'state': {
				const wasQuiet = this.state.paused || this.state.suppressed !== null;
				this.state = msg.state;
				ch.state = true;
				const quiet = msg.state.paused || msg.state.suppressed !== null;
				if (quiet && !wasQuiet) this.releaseAll(now, ch);
				break;
			}
			case 'legends':
				this.legends = { layout: msg.layout, legends: msg.legends };
				ch.legends = true;
				break;
			case 'callout':
				if (this.stack.upsert(msg.callout, now)) ch.callouts = true;
				break;
			case 'key': {
				const keys = applyKey(this.keys, msg, now);
				if (keys !== this.keys) {
					this.keys = keys;
					ch.keys = true;
				}
				break;
			}
			case 'button':
				this.mouse = applyButton(this.mouse, msg, now);
				ch.mouse = true;
				break;
			case 'wheel': {
				const mouse = applyWheel(this.mouse, msg, now);
				if (mouse !== this.mouse) {
					this.mouse = mouse;
					ch.mouse = true;
				}
				break;
			}
			case 'move': {
				const mouse = applyMove(this.mouse, msg, now);
				if (mouse !== this.mouse) {
					this.mouse = mouse;
					ch.mouse = true;
				}
				break;
			}
			case 'release_all':
				this.releaseAll(now, ch);
				break;
			case 'clear':
				if (this.stack.clear()) ch.callouts = true;
				this.releaseAll(now, ch);
				break;
			case 'pong':
				break;
		}
		return ch;
	}

	/** Advances timed state to `now`. */
	tick(now: number, ch: Changes = noChanges()): Changes {
		if (this.stack.tick(now)) ch.callouts = true;
		const keys = pruneKeys(this.keys, now, this.config.keyboard.decay_ms);
		if (keys !== this.keys) {
			this.keys = keys;
			ch.keys = true;
		}
		const mouse = pruneMouse(this.mouse, now);
		if (mouse !== this.mouse) {
			this.mouse = mouse;
			ch.mouse = true;
		}
		return ch;
	}

	/** When `tick` next has work, or null when nothing is timed. */
	nextDeadline(): number | null {
		const candidates = [
			this.stack.nextDeadline(),
			nextKeyDeadline(this.keys, this.config.keyboard.decay_ms),
			nextMouseDeadline(this.mouse),
		];
		let next: number | null = null;
		for (const c of candidates) if (c !== null && (next === null || c < next)) next = c;
		return next;
	}
}
