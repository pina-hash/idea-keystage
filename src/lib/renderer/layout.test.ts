import { describe, expect, it } from 'vitest';
import {
	CLEARANCE_PX,
	anchorStyle,
	avoidOffsetY,
	calloutsOffsetY,
	flexAlign,
	horizontalOf,
	placedBox,
	verticalOf,
	type Box,
	type Size,
} from './layout';
import { ANCHORS, defaultOverlayConfig, type OverlayConfig } from '../protocol';
import { CALLOUT_GEOMETRY, CALLOUT_UNIT_PX, KEYBOARD_UNIT_PX, calloutHeightU } from './metrics';
import { keyboardDrawingSize, mouseDrawingSize } from './sizes';

describe('anchor placement', () => {
	it('places each anchor with inward offsets scaled', () => {
		expect(anchorStyle('bottom', 0, 96, 1)).toBe(
			'left: 50%; bottom: 96px; transform: translate(calc(-50% + 0px), 0px)'
		);
		expect(anchorStyle('top-left', 10, 20, 2)).toBe('left: 20px; top: 40px');
		expect(anchorStyle('bottom-right', 32, 32, 1)).toBe('right: 32px; bottom: 32px');
		expect(anchorStyle('center', 5, -5, 1)).toBe(
			'left: 50%; top: 50%; transform: translate(calc(-50% + 5px), calc(-50% + -5px))'
		);
		expect(anchorStyle('left', 8, 0, 1)).toBe(
			'left: 8px; top: 50%; transform: translate(0px, calc(-50% + 0px))'
		);
		expect(anchorStyle('top-left', Number.NaN, 4, 0)).toBe('left: 0px; top: 4px');
	});

	it('aligns stacks to the anchored side', () => {
		const expected: Record<string, string> = {
			'top-left': 'flex-start',
			left: 'flex-start',
			'bottom-left': 'flex-start',
			top: 'center',
			center: 'center',
			bottom: 'center',
			'top-right': 'flex-end',
			right: 'flex-end',
			'bottom-right': 'flex-end',
		};
		for (const a of ANCHORS) expect(flexAlign(a)).toBe(expected[a]);
		expect(horizontalOf('right')).toBe('end');
		expect(verticalOf('top-right')).toBe('start');
		expect(verticalOf('left')).toBe('center');
	});
});

const HD: Size = { width: 1920, height: 1080 };
const UHD: Size = { width: 3840, height: 2160 };

function intersects(a: Box, b: Box): boolean {
	return a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom;
}

/** What Stage.svelte draws for `config` on the `all` route, as stage boxes. */
function composition(config: OverlayConfig, stage: Size, scale: number) {
	const kbUnit = KEYBOARD_UNIT_PX[config.keyboard.size] * scale;
	const kbSize = keyboardDrawingSize(config.keyboard, kbUnit);
	const mouseSize = mouseDrawingSize(scale);
	const others = {
		keyboard: config.keyboard.enabled ? kbSize : null,
		mouse: config.mouse.enabled ? mouseSize : null,
	};
	const y = calloutsOffsetY(config, others, stage, scale);
	const u = CALLOUT_UNIT_PX[config.callouts.size] * scale;
	const g = CALLOUT_GEOMETRY;
	const n = config.callouts.max_stack;
	// The tallest stack the config allows, as wide as it may grow.
	const stack = placedBox(
		config.callouts.anchor,
		config.callouts.offset_x,
		y,
		scale,
		{
			width: g.maxWidthU * u,
			height: (n * calloutHeightU(config.callouts.captions) + (n - 1) * g.stackGapU) * u,
		},
		stage
	);
	const firstRow: Box = config.callouts.anchor.startsWith('top')
		? { ...stack, bottom: stack.top + calloutHeightU(config.callouts.captions) * u }
		: { ...stack, top: stack.bottom - calloutHeightU(config.callouts.captions) * u };
	const kb = placedBox(
		config.keyboard.anchor,
		config.keyboard.offset_x,
		config.keyboard.offset_y,
		scale,
		kbSize,
		stage
	);
	const mouse = placedBox(
		config.mouse.anchor,
		config.mouse.offset_x,
		config.mouse.offset_y,
		scale,
		mouseSize,
		stage
	);
	return { y, stack, firstRow, kb, mouse };
}

function withKeyboard(): OverlayConfig {
	const c = defaultOverlayConfig();
	c.keyboard.enabled = true;
	return c;
}

describe('placedBox', () => {
	it('is the box anchorStyle places', () => {
		const size = { width: 200, height: 100 };
		expect(placedBox('bottom-left', 32, 32, 1, size, HD)).toEqual({
			left: 32,
			top: 948,
			right: 232,
			bottom: 1048,
		});
		expect(placedBox('bottom', 0, 96, 1, size, HD)).toEqual({
			left: 860,
			top: 884,
			right: 1060,
			bottom: 984,
		});
		expect(placedBox('top-right', 10, 20, 2, size, UHD)).toEqual({
			left: 3620,
			top: 40,
			right: 3820,
			bottom: 140,
		});
		expect(placedBox('center', 5, -5, 1, size, HD)).toEqual({
			left: 865,
			top: 485,
			right: 1065,
			bottom: 585,
		});
	});
});

describe('the callouts keep clear of the keyboard and the mouse diagram', () => {
	it('lifts the default callouts above the default keyboard (full, mouse, bottom-left) at 1080p', () => {
		const c = withKeyboard();
		const { y, stack, firstRow, kb } = composition(c, HD, 1);
		// The keyboard (1045 x 285 at size m) spans x 32..1077, past the middle.
		expect(kb.right).toBeGreaterThan(HD.width / 2);
		expect(y).toBeCloseTo(HD.height - kb.top + CLEARANCE_PX, 6);
		expect(intersects(firstRow, kb)).toBe(false);
		expect(intersects(stack, kb)).toBe(false);
		expect(kb.top - stack.bottom).toBeCloseTo(CLEARANCE_PX, 6);
		// The tallest stack still fits on the page.
		expect(stack.top).toBeGreaterThan(0);
	});

	it('scales with the page: the same composition at 4K and scale 2', () => {
		const c = withKeyboard();
		const hd = composition(c, HD, 1);
		const uhd = composition(c, UHD, 2);
		expect(uhd.y).toBeCloseTo(hd.y, 6);
		expect(intersects(uhd.stack, uhd.kb)).toBe(false);
	});

	it('keeps the configured offset when nothing is in the way', () => {
		// 4K at scale 1: the keyboard ends at x 1077, the callouts start at 1506.
		expect(composition(withKeyboard(), UHD, 1).y).toBe(96);
		// No keyboard, or the keyboard visual off.
		expect(composition(defaultOverlayConfig(), HD, 1).y).toBe(96);
		// A keyboard that ends before the widest callout begins.
		const c = withKeyboard();
		c.keyboard.layout = 'left';
		c.keyboard.mouse = false;
		const r = composition(c, HD, 1);
		expect(r.y).toBe(96);
		expect(intersects(r.stack, r.kb)).toBe(false);
	});

	it('moves top-anchored callouts down past a keyboard at the top', () => {
		const c = withKeyboard();
		c.keyboard.anchor = 'top-left';
		c.callouts.anchor = 'top';
		c.callouts.offset_y = 40;
		const r = composition(c, HD, 1);
		expect(r.y).toBeCloseTo(r.kb.bottom + CLEARANCE_PX, 6);
		expect(intersects(r.stack, r.kb)).toBe(false);
	});

	it('clears the keyboard and the mouse diagram together', () => {
		const c = withKeyboard();
		c.keyboard.layout = 'left';
		c.keyboard.mouse = false;
		c.mouse.enabled = true;
		c.mouse.anchor = 'bottom';
		c.mouse.offset_y = 32;
		c.callouts.offset_y = 40;
		const r = composition(c, HD, 1);
		expect(intersects(r.stack, r.mouse)).toBe(false);
		expect(r.stack.bottom).toBeCloseTo(r.mouse.top - CLEARANCE_PX, 6);
	});

	it('keeps moving while the next piece is in the way, in any order', () => {
		const foot = { anchor: 'bottom' as const, offsetX: 0, offsetY: 96, width: 828, row: 100 };
		const low: Box = { left: 700, top: 880, right: 1200, bottom: 1048 };
		const high: Box = { left: 800, top: 700, right: 1100, bottom: 870 };
		// Past `low` the first row is 764..864, which `high` covers: past it too.
		expect(avoidOffsetY(foot, [low, high], HD, 1)).toBe(HD.height - 700 + CLEARANCE_PX);
		expect(avoidOffsetY(foot, [high, low], HD, 1)).toBe(HD.height - 700 + CLEARANCE_PX);
	});

	it('leaves callouts on the middle row where they are', () => {
		const c = withKeyboard();
		c.callouts.anchor = 'center';
		c.keyboard.anchor = 'center';
		expect(composition(c, HD, 1).y).toBe(c.callouts.offset_y);
	});

	it('never moves toward the edge and never off the page', () => {
		const foot = { anchor: 'bottom' as const, offsetX: 0, offsetY: 96, width: 828, row: 100 };
		// A small piece tucked under the stack, clear of its first row.
		const below: Box = { left: 900, top: 1000, right: 1000, bottom: 1070 };
		expect(avoidOffsetY(foot, [below], HD, 1)).toBe(96);
		// A piece above the first row: moving would not help, so nothing moves.
		const above: Box = { left: 900, top: 100, right: 1000, bottom: 700 };
		expect(avoidOffsetY(foot, [above], HD, 1)).toBe(96);
		// A piece almost as tall as the page: the first row stays on the page.
		const tall: Box = { left: 0, top: 20, right: 1920, bottom: 1060 };
		expect(avoidOffsetY(foot, [tall], HD, 1)).toBe(HD.height - 100);
		// Zero-size pieces (not measured yet) are ignored.
		const empty: Box = { left: 900, top: 900, right: 900, bottom: 900 };
		expect(avoidOffsetY(foot, [empty], HD, 1)).toBe(96);
	});

	it('works in px at scale 1, like anchorStyle', () => {
		const foot = { anchor: 'bottom' as const, offsetX: 0, offsetY: 48, width: 400, row: 50 };
		const piece: Box = { left: 0, top: 400, right: 1000, bottom: 540 };
		// Stage 960 x 540 at scale 0.5: the piece's top is 140 px above the bottom.
		const y = avoidOffsetY(foot, [piece], { width: 960, height: 540 }, 0.5);
		expect(y * 0.5).toBeCloseTo(140 + CLEARANCE_PX * 0.5, 6);
	});
});
