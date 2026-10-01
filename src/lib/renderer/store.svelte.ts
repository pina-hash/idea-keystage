/**
 * The renderer's reactive state (Svelte 5 runes) around RendererEngine.
 *
 * One store per page (or per settings preview). It owns one timer: after
 * every change it schedules a single timeout for the engine's next deadline
 * (a callout's linger or fade ending, a key's decay ending), so an idle
 * overlay runs no timers at all.
 */
import type { AppState, Legends, OverlayConfig, Route, ServerMsg } from '../protocol';
import type { CalloutItem } from './logic/callouts';
import type { KeyStates, MouseState } from './logic/inputs';
import {
	RendererEngine,
	noChanges,
	type Changes,
	type ConnectionStatus,
	type EngineInfo,
} from './engine';
import type { ConfigOverrides } from './options';
import type { Transport } from './transport';

export interface Clock {
	now(): number;
	setTimeout(fn: () => void, ms: number): unknown;
	clearTimeout(handle: unknown): void;
}

export const systemClock: Clock = {
	now: () => (typeof performance !== 'undefined' ? performance.now() : Date.now()),
	setTimeout: (fn, ms) => setTimeout(fn, ms),
	clearTimeout: (h) => clearTimeout(h as ReturnType<typeof setTimeout>),
};

export interface RendererStoreOptions {
	route: Route;
	overrides?: ConfigOverrides;
	clock?: Clock;
}

export class RendererStore {
	readonly route: Route;
	readonly engine: RendererEngine;
	private readonly clock: Clock;
	private timer: unknown = null;
	private timerAt: number | null = null;
	private transport: Transport | null = null;

	config = $state.raw<OverlayConfig>(null as unknown as OverlayConfig);
	appState = $state.raw<AppState>(null as unknown as AppState);
	legends = $state.raw<Legends | null>(null);
	callouts = $state.raw<readonly CalloutItem[]>([]);
	keys = $state.raw<KeyStates>({});
	mouse = $state.raw<MouseState>(null as unknown as MouseState);
	connection = $state<ConnectionStatus>('idle');
	info = $state.raw<EngineInfo | null>(null);

	constructor(opts: RendererStoreOptions) {
		this.route = opts.route;
		this.clock = opts.clock ?? systemClock;
		this.engine = new RendererEngine(opts.route, opts.overrides ?? {});
		this.config = this.engine.config;
		this.appState = this.engine.state;
		this.mouse = this.engine.mouse;
	}

	/** Feeds one server message. */
	handle(msg: ServerMsg): void {
		const ch = this.engine.handle(msg, this.clock.now());
		this.commit(ch);
	}

	setConnection(status: ConnectionStatus): void {
		this.commit(this.engine.setConnection(status, this.clock.now()));
	}

	/** Applies a config as if the app had sent it. */
	setBaseConfig(config: OverlayConfig): void {
		this.commit(this.engine.setBaseConfig(config, this.clock.now()));
	}

	/**
	 * Pins a config over whatever the app or demo sends (the settings preview
	 * shows unsaved settings this way); null unpins.
	 */
	pinConfig(config: OverlayConfig | null): void {
		this.commit(this.engine.pinConfig(config, this.clock.now()));
	}

	setOverrides(overrides: ConfigOverrides): void {
		this.commit(this.engine.setOverrides(overrides, this.clock.now()));
	}

	/** Connects a transport. Any previous one is stopped first. */
	connect(transport: Transport): void {
		this.transport?.stop();
		this.transport = transport;
		transport.start({
			message: (msg) => {
				if (this.transport === transport) this.handle(msg);
			},
			status: (s) => {
				if (this.transport === transport) this.setConnection(s);
			},
		});
	}

	/** Stops the transport and the timer. */
	destroy(): void {
		this.transport?.stop();
		this.transport = null;
		this.cancelTimer();
	}

	private commit(ch: Changes): void {
		const e = this.engine;
		if (ch.config) this.config = e.config;
		if (ch.state) this.appState = e.state;
		if (ch.legends) this.legends = e.legends;
		if (ch.callouts) this.callouts = e.callouts.map((it) => ({ ...it }));
		if (ch.keys) this.keys = e.keys;
		if (ch.mouse) this.mouse = e.mouse;
		if (ch.connection) {
			this.connection = e.connection;
			this.info = e.info;
		}
		this.schedule();
	}

	private onTimer = (): void => {
		this.timer = null;
		this.timerAt = null;
		const ch = this.engine.tick(this.clock.now(), noChanges());
		this.commit(ch);
	};

	private schedule(): void {
		const next = this.engine.nextDeadline();
		if (next === null) {
			this.cancelTimer();
			return;
		}
		if (this.timer !== null && this.timerAt === next) return;
		this.cancelTimer();
		const delay = Math.max(0, Math.ceil(next - this.clock.now()));
		this.timerAt = next;
		this.timer = this.clock.setTimeout(this.onTimer, delay);
	}

	private cancelTimer(): void {
		if (this.timer !== null) this.clock.clearTimeout(this.timer);
		this.timer = null;
		this.timerAt = null;
	}
}
