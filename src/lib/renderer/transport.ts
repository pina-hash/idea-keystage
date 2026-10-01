/**
 * Transports feed server messages to a RendererStore (ARCHITECTURE.md
 * section 1): WebSocket for OBS and any browser, Tauri events for Host A and
 * the settings preview, and the built-in demo.
 *
 * Privacy: transports never log message content. Errors are reported only as
 * connection status.
 */
import {
	decodeServerMsg,
	encodeClientMsg,
	type ClientHost,
	type Route,
	type ServerMsg,
} from '../protocol';
import type { ConnectionStatus } from './engine';
import type { DemoScriptId, PageOptions } from './options';

export interface TransportSink {
	message(msg: ServerMsg): void;
	status(status: ConnectionStatus): void;
}

export interface Transport {
	readonly kind: 'ws' | 'tauri' | 'demo';
	start(sink: TransportSink): void;
	stop(): void;
}

export interface Timers {
	now(): number;
	setTimeout(fn: () => void, ms: number): unknown;
	clearTimeout(handle: unknown): void;
	setInterval(fn: () => void, ms: number): unknown;
	clearInterval(handle: unknown): void;
}

export const systemTimers: Timers = {
	now: () => (typeof performance !== 'undefined' ? performance.now() : Date.now()),
	setTimeout: (fn, ms) => setTimeout(fn, ms),
	clearTimeout: (h) => clearTimeout(h as ReturnType<typeof setTimeout>),
	setInterval: (fn, ms) => setInterval(fn, ms),
	clearInterval: (h) => clearInterval(h as ReturnType<typeof setInterval>),
};

/* ------------------------------------------------------------------------ */
/* Backoff                                                                   */
/* ------------------------------------------------------------------------ */

export interface BackoffOptions {
	initialMs: number;
	maxMs: number;
	factor: number;
	/** 0.2 = +/-20 percent. */
	jitter: number;
}

export const DEFAULT_BACKOFF: BackoffOptions = {
	initialMs: 250,
	maxMs: 4000,
	factor: 2,
	jitter: 0.2,
};

/** Delay before reconnect attempt `attempt` (0 = first retry). */
export function backoffDelay(
	attempt: number,
	opts: BackoffOptions = DEFAULT_BACKOFF,
	random: () => number = Math.random
): number {
	const base = Math.min(opts.maxMs, opts.initialMs * Math.pow(opts.factor, Math.max(0, attempt)));
	const spread = base * opts.jitter;
	return Math.max(0, Math.round(base - spread + random() * 2 * spread));
}

/* ------------------------------------------------------------------------ */
/* WebSocket                                                                 */
/* ------------------------------------------------------------------------ */

/** The subset of the WebSocket API the transport uses. */
export interface SocketLike {
	readonly readyState: number;
	onopen: ((ev: unknown) => void) | null;
	onmessage: ((ev: { data: unknown }) => void) | null;
	onclose: ((ev: unknown) => void) | null;
	onerror: ((ev: unknown) => void) | null;
	send(data: string): void;
	close(): void;
}
export type SocketFactory = (url: string) => SocketLike;

const WS_OPEN = 1;

export interface WsTransportOptions {
	url: string;
	route: Route;
	host: ClientHost;
	createSocket?: SocketFactory;
	timers?: Timers;
	backoff?: BackoffOptions;
	random?: () => number;
	/** How often to ping while connected. */
	pingMs?: number;
	/** Close and reconnect when nothing arrived for this long. */
	staleMs?: number;
}

export function defaultSocketUrl(loc: { host: string }): string {
	return `ws://${loc.host}/ws`;
}

export class WsTransport implements Transport {
	readonly kind = 'ws' as const;
	private readonly opts: Required<Omit<WsTransportOptions, 'createSocket'>> & {
		createSocket: SocketFactory;
	};
	private sink: TransportSink | null = null;
	private socket: SocketLike | null = null;
	private stopped = true;
	private attempt = 0;
	private retryTimer: unknown = null;
	private pingTimer: unknown = null;
	private lastSeen = 0;

	constructor(opts: WsTransportOptions) {
		this.opts = {
			url: opts.url,
			route: opts.route,
			host: opts.host,
			createSocket:
				opts.createSocket ?? ((url: string) => new WebSocket(url) as unknown as SocketLike),
			timers: opts.timers ?? systemTimers,
			backoff: opts.backoff ?? DEFAULT_BACKOFF,
			random: opts.random ?? Math.random,
			pingMs: opts.pingMs ?? 10_000,
			staleMs: opts.staleMs ?? 25_000,
		};
	}

	start(sink: TransportSink): void {
		this.sink = sink;
		this.stopped = false;
		this.attempt = 0;
		sink.status('connecting');
		this.open();
	}

	stop(): void {
		this.stopped = true;
		this.clearTimers();
		const s = this.socket;
		this.socket = null;
		if (s) {
			s.onopen = s.onmessage = s.onclose = s.onerror = null;
			try {
				s.close();
			} catch {
				/* already closed */
			}
		}
	}

	private open(): void {
		if (this.stopped) return;
		let socket: SocketLike;
		try {
			socket = this.opts.createSocket(this.opts.url);
		} catch {
			this.scheduleRetry();
			return;
		}
		this.socket = socket;
		socket.onopen = () => {
			if (this.socket !== socket) return;
			this.lastSeen = this.opts.timers.now();
			this.sink?.status('open');
			this.send({ t: 'hello', route: this.opts.route, host: this.opts.host });
			this.startPing();
		};
		socket.onmessage = (ev) => {
			if (this.socket !== socket) return;
			this.lastSeen = this.opts.timers.now();
			const msg = decodeServerMsg(ev.data);
			if (!msg || msg.t === 'pong') return;
			if (msg.t === 'hello') this.attempt = 0;
			this.sink?.message(msg);
		};
		const lost = () => {
			if (this.socket !== socket) return;
			socket.onopen = socket.onmessage = socket.onclose = socket.onerror = null;
			this.socket = null;
			this.stopPing();
			try {
				socket.close();
			} catch {
				/* already closed */
			}
			this.scheduleRetry();
		};
		socket.onclose = lost;
		socket.onerror = lost;
	}

	private send(msg: Parameters<typeof encodeClientMsg>[0]): void {
		const s = this.socket;
		if (!s || s.readyState !== WS_OPEN) return;
		try {
			s.send(encodeClientMsg(msg));
		} catch {
			/* the close handler reconnects */
		}
	}

	private startPing(): void {
		this.stopPing();
		this.pingTimer = this.opts.timers.setInterval(() => {
			const s = this.socket;
			if (!s) return;
			if (this.opts.timers.now() - this.lastSeen > this.opts.staleMs) {
				s.onerror?.(new Error('stale'));
				return;
			}
			this.send({ t: 'ping' });
		}, this.opts.pingMs);
	}

	private stopPing(): void {
		if (this.pingTimer !== null) this.opts.timers.clearInterval(this.pingTimer);
		this.pingTimer = null;
	}

	private scheduleRetry(): void {
		if (this.stopped) return;
		this.sink?.status('retrying');
		const delay = backoffDelay(this.attempt, this.opts.backoff, this.opts.random);
		this.attempt += 1;
		if (this.retryTimer !== null) this.opts.timers.clearTimeout(this.retryTimer);
		this.retryTimer = this.opts.timers.setTimeout(() => {
			this.retryTimer = null;
			this.open();
		}, delay);
	}

	private clearTimers(): void {
		this.stopPing();
		if (this.retryTimer !== null) this.opts.timers.clearTimeout(this.retryTimer);
		this.retryTimer = null;
	}
}

/* ------------------------------------------------------------------------ */
/* Tauri                                                                     */
/* ------------------------------------------------------------------------ */

export const OVERLAY_EVENT = 'keystage://overlay';
export const HELLO_COMMAND = 'overlay_hello';

export interface TauriApi {
	listen(event: string, handler: (ev: { payload: unknown }) => void): Promise<() => void>;
	invoke(cmd: string, args?: Record<string, unknown>): Promise<unknown>;
}

/**
 * The real Tauri API for this page. Events are heard on THIS window only
 * (target `WebviewWindow { label }`), never with the `@tauri-apps/api/event`
 * default target `Any`: Tauri hands an emit to every `Any` listener whatever
 * the emitter's filter says, so a global listener would defeat the app's
 * per-window filter (`wants_overlay_event`: Host A only while it is open, the
 * settings preview only while it can be seen). A plain `emit` (no filter, such
 * as `keystage://status`) still reaches a window-scoped listener.
 */
export async function loadTauriApi(): Promise<TauriApi> {
	const [win, core] = await Promise.all([
		import('@tauri-apps/api/webviewWindow'),
		import('@tauri-apps/api/core'),
	]);
	const self = win.getCurrentWebviewWindow();
	return {
		listen: (event, handler) => self.listen(event, (e) => handler({ payload: e.payload })),
		invoke: (cmd, args) => core.invoke(cmd, args),
	};
}

export function hasTauri(w: unknown): boolean {
	return typeof w === 'object' && w !== null && '__TAURI_INTERNALS__' in w;
}

export interface TauriTransportOptions {
	loadApi?: () => Promise<TauriApi>;
	timers?: Timers;
	backoff?: BackoffOptions;
	random?: () => number;
	/** Checked before loading the API; false reports `failed`. */
	available?: () => boolean;
}

/**
 * Listens to "keystage://overlay" and asks the app for a hello snapshot with
 * `invoke('overlay_hello')`. The command may return the hello message, emit it
 * as an event, or both. Until a hello arrives the request is retried with
 * backoff (the app may still be starting).
 */
export class TauriTransport implements Transport {
	readonly kind = 'tauri' as const;
	private readonly loadApi: () => Promise<TauriApi>;
	private readonly timers: Timers;
	private readonly backoff: BackoffOptions;
	private readonly random: () => number;
	private readonly available: () => boolean;
	private sink: TransportSink | null = null;
	private unlisten: (() => void) | null = null;
	private stopped = true;
	private live = false;
	private attempt = 0;
	private retryTimer: unknown = null;
	private api: TauriApi | null = null;

	constructor(opts: TauriTransportOptions = {}) {
		this.loadApi = opts.loadApi ?? loadTauriApi;
		this.timers = opts.timers ?? systemTimers;
		this.backoff = opts.backoff ?? DEFAULT_BACKOFF;
		this.random = opts.random ?? Math.random;
		this.available = opts.available ?? (() => hasTauri(globalThis));
	}

	start(sink: TransportSink): void {
		this.sink = sink;
		this.stopped = false;
		this.live = false;
		this.attempt = 0;
		sink.status('connecting');
		if (!this.available()) {
			sink.status('failed');
			return;
		}
		void this.boot();
	}

	stop(): void {
		this.stopped = true;
		if (this.retryTimer !== null) this.timers.clearTimeout(this.retryTimer);
		this.retryTimer = null;
		const un = this.unlisten;
		this.unlisten = null;
		if (un) {
			try {
				un();
			} catch {
				/* the window is closing */
			}
		}
	}

	private deliver(raw: unknown): void {
		if (this.stopped) return;
		const msg = decodeServerMsg(raw);
		if (!msg || msg.t === 'pong') return;
		if (msg.t === 'hello') {
			this.live = true;
			this.attempt = 0;
			if (this.retryTimer !== null) this.timers.clearTimeout(this.retryTimer);
			this.retryTimer = null;
		}
		this.sink?.message(msg);
	}

	private async boot(): Promise<void> {
		try {
			this.api = await this.loadApi();
			const un = await this.api.listen(OVERLAY_EVENT, (ev) => this.deliver(ev.payload));
			if (this.stopped) {
				un();
				return;
			}
			this.unlisten = un;
		} catch {
			if (!this.stopped) this.sink?.status('failed');
			return;
		}
		await this.requestHello();
	}

	private async requestHello(): Promise<void> {
		if (this.stopped || this.live || !this.api) return;
		try {
			const reply = await this.api.invoke(HELLO_COMMAND);
			if (reply !== null && reply !== undefined) this.deliver(reply);
		} catch {
			/* not ready yet: retried below */
		}
		if (this.stopped || this.live) return;
		this.sink?.status('retrying');
		const delay = backoffDelay(this.attempt, this.backoff, this.random);
		this.attempt += 1;
		this.retryTimer = this.timers.setTimeout(() => {
			this.retryTimer = null;
			void this.requestHello();
		}, delay);
	}
}

/* ------------------------------------------------------------------------ */
/* Demo                                                                      */
/* ------------------------------------------------------------------------ */

/** One scheduled message of a demo script. `at` is ms from the loop start. */
export interface DemoStep {
	at: number;
	msg: ServerMsg;
}

export interface DemoScript {
	/** The hello sent once before the first loop (config and state). */
	hello: ServerMsg;
	steps: readonly DemoStep[];
	/** Loop length in ms (at least the last step's `at`). */
	durationMs: number;
}

export interface DemoTransportOptions {
	script: DemoScriptId;
	route: Route;
	/** Builds the script; the default loads src/lib/demo lazily. */
	load?: (id: DemoScriptId, route: Route) => Promise<DemoScript>;
	timers?: Timers;
	loop?: boolean;
}

/** Callout ids in loop n are offset so they never collide with loop n-1. */
export const DEMO_LOOP_ID_STRIDE = 100_000;

export class DemoTransport implements Transport {
	readonly kind = 'demo' as const;
	private readonly opts: DemoTransportOptions;
	private readonly timers: Timers;
	private sink: TransportSink | null = null;
	private stopped = true;
	private timer: unknown = null;
	private script: DemoScript | null = null;
	private loopIndex = 0;
	private stepIndex = 0;
	private loopStart = 0;

	constructor(opts: DemoTransportOptions) {
		this.opts = opts;
		this.timers = opts.timers ?? systemTimers;
	}

	start(sink: TransportSink): void {
		this.sink = sink;
		this.stopped = false;
		sink.status('connecting');
		const load =
			this.opts.load ??
			(async (id: DemoScriptId, route: Route) => {
				const mod = await import('../demo/index');
				return mod.loadDemoScript(id, route);
			});
		load(this.opts.script, this.opts.route).then(
			(script) => {
				if (this.stopped) return;
				this.script = script;
				this.beginLoop(0);
			},
			() => {
				if (!this.stopped) sink.status('failed');
			}
		);
	}

	stop(): void {
		this.stopped = true;
		if (this.timer !== null) this.timers.clearTimeout(this.timer);
		this.timer = null;
	}

	private beginLoop(index: number): void {
		const script = this.script;
		if (!script || this.stopped) return;
		this.loopIndex = index;
		this.stepIndex = 0;
		this.loopStart = this.timers.now();
		if (index === 0) {
			this.sink?.message(script.hello);
			this.sink?.status('live');
		}
		this.pump();
	}

	private pump(): void {
		const script = this.script;
		if (!script || this.stopped) return;
		const elapsed = this.timers.now() - this.loopStart;
		while (this.stepIndex < script.steps.length) {
			const step = script.steps[this.stepIndex];
			if (!step || step.at > elapsed) break;
			this.stepIndex += 1;
			this.sink?.message(this.remap(step.msg));
			if (this.stopped) return;
		}
		const next = script.steps[this.stepIndex];
		if (next) {
			this.timer = this.timers.setTimeout(
				() => {
					this.timer = null;
					this.pump();
				},
				Math.max(0, next.at - elapsed)
			);
			return;
		}
		if (this.opts.loop === false) return;
		this.timer = this.timers.setTimeout(
			() => {
				this.timer = null;
				this.beginLoop(this.loopIndex + 1);
			},
			Math.max(0, script.durationMs - elapsed)
		);
	}

	private remap(msg: ServerMsg): ServerMsg {
		const shift = this.loopIndex * DEMO_LOOP_ID_STRIDE;
		const offsetT = Math.round(this.loopIndex * (this.script?.durationMs ?? 0));
		switch (msg.t) {
			case 'callout':
				return {
					t: 'callout',
					callout: {
						...msg.callout,
						id: msg.callout.id + shift,
						t_ms: msg.callout.t_ms + offsetT,
						updated_ms: msg.callout.updated_ms + offsetT,
					},
				};
			case 'key':
			case 'button':
			case 'wheel':
			case 'move':
			case 'release_all':
				return { ...msg, t_ms: msg.t_ms + offsetT };
			default:
				return msg;
		}
	}
}

/* ------------------------------------------------------------------------ */
/* Selection                                                                 */
/* ------------------------------------------------------------------------ */

export interface TransportEnv {
	location: { host: string };
}

/** Picks the transport for a page from its URL options. */
export function createTransport(options: PageOptions, env: TransportEnv): Transport {
	switch (options.transport) {
		case 'tauri':
			return new TauriTransport();
		case 'demo':
			return new DemoTransport({ script: options.demo ?? 'tour', route: options.route });
		case 'ws':
			return new WsTransport({
				url: defaultSocketUrl(env.location),
				route: options.route,
				host: options.host,
			});
	}
}
