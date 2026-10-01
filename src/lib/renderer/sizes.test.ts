import { describe, expect, it } from 'vitest';
import { defaultOverlayConfig } from '../protocol';
import {
	CALLOUT_GEOMETRY,
	CALLOUT_UNIT_PX,
	KEYBOARD_FOOTPRINTS,
	KEYBOARD_UNIT_PX,
	calloutHeightU,
} from './metrics';
import { evenCeil, obsSourceSize } from './sizes';

const monitor = { width: 2560, height: 1440 };

describe('obsSourceSize', () => {
	it('cursor and all are the monitor size', () => {
		const c = defaultOverlayConfig();
		expect(obsSourceSize('cursor', c, monitor)).toEqual(monitor);
		expect(obsSourceSize('all', c, { width: 1920.4, height: 1080 })).toEqual({
			width: 1920,
			height: 1080,
		});
	});

	it('keys is the callout box at the chosen scale, rounded up to even numbers', () => {
		const c = defaultOverlayConfig();
		const u = CALLOUT_UNIT_PX.m;
		const g = CALLOUT_GEOMETRY;
		const w = (g.maxWidthU + 2 * g.bleedU) * u;
		const rows = 4 * calloutHeightU(true) + 3 * g.stackGapU;
		const h = (rows + g.stackGapU + g.indicatorU + 2 * g.bleedU) * u + 96;
		expect(obsSourceSize('keys', c, monitor)).toEqual({ width: evenCeil(w), height: evenCeil(h) });
		const s = obsSourceSize('keys', c, monitor);
		expect(s.width % 2).toBe(0);
		expect(s.height % 2).toBe(0);
	});

	it('grows with scale, size and stack, and shrinks without captions', () => {
		const c = defaultOverlayConfig();
		const base = obsSourceSize('keys', c, monitor);
		const big = obsSourceSize('keys', { ...c, scale: 2 }, monitor);
		expect(big.width).toBeGreaterThan(base.width * 1.9);
		const xl = obsSourceSize('keys', { ...c, callouts: { ...c.callouts, size: 'xl' } }, monitor);
		expect(xl.height).toBeGreaterThan(base.height);
		const more = obsSourceSize(
			'keys',
			{ ...c, callouts: { ...c.callouts, max_stack: 8 } },
			monitor
		);
		expect(more.height).toBeGreaterThan(base.height);
		const bare = obsSourceSize(
			'keys',
			{ ...c, callouts: { ...c.callouts, captions: false } },
			monitor
		);
		expect(bare.height).toBeLessThan(base.height);
	});

	it('keyboard uses the layout footprint, plus the mouse when it is beside it', () => {
		const c = defaultOverlayConfig();
		c.keyboard.offset_x = 0;
		c.keyboard.offset_y = 0;
		c.keyboard.mouse = false;
		const unit = KEYBOARD_UNIT_PX.m;
		const fp = KEYBOARD_FOOTPRINTS.full;
		expect(obsSourceSize('keyboard', c, monitor)).toEqual({
			width: evenCeil(fp.w * unit),
			height: evenCeil(fp.h * unit),
		});
		const withMouse = obsSourceSize(
			'keyboard',
			{ ...c, keyboard: { ...c.keyboard, mouse: true } },
			monitor
		);
		expect(withMouse.width).toBeGreaterThan(fp.w * unit);
		const left = obsSourceSize(
			'keyboard',
			{ ...c, keyboard: { ...c.keyboard, layout: 'left' } },
			monitor
		);
		expect(left.width).toBeLessThan(fp.w * unit);
	});

	it('mouse is the diagram box', () => {
		const c = defaultOverlayConfig();
		const s = obsSourceSize('mouse', c, monitor);
		expect(s.width % 2).toBe(0);
		expect(s.height).toBeGreaterThan(160);
	});

	it('evenCeil rounds up to an even number inside OBS limits', () => {
		expect(evenCeil(1)).toBe(2);
		expect(evenCeil(3.01)).toBe(4);
		expect(evenCeil(4)).toBe(4);
		expect(evenCeil(99999)).toBe(8192);
	});
});
