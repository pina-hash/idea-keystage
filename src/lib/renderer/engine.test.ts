import { describe, expect, it } from 'vitest';
import { defaultAppState, defaultOverlayConfig, type Callout, type ServerMsg } from '../protocol';
import { RendererEngine } from './engine';

function hello(changes: { theme?: string; maxStack?: number } = {}): ServerMsg {
	const config = defaultOverlayConfig();
	if (changes.theme) config.theme = changes.theme;
	if (changes.maxStack) config.callouts.max_stack = changes.maxStack;
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

const callout = (id: number): Callout => ({
	id,
	kind: 'key',
	steps: [[{ key: 'F', label: 'F', kind: 'key' }]],
	text: null,
	count: 1,
	caption: null,
	context: null,
	profile: null,
	t_ms: 0,
	updated_ms: 0,
});

describe('RendererEngine', () => {
	it('goes live on hello and takes its config, with URL overrides on top', () => {
		const e = new RendererEngine('keys', { theme: 'carbon', stack: 2 });
		expect(e.config.theme).toBe('carbon');
		const ch = e.handle(hello({ theme: 'studio', maxStack: 6 }), 0);
		expect(ch.connection && ch.config && ch.state).toBe(true);
		expect(e.connection).toBe('live');
		expect(e.config.theme).toBe('carbon');
		expect(e.config.callouts.max_stack).toBe(2);
		expect(e.info?.version).toBe('0.1.0');
	});

	it('a pinned config wins over what the app sends until it is unpinned', () => {
		const e = new RendererEngine('all');
		const pinned = defaultOverlayConfig();
		pinned.theme = 'blueprint';
		e.pinConfig(pinned, 0);
		e.handle(hello({ theme: 'studio' }), 0);
		expect(e.config.theme).toBe('blueprint');
		e.pinConfig(null, 1);
		expect(e.config.theme).toBe('studio');
	});

	it('releases keys and buttons when the connection drops, and on pause or suppression', () => {
		const e = new RendererEngine('all');
		e.handle(hello(), 0);
		e.handle({ t: 'key', phys: 'sc:1D', key: 'Ctrl', down: true, modifier: 'ctrl', t_ms: 1 }, 1);
		e.handle({ t: 'button', button: 'left', down: true, x: 0.5, y: 0.5, t_ms: 1 }, 1);
		e.setConnection('retrying', 10);
		expect(e.keys['sc:1D']?.down).toBe(false);
		expect(e.mouse.buttons.left.down).toBe(false);

		e.handle(hello(), 20);
		e.handle({ t: 'key', phys: 'sc:1D', key: 'Ctrl', down: true, modifier: 'ctrl', t_ms: 2 }, 21);
		e.handle({ t: 'state', state: { ...defaultAppState(), suppressed: 'password' } }, 30);
		expect(e.keys['sc:1D']?.down).toBe(false);
	});

	it('clear removes callouts; release_all keeps them', () => {
		const e = new RendererEngine('keys');
		e.handle({ t: 'callout', callout: callout(1) }, 0);
		e.handle({ t: 'release_all', t_ms: 1 }, 1);
		expect(e.callouts).toHaveLength(1);
		e.handle({ t: 'clear' }, 2);
		expect(e.callouts).toHaveLength(0);
	});

	it('stores legends from hello and from the legends message', () => {
		const e = new RendererEngine('keyboard');
		e.handle({ t: 'legends', layout: '0000040C', legends: { 'sc:10': 'A' } }, 0);
		expect(e.legends).toEqual({ layout: '0000040C', legends: { 'sc:10': 'A' } });
	});

	it('reports the earliest deadline across callouts, keys and the mouse', () => {
		const e = new RendererEngine('all');
		e.handle(hello(), 0);
		expect(e.nextDeadline()).toBeNull();
		e.handle({ t: 'callout', callout: callout(1) }, 0);
		expect(e.nextDeadline()).toBe(2200);
		e.handle({ t: 'key', phys: 'sc:21', key: 'F', down: true, modifier: null, t_ms: 0 }, 0);
		e.handle({ t: 'key', phys: 'sc:21', key: 'F', down: false, modifier: null, t_ms: 0 }, 100);
		expect(e.nextDeadline()).toBe(800);
		const ch = e.tick(800);
		expect(ch.keys).toBe(true);
		expect(e.keys).toEqual({});
	});
});
