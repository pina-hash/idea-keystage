/**
 * Geometry and timing for the mouse diagram (MouseDiagram.svelte): a top
 * view of a mouse in a 64 x 100 box (width over height 0.64, MOUSE_DIAGRAM
 * in metrics.ts), with left, right and middle buttons, a wheel with ridges,
 * and two thumb buttons (X1 back, lower; X2 forward, upper) on the left side.
 * Pure data and functions, tested in mouse-diagram.test.ts.
 */
import type { MouseButton } from '../../protocol';
import type { WheelDirection, WheelPulse } from '../logic/inputs';

export const VIEW_W = 64;
export const VIEW_H = 100;

/** The body outline. */
export const BODY =
	'M33 3C47.5 3 58 12.5 58.5 30L59 67C59 86.5 47.5 97.5 33 97.5C18.5 97.5 7 86.5 7 67L7.5 30C8 12.5 18.5 3 33 3Z';

/** Where the buttons end and the palm begins. */
export const SPLIT_Y = 41;
/** Half the channel between the left and right buttons. */
const GAP = 0.9;
const CENTER_X = 33;

export interface Box {
	x: number;
	y: number;
	w: number;
	h: number;
	r: number;
}

/** Areas clipped by the body: each main button is the body above the split. */
export const LEFT_AREA: Box = { x: 0, y: 0, w: CENTER_X - GAP, h: SPLIT_Y, r: 0 };
export const RIGHT_AREA: Box = {
	x: CENTER_X + GAP,
	y: 0,
	w: VIEW_W - (CENTER_X + GAP),
	h: SPLIT_Y,
	r: 0,
};

/** The wheel's well and the wheel itself. */
export const WHEEL_WELL: Box = { x: 28.4, y: 13, w: 9.2, h: 20.4, r: 4.6 };
export const WHEEL: Box = { x: 29.6, y: 14.3, w: 6.8, h: 17.8, r: 3.4 };
/** Spacing of the ridges on the wheel (one notch scrolls one ridge). */
export const RIDGE_STEP = 2.5;

/** Thumb buttons: X2 (forward) above X1 (back), on the left side. */
export const X2_BOX: Box = { x: 2.2, y: 46, w: 8.4, h: 10.5, r: 3.4 };
export const X1_BOX: Box = { x: 2.2, y: 58.5, w: 8.4, h: 10.5, r: 3.4 };

/** The accent tab near the tail, the plate's machined detail. */
export const TAB = 'M27 87H39L37.8 88.4H28.2Z';

export type MousePart = MouseButton;

/** Center a pressed part shrinks toward, and how much it shrinks. */
export const PRESS: Readonly<Record<MousePart, { cx: number; cy: number; scale: number }>> = {
	left: { cx: 19, cy: 23, scale: 0.93 },
	right: { cx: 47, cy: 23, scale: 0.93 },
	middle: { cx: WHEEL.x + WHEEL.w / 2, cy: WHEEL.y + WHEEL.h / 2, scale: 0.86 },
	x1: { cx: X1_BOX.x + X1_BOX.w / 2, cy: X1_BOX.y + X1_BOX.h / 2, scale: 0.82 },
	x2: { cx: X2_BOX.x + X2_BOX.w / 2, cy: X2_BOX.y + X2_BOX.h / 2, scale: 0.82 },
};

/** Lines across the wheel, as path data (a few extra so a scroll never shows a gap). */
export function ridgesPath(): string {
	const parts: string[] = [];
	const x0 = WHEEL.x + 1.2;
	const x1 = WHEEL.x + WHEEL.w - 1.2;
	for (let y = WHEEL.y - RIDGE_STEP * 2; y <= WHEEL.y + WHEEL.h + RIDGE_STEP * 2; y += RIDGE_STEP) {
		parts.push(`M${x0} ${Math.round(y * 100) / 100}H${x1}`);
	}
	return parts.join('');
}

/**
 * Chevrons for a wheel direction: two stacked chevrons pointing the way the
 * wheel turned, above the wheel for up, below for down, beside it for a tilt.
 * Returns the path of each chevron (nearest the wheel first) and the
 * distance (in view units) they travel while they fade.
 */
export function chevrons(direction: WheelDirection): { paths: string[]; dx: number; dy: number } {
	const cx = WHEEL.x + WHEEL.w / 2;
	const cy = WHEEL.y + WHEEL.h / 2;
	const half = 3.4;
	const depth = 2.6;
	const step = 2.4;
	switch (direction) {
		case 'up': {
			const y0 = WHEEL_WELL.y - 1.4;
			return {
				paths: [0, 1].map((i) => {
					const y = y0 - i * step;
					return `M${cx - half} ${y}L${cx} ${y - depth}L${cx + half} ${y}`;
				}),
				dx: 0,
				dy: -1.4,
			};
		}
		case 'down': {
			const y0 = WHEEL_WELL.y + WHEEL_WELL.h + 1.4;
			return {
				paths: [0, 1].map((i) => {
					const y = y0 + i * step;
					return `M${cx - half} ${y}L${cx} ${y + depth}L${cx + half} ${y}`;
				}),
				dx: 0,
				dy: 1.2,
			};
		}
		case 'left': {
			const x0 = WHEEL_WELL.x - 1.6;
			return {
				paths: [0, 1].map((i) => {
					const x = x0 - i * step;
					return `M${x} ${cy - half}L${x - depth} ${cy}L${x} ${cy + half}`;
				}),
				dx: -1.4,
				dy: 0,
			};
		}
		case 'right': {
			const x0 = WHEEL_WELL.x + WHEEL_WELL.w + 1.6;
			return {
				paths: [0, 1].map((i) => {
					const x = x0 + i * step;
					return `M${x} ${cy - half}L${x + depth} ${cy}L${x} ${cy + half}`;
				}),
				dx: 1.4,
				dy: 0,
			};
		}
	}
}

/** How long one wheel notch is shown (chevrons and the lit wheel). */
export const WHEEL_SHOW_MS = 460;
/** A pulse older than this when first seen is not animated (a stale replay). */
export const WHEEL_STALE_MS = 300;

/**
 * Whether a wheel pulse should start an animation: it must be newer than the
 * last one animated and recent enough to still be news.
 */
export function shouldAnimateWheel(
	pulse: WheelPulse | null,
	lastSeq: number,
	now: number
): boolean {
	if (!pulse) return false;
	if (pulse.seq <= lastSeq) return false;
	return now - pulse.at < WHEEL_STALE_MS;
}

/** Ridges scroll one step per notch, up to this many steps per pulse. */
export const MAX_RIDGE_STEPS = 4;

export function ridgeSteps(pulse: WheelPulse): number {
	return Math.max(1, Math.min(MAX_RIDGE_STEPS, Math.round(pulse.notches)));
}

/** Default fade after a button is released, when no decay time is given. */
export const BUTTON_FADE_MS = 360;

/** Rounded rectangle path data. */
export function boxPath(b: Box): string {
	const r = Math.min(b.r, b.w / 2, b.h / 2);
	const x1 = b.x + b.w;
	const y1 = b.y + b.h;
	if (r <= 0) return `M${b.x} ${b.y}H${x1}V${y1}H${b.x}Z`;
	return (
		`M${b.x + r} ${b.y}H${x1 - r}A${r} ${r} 0 0 1 ${x1} ${b.y + r}V${y1 - r}` +
		`A${r} ${r} 0 0 1 ${x1 - r} ${y1}H${b.x + r}A${r} ${r} 0 0 1 ${b.x} ${y1 - r}` +
		`V${b.y + r}A${r} ${r} 0 0 1 ${b.x + r} ${b.y}Z`
	);
}
