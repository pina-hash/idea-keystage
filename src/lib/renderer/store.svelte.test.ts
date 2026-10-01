import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { defaultAppState, defaultOverlayConfig, type Callout, type ServerMsg } from '../protocol';
import { RendererStore, type Clock } from './store.svelte';
import type { Transport, TransportSink } from './transport';

const fakeClock: Clock = {
	now: () => Date.now(),
	setTimeout: (fn, ms) => setTimeout(fn, ms),
	clearTimeout: (h) => clearTimeout(h as ReturnType<typeof setTimeout>),
};

function hello(): ServerMsg {
	const config = defaultOverlayConfig();
	config.callouts.linger_ms = 1000;
	config.callouts.fade_ms = 300;
	config.callouts.max_stack = 3;
	config.keyboard.decay_ms = 500;
	return {
		t: 'hello',
		v: 1,
		app: 'IDEA Keystage',
		version: '0.1.0',
		config,
		state: defaultAppState(),
		legends: null,
	};
}

function callout(id: number, count = 1): ServerMsg {
	const c: Callout = {
		id,
		kind: 'combo',
		steps: [
			[
				{ key: 'Ctrl', label: 'Ctrl', kind: 'modifier' },
				{ key: 'S', label: 'S', kind: 'key' },
			],
		],
		text: null,
		count,
		caption: null,
		context: null,
		profile: null,
		t_ms: id,
		updated_ms: id * 100 + count,
	};
	return { t: 'callout', callout: c };
}

describe('RendererStore with fake timers', () => {
	beforeEach(() => {
		vi.useFakeTimers();
		vi.setSystemTime(0);
	});
	afterEach(() => {
		vi.useRealTimers();
	});

	it('fades and removes a callout on schedule, with one timer at a time', () => {
		const store = new RendererStore({ route: 'keys', clock: fakeClock });
		store.handle(hello());
		store.handle(callout(1));
		expect(store.callouts.map((c) => c.phase)).toEqual(['live']);
		expect(vi.getTimerCount()).toBe(1);
		vi.advanceTimersByTime(999);
		expect(store.callouts[0]?.phase).toBe('live');
		vi.advanceTimersByTime(1);
		expect(store.callouts[0]?.phase).toBe('fading');
		vi.advanceTimersByTime(300);
		expect(store.callouts).toHaveLength(0);
		expect(vi.getTimerCount()).toBe(0);
	});

	it('an update restarts the linger', () => {
		const store = new RendererStore({ route: 'keys', clock: fakeClock });
		store.handle(hello());
		store.handle(callout(1));
		vi.advanceTimersByTime(800);
		store.handle(callout(1, 2));
		vi.advanceTimersByTime(800);
		expect(store.callouts[0]?.phase).toBe('live');
		expect(store.callouts[0]?.callout.count).toBe(2);
		vi.advanceTimersByTime(200);
		expect(store.callouts[0]?.phase).toBe('fading');
	});

	it('keeps every callout of a burst of 200 messages at 20 per second', () => {
		const store = new RendererStore({ route: 'keys', clock: fakeClock });
		store.handle(hello());
		let shown = 0;
		for (let n = 0; n < 200; n++) {
			store.handle(callout(n + 1));
			const item = store.callouts.find((c) => c.id === n + 1);
			if (item && item.phase === 'live') shown++;
			expect(store.callouts.filter((c) => c.phase !== 'leaving').length).toBeLessThanOrEqual(3);
			vi.advanceTimersByTime(50);
		}
		expect(shown).toBe(200);
		vi.advanceTimersByTime(1000 + 300);
		expect(store.callouts).toHaveLength(0);
		expect(vi.getTimerCount()).toBe(0);
	});

	it('decays released keys and drops them', () => {
		const store = new RendererStore({ route: 'keyboard', clock: fakeClock });
		store.handle(hello());
		store.handle({ t: 'key', phys: 'sc:1F', key: 'S', down: true, modifier: null, t_ms: 0 });
		vi.advanceTimersByTime(100);
		store.handle({ t: 'key', phys: 'sc:1F', key: 'S', down: false, modifier: null, t_ms: 0 });
		expect(store.keys['sc:1F']?.releasedAt).toBe(100);
		vi.advanceTimersByTime(500);
		expect(store.keys['sc:1F']).toBeUndefined();
	});

	it('wires a transport and ignores a stopped one', () => {
		const store = new RendererStore({ route: 'keys', clock: fakeClock });
		let sinkA: TransportSink | null = null;
		let stoppedA = false;
		const a: Transport = {
			kind: 'demo',
			start: (s) => {
				sinkA = s;
			},
			stop: () => {
				stoppedA = true;
			},
		};
		store.connect(a);
		(sinkA as TransportSink | null)?.status('live');
		expect(store.connection).toBe('live');
		const b: Transport = { kind: 'demo', start: () => {}, stop: () => {} };
		store.connect(b);
		expect(stoppedA).toBe(true);
		(sinkA as TransportSink | null)?.message(callout(5));
		expect(store.callouts).toHaveLength(0);
		store.destroy();
		expect(vi.getTimerCount()).toBe(0);
	});

	it('pins a config for the settings preview', () => {
		const store = new RendererStore({ route: 'all', clock: fakeClock });
		const c = defaultOverlayConfig();
		c.theme = 'glass';
		store.pinConfig(c);
		store.handle(hello());
		expect(store.config.theme).toBe('glass');
		store.setOverrides({ theme: 'carbon' });
		expect(store.config.theme).toBe('carbon');
	});
});
