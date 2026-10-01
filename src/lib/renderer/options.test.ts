import { describe, expect, it } from 'vitest';
import { defaultOverlayConfig } from '../protocol';
import {
	applyOverrides,
	isInObs,
	parseOverrides,
	parsePageOptions,
	routeFromPath,
} from './options';

describe('page options', () => {
	it('reads the route from /overlay/<route> or ?route=', () => {
		expect(routeFromPath('/overlay/keys')).toBe('keys');
		expect(routeFromPath('/overlay/keyboard/')).toBe('keyboard');
		expect(routeFromPath('/overlay/nope')).toBeNull();
		expect(
			parsePageOptions({ pathname: '/overlay.html', search: '?route=mouse' }, false).route
		).toBe('mouse');
		expect(parsePageOptions({ pathname: '/overlay.html', search: '' }, false).route).toBe('all');
		expect(
			parsePageOptions({ pathname: '/overlay/cursor', search: '?route=keys' }, false).route
		).toBe('cursor');
	});

	it('picks the transport: native or preview -> Tauri, demo=1 -> demo, else WebSocket', () => {
		const p = (search: string, obs = false) =>
			parsePageOptions({ pathname: '/overlay/keys', search }, obs);
		expect(p('?host=native&route=all')).toMatchObject({ transport: 'tauri', host: 'native' });
		expect(p('?host=preview')).toMatchObject({ transport: 'tauri', host: 'preview' });
		expect(p('?demo=1')).toMatchObject({ transport: 'demo', demo: 'tour', host: 'browser' });
		expect(p('?demo=general')).toMatchObject({ transport: 'demo', demo: 'general' });
		expect(p('?demo=0')).toMatchObject({ transport: 'ws', demo: null });
		expect(p('', true)).toMatchObject({ transport: 'ws', host: 'obs', status: false });
		expect(p('?status=1', true).status).toBe(true);
		expect(p('?host=preview&demo=solidworks')).toMatchObject({
			transport: 'demo',
			host: 'preview',
		});
	});

	it('parses and validates overrides', () => {
		const q = new URLSearchParams(
			'theme=carbon&scale=9&anchor=top-right&x=12&y=-8&kb=tkl&stack=20&size=xl'
		);
		expect(parseOverrides(q, 'keys')).toEqual({
			theme: 'carbon',
			scale: 4,
			anchor: 'top-right',
			x: 12,
			y: -8,
			kb: 'tkl',
			stack: 12,
			size: 'xl',
		});
		expect(parseOverrides(new URLSearchParams('size=xl'), 'keyboard').size).toBeUndefined();
		expect(parseOverrides(new URLSearchParams('size=s'), 'keyboard').size).toBe('s');
		expect(parseOverrides(new URLSearchParams('theme=<b>&anchor=up&scale=abc'), 'keys')).toEqual(
			{}
		);
	});

	it('applies anchor and offsets to the piece the route is about', () => {
		const base = defaultOverlayConfig();
		const o = { anchor: 'top-left' as const, x: 10, y: 20 };
		const keys = applyOverrides(base, o, 'keys');
		expect(keys.callouts).toMatchObject({
			anchor: 'top-left',
			offset_x: 10,
			offset_y: 20,
			direction: 'down',
		});
		const kb = applyOverrides(base, o, 'keyboard');
		expect(kb.keyboard).toMatchObject({ anchor: 'top-left', offset_x: 10, offset_y: 20 });
		expect(kb.callouts).toEqual(base.callouts);
		const mouse = applyOverrides(base, o, 'mouse');
		expect(mouse.mouse).toMatchObject({ anchor: 'top-left', offset_x: 10 });
		const cursor = applyOverrides(base, o, 'cursor');
		expect(cursor).toEqual(base);
		expect(base.callouts.anchor).toBe('bottom');
	});

	it('detects OBS by window.obsstudio', () => {
		expect(isInObs({ obsstudio: {} })).toBe(true);
		expect(isInObs({})).toBe(false);
		expect(isInObs(null)).toBe(false);
	});
});
