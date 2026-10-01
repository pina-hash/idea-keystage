import { describe, expect, it } from 'vitest';
import { MOUSE_BUTTONS } from '../../protocol';
import { MOUSE_DIAGRAM } from '../metrics';
import type { WheelPulse } from '../logic/inputs';
import {
	BODY,
	LEFT_AREA,
	MAX_RIDGE_STEPS,
	PRESS,
	RIDGE_STEP,
	RIGHT_AREA,
	SPLIT_Y,
	VIEW_H,
	VIEW_W,
	WHEEL,
	WHEEL_STALE_MS,
	WHEEL_WELL,
	X1_BOX,
	X2_BOX,
	boxPath,
	chevrons,
	ridgeSteps,
	ridgesPath,
	shouldAnimateWheel,
} from './mouse-diagram';

function pulse(seq: number, at: number, notches = 1): WheelPulse {
	return { seq, direction: 'up', notches, x: 0.5, y: 0.5, at };
}

function coords(d: string): number[][] {
	return [...d.matchAll(/(-?[\d.]+)[ ,](-?[\d.]+)/g)].map((m) => [Number(m[1]), Number(m[2])]);
}

describe('mouse diagram geometry', () => {
	it('has the proportions the renderer sizes it by', () => {
		expect(VIEW_W / VIEW_H).toBeCloseTo(MOUSE_DIAGRAM.aspect, 9);
	});

	it('keeps every part inside the view box', () => {
		for (const d of [
			BODY,
			boxPath(WHEEL),
			boxPath(WHEEL_WELL),
			boxPath(X1_BOX),
			boxPath(X2_BOX),
			ridgesPath(),
		]) {
			for (const [x, y] of coords(d)) {
				expect(x).toBeGreaterThanOrEqual(0);
				expect(x).toBeLessThanOrEqual(VIEW_W);
				expect(y).toBeGreaterThanOrEqual(-2 * RIDGE_STEP);
				expect(y).toBeLessThanOrEqual(VIEW_H);
			}
		}
	});

	it('splits the buttons with a channel and seats the wheel over it', () => {
		expect(LEFT_AREA.x + LEFT_AREA.w).toBeLessThan(RIGHT_AREA.x);
		const mid = (LEFT_AREA.x + LEFT_AREA.w + RIGHT_AREA.x) / 2;
		expect(WHEEL.x + WHEEL.w / 2).toBeCloseTo(mid, 9);
		expect(WHEEL.x).toBeGreaterThan(WHEEL_WELL.x);
		expect(WHEEL.y + WHEEL.h).toBeLessThan(WHEEL_WELL.y + WHEEL_WELL.h);
		expect(X2_BOX.y + X2_BOX.h).toBeLessThan(X1_BOX.y); // forward above back
	});

	it('shrinks every pressed part toward its own center', () => {
		for (const b of MOUSE_BUTTONS) {
			expect(PRESS[b].scale, b).toBeGreaterThan(0.7);
			expect(PRESS[b].scale, b).toBeLessThan(1);
		}
		expect(PRESS.left.cx).toBeLessThan(PRESS.right.cx);
		expect(PRESS.x2.cy).toBeLessThan(PRESS.x1.cy);
	});

	it('draws rounded boxes', () => {
		expect(boxPath({ x: 0, y: 0, w: 10, h: 4, r: 1 })).toBe(
			'M1 0H9A1 1 0 0 1 10 1V3A1 1 0 0 1 9 4H1A1 1 0 0 1 0 3V1A1 1 0 0 1 1 0Z'
		);
		expect(boxPath({ x: 0, y: 0, w: 10, h: 4, r: 0 })).toBe('M0 0H10V4H0Z');
		expect(boxPath({ x: 0, y: 0, w: 10, h: 4, r: 9 })).toContain('A2 2');
	});
});

describe('wheel chevrons', () => {
	it('point the way the wheel turned and travel that way', () => {
		const up = chevrons('up');
		const down = chevrons('down');
		const left = chevrons('left');
		const right = chevrons('right');
		for (const c of [up, down, left, right]) expect(c.paths).toHaveLength(2);
		expect(up.dy).toBeLessThan(0);
		expect(down.dy).toBeGreaterThan(0);
		expect(left.dx).toBeLessThan(0);
		expect(right.dx).toBeGreaterThan(0);
		const tip = (d: string): [number, number] => {
			const p = coords(d)[1] ?? [];
			return [p[0] ?? Number.NaN, p[1] ?? Number.NaN];
		};
		// Up chevrons sit above the wheel, down chevrons below it.
		expect(tip(up.paths[0] as string)[1]).toBeLessThan(WHEEL_WELL.y);
		expect(tip(down.paths[0] as string)[1]).toBeGreaterThan(WHEEL_WELL.y + WHEEL_WELL.h);
		expect(tip(left.paths[0] as string)[0]).toBeLessThan(WHEEL_WELL.x);
		expect(tip(right.paths[0] as string)[0]).toBeGreaterThan(WHEEL_WELL.x + WHEEL_WELL.w);
		// The second chevron is farther out than the first.
		expect(tip(up.paths[1] as string)[1]).toBeLessThan(tip(up.paths[0] as string)[1]);
		expect(tip(down.paths[1] as string)[1]).toBeGreaterThan(tip(down.paths[0] as string)[1]);
		// Up chevrons stay inside the body's top and down chevrons above the palm,
		// all the way through their travel.
		expect(tip(up.paths[1] as string)[1] + up.dy).toBeGreaterThan(4);
		expect(tip(down.paths[1] as string)[1] + down.dy).toBeLessThanOrEqual(SPLIT_Y);
	});
});

describe('wheel animation', () => {
	it('animates each new pulse once, and never a stale one', () => {
		expect(shouldAnimateWheel(null, 0, 0)).toBe(false);
		expect(shouldAnimateWheel(pulse(5, 1000), 4, 1010)).toBe(true);
		expect(shouldAnimateWheel(pulse(5, 1000), 5, 1010)).toBe(false);
		expect(shouldAnimateWheel(pulse(5, 1000), 9, 1010)).toBe(false);
		expect(shouldAnimateWheel(pulse(6, 1000), 5, 1000 + WHEEL_STALE_MS)).toBe(false);
	});

	it('scrolls the ridges one step per notch, within a limit', () => {
		expect(ridgeSteps(pulse(1, 0, 1))).toBe(1);
		expect(ridgeSteps(pulse(1, 0, 3))).toBe(3);
		expect(ridgeSteps(pulse(1, 0, 40))).toBe(MAX_RIDGE_STEPS);
		expect(ridgeSteps(pulse(1, 0, 0))).toBe(1);
	});

	it('draws ridges past both ends of the wheel so a scroll never shows a gap', () => {
		const ys = coords(ridgesPath()).map((p) => p[1] as number);
		expect(Math.min(...ys)).toBeLessThanOrEqual(WHEEL.y - RIDGE_STEP);
		expect(Math.max(...ys)).toBeGreaterThanOrEqual(WHEEL.y + WHEEL.h + RIDGE_STEP);
	});
});
