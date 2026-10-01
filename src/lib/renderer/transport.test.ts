import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { defaultAppState, defaultOverlayConfig, type ServerMsg } from '../protocol';
import type { ConnectionStatus } from './engine';
import {
	DEMO_LOOP_ID_STRIDE,
	DemoTransport,
	TauriTransport,
	WsTransport,
	backoffDelay,
	createTransport,
	type DemoScript,
	type SocketLike,
	type TauriApi,
	type Timers,
	type TransportSink,
} from './transport';
import { parsePageOptions } from './options';

const timers: Timers = {
	now: () => Date.now(),
	setTimeout: (fn, ms) => setTimeout(fn, ms),
	clearTimeout: (h) => clearTimeout(h as ReturnType<typeof setTimeout>),
	setInterval: (fn, ms) => setInterval(fn, ms),
	clearInterval: (h) => clearInterval(h as ReturnType<typeof setInterval>),
};

const HELLO: ServerMsg = {
	t: 'hello',
	v: 1,
	app: 'IDEA Keystage',
	version: '0.1.0',
	config: defaultOverlayConfig(),
	state: defaultAppState(),
	legends: null,
};

function recorder() {
	const messages: ServerMsg[] = [];
	const statuses: ConnectionStatus[] = [];
	const sink: TransportSink = {
		message: (m) => messages.push(m),
		status: (s) => statuses.push(s),
	};
	return { sink, messages, statuses };
}

class FakeSocket implements SocketLike {
	readyState = 0;
	sent: string[] = [];
	closed = false;
	onopen: ((ev: unknown) => void) | null = null;
	onmessage: ((ev: { data: unknown }) => void) | null = null;
	onclose: ((ev: unknown) => void) | null = null;
	onerror: ((ev: unknown) => void) | null = null;
	constructor(readonly url: string) {}
	send(data: string) {
		this.sent.push(data);
	}
	close() {
		this.closed = true;
		this.readyState = 3;
	}
	open() {
		this.readyState = 1;
		this.onopen?.({});
	}
	receive(msg: unknown) {
		this.onmessage?.({ data: typeof msg === 'string' ? msg : JSON.stringify(msg) });
	}
	drop() {
		this.readyState = 3;
		this.onclose?.({});
	}
}

describe('backoff', () => {
	it('doubles from 250 ms up to 4 s, with jitter', () => {
		const mid = () => 0.5;
		expect([0, 1, 2, 3, 4, 5, 9].map((a) => backoffDelay(a, undefined, mid))).toEqual([
			250, 500, 1000, 2000, 4000, 4000, 4000,
		]);
		expect(backoffDelay(0, undefined, () => 0)).toBe(200);
		expect(backoffDelay(0, undefined, () => 1)).toBe(300);
	});
});

describe('WsTransport', () => {
	beforeEach(() => {
		vi.useFakeTimers();
		vi.setSystemTime(0);
	});
	afterEach(() => vi.useRealTimers());

	function setup() {
		const sockets: FakeSocket[] = [];
		const t = new WsTransport({
			url: 'ws://127.0.0.1:47820/ws',
			route: 'keys',
			host: 'obs',
			createSocket: (url) => {
				const s = new FakeSocket(url);
				sockets.push(s);
				return s;
			},
			timers,
			random: () => 0.5,
			pingMs: 1000,
			staleMs: 2500,
		});
		const r = recorder();
		t.start(r.sink);
		return { t, sockets, ...r };
	}

	it('sends the client hello on open and goes live on the server hello', () => {
		const { sockets, statuses, messages } = setup();
		expect(sockets[0]?.url).toBe('ws://127.0.0.1:47820/ws');
		sockets[0]?.open();
		expect(sockets[0]?.sent.map((s) => JSON.parse(s))).toEqual([
			{ t: 'hello', route: 'keys', host: 'obs' },
		]);
		sockets[0]?.receive(HELLO);
		sockets[0]?.receive({ t: 'pong' });
		sockets[0]?.receive('garbage');
		expect(messages.map((m) => m.t)).toEqual(['hello']);
		expect(statuses).toEqual(['connecting', 'open']);
	});

	it('pings while connected and reconnects when nothing arrives', () => {
		const { sockets } = setup();
		sockets[0]?.open();
		vi.advanceTimersByTime(1000);
		expect(sockets[0]?.sent.at(-1)).toBe('{"t":"ping"}');
		vi.advanceTimersByTime(2000); // 3000 ms since the last message: stale
		expect(sockets[0]?.closed).toBe(true);
		vi.advanceTimersByTime(250);
		expect(sockets).toHaveLength(2);
	});

	it('reconnects with exponential backoff and resets after a hello', () => {
		const { sockets, statuses } = setup();
		sockets[0]?.drop();
		expect(statuses.at(-1)).toBe('retrying');
		vi.advanceTimersByTime(249);
		expect(sockets).toHaveLength(1);
		vi.advanceTimersByTime(1);
		expect(sockets).toHaveLength(2);
		sockets[1]?.drop();
		vi.advanceTimersByTime(499);
		expect(sockets).toHaveLength(2);
		vi.advanceTimersByTime(1);
		expect(sockets).toHaveLength(3);
		sockets[2]?.open();
		sockets[2]?.receive(HELLO);
		sockets[2]?.drop();
		vi.advanceTimersByTime(250);
		expect(sockets).toHaveLength(4);
	});

	it('stops cleanly: no reconnect, no timers', () => {
		const { t, sockets } = setup();
		sockets[0]?.open();
		t.stop();
		expect(sockets[0]?.closed).toBe(true);
		vi.advanceTimersByTime(60_000);
		expect(sockets).toHaveLength(1);
		expect(vi.getTimerCount()).toBe(0);
	});

	it('retries when the socket cannot even be created', () => {
		let n = 0;
		const t = new WsTransport({
			url: 'ws://x/ws',
			route: 'all',
			host: 'browser',
			createSocket: () => {
				n++;
				throw new Error('blocked');
			},
			timers,
			random: () => 0.5,
		});
		const r = recorder();
		t.start(r.sink);
		vi.advanceTimersByTime(250);
		expect(n).toBe(2);
		expect(r.statuses).toContain('retrying');
		t.stop();
	});
});

describe('TauriTransport', () => {
	beforeEach(() => {
		vi.useFakeTimers();
		vi.setSystemTime(0);
	});
	afterEach(() => vi.useRealTimers());

	function api(reply: () => Promise<unknown>) {
		let handler: ((ev: { payload: unknown }) => void) | null = null;
		let unlistened = false;
		const calls: string[] = [];
		const fake: TauriApi = {
			listen: async (event, h) => {
				calls.push(`listen ${event}`);
				handler = h;
				return () => {
					unlistened = true;
				};
			},
			invoke: async (cmd) => {
				calls.push(`invoke ${cmd}`);
				return reply();
			},
		};
		return {
			fake,
			calls,
			emit: (p: unknown) => handler?.({ payload: p }),
			get unlistened() {
				return unlistened;
			},
		};
	}

	it('listens to keystage://overlay and takes the hello invoke_hello returns', async () => {
		const a = api(async () => HELLO);
		const t = new TauriTransport({ loadApi: async () => a.fake, timers, available: () => true });
		const r = recorder();
		t.start(r.sink);
		await vi.runAllTimersAsync();
		expect(a.calls).toEqual(['listen keystage://overlay', 'invoke overlay_hello']);
		expect(r.messages.map((m) => m.t)).toEqual(['hello']);
		a.emit({ t: 'clear' });
		a.emit({ t: 'nonsense' });
		expect(r.messages.map((m) => m.t)).toEqual(['hello', 'clear']);
		t.stop();
		expect(a.unlistened).toBe(true);
		a.emit({ t: 'clear' });
		expect(r.messages).toHaveLength(2);
	});

	it('retries the hello request until the app answers', async () => {
		let n = 0;
		const a = api(async () => {
			n++;
			if (n < 3) throw new Error('not ready');
			return null;
		});
		const t = new TauriTransport({
			loadApi: async () => a.fake,
			timers,
			available: () => true,
			random: () => 0.5,
		});
		const r = recorder();
		t.start(r.sink);
		await vi.advanceTimersByTimeAsync(0);
		expect(n).toBe(1);
		await vi.advanceTimersByTimeAsync(250);
		expect(n).toBe(2);
		await vi.advanceTimersByTimeAsync(500);
		expect(n).toBe(3);
		// The app emits the hello as an event instead of returning it.
		a.emit(HELLO);
		await vi.advanceTimersByTimeAsync(10_000);
		expect(n).toBe(3);
		expect(r.statuses).toContain('retrying');
		t.stop();
	});

	it('reports failed outside Tauri', () => {
		const t = new TauriTransport({ timers, available: () => false });
		const r = recorder();
		t.start(r.sink);
		expect(r.statuses).toEqual(['connecting', 'failed']);
	});
});

describe('DemoTransport', () => {
	beforeEach(() => {
		vi.useFakeTimers();
		vi.setSystemTime(0);
	});
	afterEach(() => vi.useRealTimers());

	const script: DemoScript = {
		hello: HELLO,
		steps: [
			{
				at: 100,
				msg: {
					t: 'callout',
					callout: {
						id: 1,
						kind: 'key',
						steps: [[{ key: 'F', label: 'F', kind: 'key' }]],
						text: null,
						count: 1,
						caption: null,
						context: null,
						profile: null,
						t_ms: 100,
						updated_ms: 100,
					},
				},
			},
			{
				at: 300,
				msg: { t: 'key', phys: 'sc:21', key: 'F', down: true, modifier: null, t_ms: 300 },
			},
		],
		durationMs: 1000,
	};

	it('plays steps on time, sends the hello once, and loops with fresh ids', async () => {
		const t = new DemoTransport({
			script: 'general',
			route: 'keys',
			load: async () => script,
			timers,
		});
		const r = recorder();
		t.start(r.sink);
		await vi.advanceTimersByTimeAsync(0);
		expect(r.messages.map((m) => m.t)).toEqual(['hello']);
		await vi.advanceTimersByTimeAsync(100);
		expect(r.messages.map((m) => m.t)).toEqual(['hello', 'callout']);
		await vi.advanceTimersByTimeAsync(200);
		expect(r.messages).toHaveLength(3);
		await vi.advanceTimersByTimeAsync(800);
		const second = r.messages.at(-1);
		expect(second?.t).toBe('callout');
		if (second?.t === 'callout') {
			expect(second.callout.id).toBe(1 + DEMO_LOOP_ID_STRIDE);
			expect(second.callout.t_ms).toBe(1100);
		}
		expect(r.messages.filter((m) => m.t === 'hello')).toHaveLength(1);
		t.stop();
		await vi.advanceTimersByTimeAsync(5000);
		expect(r.messages).toHaveLength(4);
	});

	it('reports failed when the script cannot load', async () => {
		const t = new DemoTransport({
			script: 'tour',
			route: 'keys',
			load: async () => {
				throw new Error('missing');
			},
			timers,
		});
		const r = recorder();
		t.start(r.sink);
		await vi.advanceTimersByTimeAsync(0);
		expect(r.statuses).toEqual(['connecting', 'failed']);
	});
});

describe('createTransport', () => {
	it('selects the transport from the URL', () => {
		const env = { location: { host: '127.0.0.1:47820' } };
		const opts = (search: string) => parsePageOptions({ pathname: '/overlay/keys', search }, false);
		expect(createTransport(opts('?host=native'), env).kind).toBe('tauri');
		expect(createTransport(opts('?demo=1'), env).kind).toBe('demo');
		expect(createTransport(opts(''), env).kind).toBe('ws');
	});
});
