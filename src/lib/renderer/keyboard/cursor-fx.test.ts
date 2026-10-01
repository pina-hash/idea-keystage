import { describe, expect, it } from 'vitest';
import type { CursorConfig, MouseButton } from '../../protocol';
import { MOUSE_BUTTONS, defaultOverlayConfig } from '../../protocol';
import {
	applyButton,
	applyMove,
	applyWheel,
	emptyButtons,
	emptyMouse,
	type ClickEvent,
	type MouseButtons,
	type MouseState,
	type WheelPulse,
} from '../logic/inputs';
import {
	PRESS_IN_MS,
	PRESS_OUT_MS,
	RIPPLE_MS,
	RIPPLE_SHAPES,
	TRAIL_MAX,
	TRAIL_MS,
	Trail,
	WHEEL_MS,
	drawScene,
	easeOutCubic,
	fxStyle,
	liveRipples,
	liveWheels,
	monitorRect,
	pressAmount,
	pressSettled,
	progress,
	rippleShapeFor,
	toPage,
	type Ctx,
	type Scene,
} from './cursor-fx';

/** A canvas context that records what was drawn. */
class FakeCtx implements Ctx {
	calls: string[] = [];
	fillStyle: unknown = '';
	strokeStyle: unknown = '';
	lineWidth = 1;
	lineCap = 'butt';
	lineJoin = 'miter';
	globalAlpha = 1;
	gradients = 0;
	save() {
		this.calls.push('save');
	}
	restore() {
		this.calls.push('restore');
	}
	beginPath() {
		this.calls.push('beginPath');
	}
	closePath() {
		this.calls.push('closePath');
	}
	moveTo() {
		this.calls.push('moveTo');
	}
	lineTo() {
		this.calls.push('lineTo');
	}
	arc() {
		this.calls.push('arc');
	}
	arcTo() {
		this.calls.push('arcTo');
	}
	fill() {
		this.calls.push('fill');
	}
	stroke() {
		this.calls.push('stroke');
	}
	fillRect(x: number, y: number, w: number, h: number) {
		this.calls.push(`fillRect:${x},${y},${w},${h}`);
	}
	createRadialGradient() {
		this.gradients += 1;
		return { addColorStop: () => undefined };
	}
	linear = 0;
	createLinearGradient() {
		this.linear += 1;
		return { addColorStop: () => undefined };
	}
	count(name: string) {
		return this.calls.filter((c) => c === name).length;
	}
}

function cursorCfg(over: Partial<CursorConfig> = {}): CursorConfig {
	return { ...defaultOverlayConfig().cursor, enabled: true, ...over };
}

function scene(
	mouse: MouseState,
	over: Partial<Scene> = {},
	cfg: Partial<CursorConfig> = {}
): Scene {
	const cursor = cursorCfg(cfg);
	return {
		cursor,
		style: fxStyle(cursor, 1),
		width: 1920,
		height: 1080,
		rect: monitorRect(1920, 1080, 16 / 9),
		pointer: mouse.pointer,
		buttons: mouse.buttons,
		clicks: mouse.clicks,
		wheels: mouse.wheels,
		trail: [],
		...over,
	};
}

function click(button: MouseButton, at: number): ClickEvent {
	return { seq: at, button, x: 0.5, y: 0.5, at };
}

describe('mapping to the page', () => {
	it('fills the page when it has the shape of the monitor', () => {
		expect(monitorRect(1920, 1080, 16 / 9)).toEqual({ x: 0, y: 0, w: 1920, h: 1080 });
		expect(monitorRect(3840, 2160, 3840 / 2160)).toEqual({ x: 0, y: 0, w: 3840, h: 2160 });
	});

	it('fits the monitor inside a page of another shape, centered', () => {
		const pillar = monitorRect(1000, 400, 16 / 9);
		expect(pillar.h).toBe(400);
		expect(pillar.w).toBeCloseTo(711.11, 1);
		expect(pillar.x).toBeCloseTo((1000 - pillar.w) / 2, 9);
		const letter = monitorRect(800, 800, 2);
		expect(letter).toEqual({ x: 0, y: 200, w: 800, h: 400 });
		expect(monitorRect(0, 100, 1)).toEqual({ x: 0, y: 0, w: 0, h: 0 });
		expect(monitorRect(400, 200, Number.NaN)).toEqual({ x: 0, y: 0, w: 400, h: 200 });
	});

	it('maps normalized positions, including outside the monitor', () => {
		const r = monitorRect(1920, 1080, 16 / 9);
		expect(toPage(r, 0.5, 0.25)).toEqual({ x: 960, y: 270 });
		expect(toPage(r, 1, 1)).toEqual({ x: 1920, y: 1080 });
		expect(toPage(r, -0.1, 0)).toEqual({ x: -192, y: 0 });
		const boxed = monitorRect(800, 800, 2);
		expect(toPage(boxed, 0, 0)).toEqual({ x: 0, y: 200 });
	});
});

describe('ripples', () => {
	it('gives every button its own shape', () => {
		expect(rippleShapeFor('left')).toBe('ring');
		expect(rippleShapeFor('right')).toBe('square');
		expect(rippleShapeFor('middle')).toBe('pulse');
		expect(rippleShapeFor('x1')).toBe('back');
		expect(rippleShapeFor('x2')).toBe('forward');
		const shapes = MOUSE_BUTTONS.map((b) => RIPPLE_SHAPES[b]);
		expect(new Set(shapes).size).toBe(MOUSE_BUTTONS.length);
	});

	it('draws a single ring for left, rounded squares for right, a filled pulse with a dot for middle', () => {
		const m = emptyMouse();
		const at = 1000;
		const now = at + RIPPLE_MS / 3;

		const left = new FakeCtx();
		drawScene(left, scene(m, { clicks: [click('left', at)] }, { ripples: true }), now);
		expect(left.count('arc')).toBe(1);
		expect(left.count('arcTo')).toBe(0);
		expect(left.count('fill')).toBe(0);
		expect(left.count('stroke')).toBe(2); // rim and color

		const right = new FakeCtx();
		drawScene(right, scene(m, { clicks: [click('right', at)] }), now);
		expect(right.count('arcTo')).toBe(8); // two squares, four corners each
		expect(right.count('arc')).toBe(0);

		const middle = new FakeCtx();
		drawScene(middle, scene(m, { clicks: [click('middle', at)] }), now);
		expect(middle.count('arc')).toBe(4); // the pulse, its edge, the dot's rim, the dot
		expect(middle.count('fill')).toBe(3);
		expect(middle.count('stroke')).toBe(2); // only the pulse edge, rim and color

		for (const b of ['x1', 'x2'] as const) {
			const side = new FakeCtx();
			drawScene(side, scene(m, { clicks: [click(b, at)] }), now);
			expect(side.count('lineTo')).toBe(4); // two chevrons
			expect(side.count('arc')).toBe(0);
		}
	});

	it('keeps animating only while a ripple runs, and draws none when ripples are off', () => {
		const m = emptyMouse();
		const c = [click('left', 1000)];
		expect(drawScene(new FakeCtx(), scene(m, { clicks: c }), 1000 + RIPPLE_MS / 2)).toBe(true);
		expect(drawScene(new FakeCtx(), scene(m, { clicks: c }), 1000 + RIPPLE_MS)).toBe(false);
		const off = new FakeCtx();
		expect(drawScene(off, scene(m, { clicks: c }, { ripples: false }), 1000 + RIPPLE_MS / 2)).toBe(
			false
		);
		expect(off.count('arc')).toBe(0);
		expect(liveRipples(c, 1000 + RIPPLE_MS - 1)).toHaveLength(1);
		expect(liveRipples(c, 1000 + RIPPLE_MS)).toHaveLength(0);
	});

	it('comes from button presses in the mouse state', () => {
		let m = emptyMouse();
		m = applyButton(m, { t: 'button', button: 'right', down: true, x: 0.2, y: 0.3, t_ms: 1 }, 500);
		m = applyButton(m, { t: 'button', button: 'right', down: false, x: 0.2, y: 0.3, t_ms: 2 }, 560);
		expect(m.clicks).toHaveLength(1);
		const ctx = new FakeCtx();
		drawScene(ctx, scene(m), 600);
		expect(ctx.count('arcTo')).toBe(8);
	});
});

describe('timing', () => {
	it('measures progress and eases out', () => {
		expect(progress(100, 100, 200)).toBe(0);
		expect(progress(100, 200, 200)).toBe(0.5);
		expect(progress(100, 300, 200)).toBeNull();
		expect(progress(100, 50, 200)).toBeNull();
		expect(easeOutCubic(0)).toBe(0);
		expect(easeOutCubic(1)).toBe(1);
		expect(easeOutCubic(0.5)).toBeCloseTo(0.875, 9);
		expect(easeOutCubic(2)).toBe(1);
	});

	it('squeezes the highlight on a press and relaxes it after', () => {
		const b = emptyButtons();
		expect(pressAmount(b, 0)).toBe(0);
		expect(pressSettled(b, 0)).toBe(true);
		const held: MouseButtons = { ...b, left: { down: true, pressedAt: 1000, releasedAt: null } };
		expect(pressAmount(held, 1000)).toBe(0);
		expect(pressSettled(held, 1000)).toBe(false);
		expect(pressAmount(held, 1000 + PRESS_IN_MS / 2)).toBeCloseTo(0.5, 9);
		expect(pressAmount(held, 1000 + PRESS_IN_MS)).toBe(1);
		expect(pressSettled(held, 1000 + PRESS_IN_MS)).toBe(true);
		const released: MouseButtons = {
			...b,
			left: { down: false, pressedAt: 1000, releasedAt: 2000 },
		};
		expect(pressAmount(released, 2000)).toBe(1);
		expect(pressSettled(released, 2000)).toBe(false);
		expect(pressAmount(released, 2000 + PRESS_OUT_MS / 2)).toBeCloseTo(0.5, 9);
		expect(pressAmount(released, 2000 + PRESS_OUT_MS)).toBe(0);
		expect(pressSettled(released, 2000 + PRESS_OUT_MS)).toBe(true);
	});

	it('animates a press even with ripples off, then stops', () => {
		let m = applyMove(emptyMouse(), { t: 'move', x: 0.5, y: 0.5, t_ms: 1 }, 10);
		m = applyButton(m, { t: 'button', button: 'left', down: true, x: 0.5, y: 0.5, t_ms: 2 }, 1000);
		expect(drawScene(new FakeCtx(), scene(m, {}, { ripples: false }), 1000)).toBe(true);
		expect(drawScene(new FakeCtx(), scene(m, {}, { ripples: false }), 1000 + PRESS_IN_MS)).toBe(
			false
		);
	});
});

describe('the highlight', () => {
	const moved = applyMove(emptyMouse(), { t: 'move', x: 0.25, y: 0.5, t_ms: 1 }, 10);

	it('draws nothing before the pointer position is known', () => {
		const ctx = new FakeCtx();
		expect(drawScene(ctx, scene(emptyMouse()), 100)).toBe(false);
		expect(ctx.calls.filter((c) => c !== 'save' && c !== 'restore')).toEqual([]);
	});

	it('draws a ring, a dot, or a spotlight that darkens the page around the cursor', () => {
		const ring = new FakeCtx();
		expect(drawScene(ring, scene(moved, {}, { style: 'ring' }), 100)).toBe(false);
		expect(ring.count('arc')).toBe(1);
		expect(ring.count('stroke')).toBe(2);
		expect(ring.count('fill')).toBe(1);

		const dot = new FakeCtx();
		drawScene(dot, scene(moved, {}, { style: 'dot' }), 100);
		expect(dot.count('arc')).toBe(2);
		expect(dot.count('fill')).toBe(2);
		expect(dot.count('stroke')).toBe(0);

		const spot = new FakeCtx();
		drawScene(spot, scene(moved, {}, { style: 'spotlight' }), 100);
		expect(spot.gradients).toBe(1);
		expect(spot.calls).toContain('fillRect:0,0,1920,1080');
		expect(spot.count('stroke')).toBe(1);
	});

	it('sizes from the config at scale 1 and grows with the scale', () => {
		const a = fxStyle(cursorCfg({ size: 44 }), 1);
		const b = fxStyle(cursorCfg({ size: 44 }), 2);
		expect(a.radius).toBe(22);
		expect(b.radius).toBe(44);
		expect(b.spot).toBe(a.spot * 2);
		expect(fxStyle(cursorCfg({ opacity: 4 }), 1).opacity).toBe(1);
		expect(fxStyle(cursorCfg({ opacity: -1 }), 1).opacity).toBe(0);
		expect(fxStyle(cursorCfg({ color: 'nope' }), 1).color).toEqual({
			r: 120,
			g: 184,
			b: 112,
			a: 1,
		});
		expect(fxStyle(cursorCfg({ color: 'rgba(255, 0, 0, 0.2)' }), 1).color).toEqual({
			r: 255,
			g: 0,
			b: 0,
			a: 1,
		});
		expect(fxStyle(cursorCfg(), Number.NaN).scale).toBe(1);
	});
});

describe('the wheel indicator', () => {
	function pulse(direction: WheelPulse['direction'], notches: number, at: number): WheelPulse {
		return { seq: at, direction, notches, x: 0.5, y: 0.5, at };
	}

	it('draws one chevron per notch, up to three, and stops when done', () => {
		const m = applyMove(emptyMouse(), { t: 'move', x: 0.5, y: 0.5, t_ms: 1 }, 1);
		const one = new FakeCtx();
		expect(drawScene(one, scene(m, { wheels: [pulse('up', 1, 100)] }), 150)).toBe(true);
		expect(one.count('lineTo')).toBe(2);
		const many = new FakeCtx();
		drawScene(many, scene(m, { wheels: [pulse('down', 9, 100)] }), 150);
		expect(many.count('lineTo')).toBe(6);
		expect(
			drawScene(new FakeCtx(), scene(m, { wheels: [pulse('up', 1, 100)] }), 100 + WHEEL_MS)
		).toBe(false);
		const off = new FakeCtx();
		drawScene(off, scene(m, { wheels: [pulse('up', 1, 100)] }, { wheel: false }), 150);
		expect(off.count('lineTo')).toBe(0);
	});

	it('comes from wheel messages', () => {
		const m = applyWheel(
			emptyMouse(),
			{ t: 'wheel', dx: 0, dy: -240, x: 0.5, y: 0.5, t_ms: 1 },
			100
		);
		expect(liveWheels(m.wheels, 120)).toHaveLength(1);
		expect(m.wheels[0]?.direction).toBe('down');
		expect(m.wheels[0]?.notches).toBe(2);
		const ctx = new FakeCtx();
		drawScene(ctx, scene(m), 120);
		expect(ctx.count('lineTo')).toBe(4);
	});
});

describe('the drag trail', () => {
	it('records positions only while a button is held, as separate strokes', () => {
		const t = new Trail();
		t.record(0.1, 0.1, 0, false);
		expect(t.points).toHaveLength(0);
		t.record(0.1, 0.1, 10, true);
		t.record(0.2, 0.1, 20, true);
		t.record(0.2, 0.1, 25, true); // unchanged position
		t.record(0.3, 0.1, 30, false);
		t.record(0.5, 0.5, 40, true);
		expect(t.points.map((p) => p.stroke)).toEqual([1, 1, 2]);
		expect(t.points).toHaveLength(3);
	});

	it('fades points out after TRAIL_MS and stays bounded', () => {
		const t = new Trail();
		for (let i = 0; i < TRAIL_MAX + 50; i++) t.record(i / 1000, 0.5, i, true);
		expect(t.points.length).toBe(TRAIL_MAX);
		expect(t.prune(TRAIL_MAX + 49 + TRAIL_MS - 1)).toBe(true);
		expect(t.prune(TRAIL_MAX + 50 + TRAIL_MS)).toBe(false);
		expect(t.points).toHaveLength(0);
		t.record(0.1, 0.1, 0, true);
		t.clear();
		expect(t.points).toHaveLength(0);
	});

	it('draws segments within a stroke only, and only when the trail is on', () => {
		const t = new Trail();
		t.record(0.1, 0.1, 100, true);
		t.record(0.2, 0.1, 110, true);
		t.record(0.3, 0.1, 120, true);
		t.record(0, 0, 125, false);
		t.record(0.6, 0.6, 130, true);
		t.record(0.7, 0.6, 140, true);
		const m = emptyMouse();
		const on = new FakeCtx();
		expect(drawScene(on, scene(m, { trail: t.points }, { trail: true }), 150)).toBe(true);
		expect(on.count('lineTo')).toBe(3);
		// One path per stroke (no overlapping segments), a dark rim and the color.
		expect(on.count('beginPath')).toBe(2);
		expect(on.count('stroke')).toBe(4);
		expect(on.linear).toBe(4); // the alpha runs from tail to head
		// A fully faded stroke is not drawn.
		const old = new FakeCtx();
		drawScene(old, scene(m, { trail: t.points }, { trail: true }), 140 + TRAIL_MS);
		expect(old.count('stroke')).toBe(0);
		const off = new FakeCtx();
		drawScene(off, scene(m, { trail: t.points }, { trail: false }), 150);
		expect(off.count('lineTo')).toBe(0);
	});
});
