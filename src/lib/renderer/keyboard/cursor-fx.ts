/**
 * Cursor effects (CursorFx.svelte): the pointer highlight (ring, spotlight or
 * dot), click ripples whose SHAPE says which button was pressed, a drag
 * trail and a wheel indicator, drawn on a canvas at devicePixelRatio.
 *
 * Positions arrive normalized to the chosen monitor (0..1, may fall
 * outside). The page normally IS that monitor (Host A, or an OBS source the
 * size of the monitor laid over a Display Capture of it), so x 0..1 spans
 * the page; when the page has another shape (the settings preview), the
 * monitor is fitted inside it, centered, so the route still lines up.
 *
 * Everything here is pure (an injected clock, a minimal canvas interface) and
 * tested in cursor-fx.test.ts. The component only owns the canvas, the
 * animation frame and the trail buffer.
 */
import type { CursorConfig, MouseButton } from '../../protocol';
import { parseColor, toCss, type Rgba } from '../../themes/color';
import type { ClickEvent, MouseButtons, PointerState, WheelPulse } from '../logic/inputs';

/* ------------------------------------------------------------------------ */
/* Mapping                                                                    */
/* ------------------------------------------------------------------------ */

export interface Rect {
	x: number;
	y: number;
	w: number;
	h: number;
}

/** The monitor's rectangle inside a page of `w` x `h` CSS px (contain, centered). */
export function monitorRect(w: number, h: number, aspect: number): Rect {
	if (!(w > 0) || !(h > 0)) return { x: 0, y: 0, w: 0, h: 0 };
	const a = Number.isFinite(aspect) && aspect > 0 ? aspect : w / h;
	if (w / h > a) {
		const mw = h * a;
		return { x: (w - mw) / 2, y: 0, w: mw, h };
	}
	const mh = w / a;
	return { x: 0, y: (h - mh) / 2, w, h: mh };
}

/** A normalized monitor position to page px. */
export function toPage(rect: Rect, nx: number, ny: number): { x: number; y: number } {
	return { x: rect.x + nx * rect.w, y: rect.y + ny * rect.h };
}

/* ------------------------------------------------------------------------ */
/* Timing                                                                     */
/* ------------------------------------------------------------------------ */

export const RIPPLE_MS = 620;
export const WHEEL_MS = 520;
export const TRAIL_MS = 650;
/** How long the highlight takes to squeeze on a press and to relax after. */
export const PRESS_IN_MS = 90;
export const PRESS_OUT_MS = 180;

/** Progress 0..1 of an effect that started at `at`, or null when over (or not started). */
export function progress(at: number, now: number, duration: number): number | null {
	const t = (now - at) / duration;
	if (!Number.isFinite(t) || t < 0 || t >= 1) return null;
	return t;
}

export function easeOutCubic(t: number): number {
	const c = Math.min(1, Math.max(0, t));
	return 1 - (1 - c) ** 3;
}

/**
 * How squeezed the highlight is, 0..1: rises over PRESS_IN_MS after the
 * latest press while any button is held, relaxes over PRESS_OUT_MS after the
 * last release.
 */
export function pressAmount(buttons: MouseButtons, now: number): number {
	let anyDown = false;
	let lastPress = -Infinity;
	let lastRelease = -Infinity;
	for (const b of Object.values(buttons)) {
		if (b.down) {
			anyDown = true;
			if (b.pressedAt !== null) lastPress = Math.max(lastPress, b.pressedAt);
		} else if (b.releasedAt !== null) {
			lastRelease = Math.max(lastRelease, b.releasedAt);
		}
	}
	if (anyDown) {
		if (!Number.isFinite(lastPress)) return 1;
		return Math.min(1, Math.max(0, (now - lastPress) / PRESS_IN_MS));
	}
	if (!Number.isFinite(lastRelease)) return 0;
	return 1 - Math.min(1, Math.max(0, (now - lastRelease) / PRESS_OUT_MS));
}

/** True when the squeeze has reached its resting value (1 while held, 0 after). */
export function pressSettled(buttons: MouseButtons, now: number): boolean {
	const s = pressAmount(buttons, now);
	return anyButtonDown(buttons) ? s >= 1 : s <= 0;
}

export function anyButtonDown(buttons: MouseButtons): boolean {
	return Object.values(buttons).some((b) => b.down);
}

/* ------------------------------------------------------------------------ */
/* Ripples                                                                    */
/* ------------------------------------------------------------------------ */

/**
 * The ripple shape for each button. Shape, not color, tells them apart:
 *   left    one expanding ring
 *   right   two expanding rounded squares
 *   middle  a filled pulse with a solid center dot
 *   x1      chevrons sweeping left (back)
 *   x2      chevrons sweeping right (forward)
 */
export type RippleShape = 'ring' | 'square' | 'pulse' | 'back' | 'forward';

export const RIPPLE_SHAPES: Readonly<Record<MouseButton, RippleShape>> = {
	left: 'ring',
	right: 'square',
	middle: 'pulse',
	x1: 'back',
	x2: 'forward',
};

export function rippleShapeFor(button: MouseButton): RippleShape {
	return RIPPLE_SHAPES[button];
}

/** Clicks whose ripple is still running at `now`. */
export function liveRipples(clicks: readonly ClickEvent[], now: number): ClickEvent[] {
	return clicks.filter((c) => progress(c.at, now, RIPPLE_MS) !== null);
}

/** Wheel pulses whose indicator is still running at `now`. */
export function liveWheels(wheels: readonly WheelPulse[], now: number): WheelPulse[] {
	return wheels.filter((w) => progress(w.at, now, WHEEL_MS) !== null);
}

/* ------------------------------------------------------------------------ */
/* Trail                                                                      */
/* ------------------------------------------------------------------------ */

export interface TrailPoint {
	x: number;
	y: number;
	at: number;
	/** Points of one drag share a stroke number; a new press starts a new stroke. */
	stroke: number;
}

/** Most points kept (125 Hz for well over TRAIL_MS). */
export const TRAIL_MAX = 160;

/** A small ring buffer of recent drag positions (normalized). */
export class Trail {
	private pts: TrailPoint[] = [];
	private strokeId = 0;
	private open = false;

	get points(): readonly TrailPoint[] {
		return this.pts;
	}

	/** Records a position while a button is held; `held` false ends the stroke. */
	record(x: number, y: number, at: number, held: boolean): void {
		if (!held) {
			this.open = false;
			return;
		}
		if (!this.open) {
			this.strokeId += 1;
			this.open = true;
		}
		const last = this.pts[this.pts.length - 1];
		if (last && last.stroke === this.strokeId && last.x === x && last.y === y) return;
		this.pts.push({ x, y, at, stroke: this.strokeId });
		if (this.pts.length > TRAIL_MAX) this.pts.splice(0, this.pts.length - TRAIL_MAX);
	}

	/** Drops points that have faded. Returns true when any remain. */
	prune(now: number): boolean {
		let i = 0;
		while (i < this.pts.length && now - (this.pts[i] as TrailPoint).at >= TRAIL_MS) i += 1;
		if (i > 0) this.pts.splice(0, i);
		return this.pts.length > 0;
	}

	clear(): void {
		this.pts = [];
		this.open = false;
	}
}

/* ------------------------------------------------------------------------ */
/* Sizes and colors                                                           */
/* ------------------------------------------------------------------------ */

export const FALLBACK_COLOR = '#78b870';

export interface FxStyle {
	color: Rgba;
	opacity: number;
	/** Highlight radius (ring, dot) in page px. */
	radius: number;
	/** Spotlight hole radius in page px. */
	spot: number;
	/** A thin line width in page px. */
	line: number;
	scale: number;
}

export function fxStyle(cursor: CursorConfig, scale: number): FxStyle {
	const s = Number.isFinite(scale) && scale > 0 ? scale : 1;
	const size = Number.isFinite(cursor.size) && cursor.size > 0 ? cursor.size : 44;
	const parsed = parseColor(cursor.color) ?? (parseColor(FALLBACK_COLOR) as Rgba);
	const opacity = Number.isFinite(cursor.opacity) ? Math.min(1, Math.max(0, cursor.opacity)) : 0.85;
	return {
		color: { ...parsed, a: 1 },
		opacity,
		radius: (size / 2) * s,
		spot: size * 2.2 * s,
		line: Math.max(1.5, size * 0.075 * s),
		scale: s,
	};
}

function rgba(c: Rgba, a: number): string {
	return toCss({ ...c, a: Math.min(1, Math.max(0, a)) });
}

/** A dark rim drawn under colored strokes so they read on white and black. */
const RIM = 'rgba(0, 0, 0, 0.32)';

/* ------------------------------------------------------------------------ */
/* Drawing                                                                    */
/* ------------------------------------------------------------------------ */

/** The part of CanvasRenderingContext2D the effects use (a fake in tests). */
export interface Ctx {
	save(): void;
	restore(): void;
	beginPath(): void;
	closePath(): void;
	moveTo(x: number, y: number): void;
	lineTo(x: number, y: number): void;
	arc(x: number, y: number, r: number, a0: number, a1: number): void;
	arcTo(x1: number, y1: number, x2: number, y2: number, r: number): void;
	fill(): void;
	stroke(): void;
	fillRect(x: number, y: number, w: number, h: number): void;
	createRadialGradient(
		x0: number,
		y0: number,
		r0: number,
		x1: number,
		y1: number,
		r1: number
	): { addColorStop(offset: number, color: string): void };
	createLinearGradient(
		x0: number,
		y0: number,
		x1: number,
		y1: number
	): { addColorStop(offset: number, color: string): void };
	fillStyle: unknown;
	strokeStyle: unknown;
	lineWidth: number;
	lineCap: string;
	lineJoin: string;
	globalAlpha: number;
}

export interface Scene {
	cursor: CursorConfig;
	style: FxStyle;
	/** Page size in CSS px, and the monitor inside it. */
	width: number;
	height: number;
	rect: Rect;
	pointer: PointerState;
	buttons: MouseButtons;
	clicks: readonly ClickEvent[];
	wheels: readonly WheelPulse[];
	trail: readonly TrailPoint[];
}

function circle(ctx: Ctx, x: number, y: number, r: number): void {
	ctx.beginPath();
	ctx.arc(x, y, Math.max(0, r), 0, Math.PI * 2);
}

function roundedSquare(ctx: Ctx, x: number, y: number, half: number, r: number): void {
	const rr = Math.min(r, half);
	ctx.beginPath();
	ctx.moveTo(x - half + rr, y - half);
	ctx.arcTo(x + half, y - half, x + half, y + half, rr);
	ctx.arcTo(x + half, y + half, x - half, y + half, rr);
	ctx.arcTo(x - half, y + half, x - half, y - half, rr);
	ctx.arcTo(x - half, y - half, x + half, y - half, rr);
	ctx.closePath();
}

function chevron(
	ctx: Ctx,
	x: number,
	y: number,
	size: number,
	dir: 'up' | 'down' | 'left' | 'right'
): void {
	const h = size / 2;
	const d = size * 0.45;
	ctx.beginPath();
	switch (dir) {
		case 'up':
			ctx.moveTo(x - h, y + d / 2);
			ctx.lineTo(x, y - d / 2);
			ctx.lineTo(x + h, y + d / 2);
			break;
		case 'down':
			ctx.moveTo(x - h, y - d / 2);
			ctx.lineTo(x, y + d / 2);
			ctx.lineTo(x + h, y - d / 2);
			break;
		case 'left':
			ctx.moveTo(x + d / 2, y - h);
			ctx.lineTo(x - d / 2, y);
			ctx.lineTo(x + d / 2, y + h);
			break;
		case 'right':
			ctx.moveTo(x - d / 2, y - h);
			ctx.lineTo(x + d / 2, y);
			ctx.lineTo(x - d / 2, y + h);
			break;
	}
}

/** Strokes the current path twice: a dark rim, then the color. */
function rimmedStroke(ctx: Ctx, color: string, width: number, alpha: number): void {
	ctx.globalAlpha = alpha;
	ctx.strokeStyle = RIM;
	ctx.lineWidth = width + 2;
	ctx.stroke();
	ctx.strokeStyle = color;
	ctx.lineWidth = width;
	ctx.stroke();
	ctx.globalAlpha = 1;
}

export function drawHighlight(ctx: Ctx, scene: Scene, now: number): void {
	const { style, cursor, pointer } = scene;
	if (!pointer.known) return;
	const p = toPage(scene.rect, pointer.x, pointer.y);
	const squeeze = pressAmount(scene.buttons, now);
	const color = rgba(style.color, 1);
	ctx.save();
	ctx.lineCap = 'round';
	ctx.lineJoin = 'round';
	switch (cursor.style) {
		case 'ring': {
			const r = style.radius * (1 - 0.14 * squeeze);
			circle(ctx, p.x, p.y, r);
			ctx.globalAlpha = style.opacity;
			ctx.fillStyle = rgba(style.color, 0.12 + 0.16 * squeeze);
			ctx.fill();
			rimmedStroke(ctx, color, style.line * (1 + 0.35 * squeeze), style.opacity);
			break;
		}
		case 'dot': {
			const r = style.radius * 0.42 * (1 + 0.3 * squeeze);
			circle(ctx, p.x, p.y, r + 1);
			ctx.globalAlpha = style.opacity;
			ctx.fillStyle = RIM;
			ctx.fill();
			circle(ctx, p.x, p.y, r);
			ctx.fillStyle = color;
			ctx.fill();
			break;
		}
		case 'spotlight': {
			const r = style.spot * (1 - 0.06 * squeeze);
			const soft = r * 0.22;
			const g = ctx.createRadialGradient(p.x, p.y, Math.max(0, r - soft), p.x, p.y, r + soft);
			const dark = Math.min(0.85, style.opacity * 0.62);
			g.addColorStop(0, 'rgba(0, 0, 0, 0)');
			g.addColorStop(1, `rgba(0, 0, 0, ${dark.toFixed(3)})`);
			ctx.globalAlpha = 1;
			ctx.fillStyle = g;
			ctx.fillRect(0, 0, scene.width, scene.height);
			circle(ctx, p.x, p.y, r);
			ctx.globalAlpha = style.opacity * 0.55;
			ctx.strokeStyle = color;
			ctx.lineWidth = Math.max(1.5, style.line * 0.5 * (1 + squeeze));
			ctx.stroke();
			break;
		}
	}
	ctx.restore();
}

export function drawRipple(ctx: Ctx, scene: Scene, click: ClickEvent, now: number): void {
	const t = progress(click.at, now, RIPPLE_MS);
	if (t === null) return;
	const { style } = scene;
	const p = toPage(scene.rect, click.x, click.y);
	const e = easeOutCubic(t);
	// Strong for most of the ripple, gone at the end.
	const fade = 1 - t * t;
	const color = rgba(style.color, 1);
	const base = style.radius;
	ctx.save();
	ctx.lineCap = 'round';
	ctx.lineJoin = 'round';
	switch (rippleShapeFor(click.button)) {
		case 'ring': {
			circle(ctx, p.x, p.y, base * (0.5 + 1.5 * e));
			rimmedStroke(ctx, color, style.line * (1.7 - 0.9 * t), fade);
			break;
		}
		case 'square': {
			for (const lag of [0, 0.22]) {
				const tt = Math.max(0, (t - lag) / (1 - lag));
				const half = base * (0.45 + 1.3 * easeOutCubic(tt));
				roundedSquare(ctx, p.x, p.y, half, half * 0.3);
				rimmedStroke(ctx, color, style.line * (1.6 - 0.8 * tt), fade * (lag > 0 ? 0.8 : 1));
			}
			break;
		}
		case 'pulse': {
			const r = base * (0.4 + 1.4 * e);
			circle(ctx, p.x, p.y, r);
			ctx.globalAlpha = 0.45 * fade;
			ctx.fillStyle = color;
			ctx.fill();
			circle(ctx, p.x, p.y, r);
			rimmedStroke(ctx, color, style.line * 0.8, 0.85 * fade);
			const dotFade = t < 0.7 ? 1 : 1 - (t - 0.7) / 0.3;
			const dot = base * 0.3;
			circle(ctx, p.x, p.y, dot + 1.5);
			ctx.globalAlpha = dotFade;
			ctx.fillStyle = RIM;
			ctx.fill();
			circle(ctx, p.x, p.y, dot);
			ctx.fillStyle = color;
			ctx.fill();
			break;
		}
		case 'back':
		case 'forward': {
			const dir = click.button === 'x1' ? -1 : 1;
			const size = base * 1.1;
			for (let i = 0; i < 2; i++) {
				const x = p.x + dir * (base * 0.6 + base * 1.4 * e + i * size * 0.5);
				chevron(ctx, x, p.y, size, dir < 0 ? 'left' : 'right');
				rimmedStroke(ctx, color, style.line * 1.5, fade * (i === 0 ? 1 : 0.75));
			}
			break;
		}
	}
	ctx.restore();
}

export function drawWheel(ctx: Ctx, scene: Scene, pulse: WheelPulse, now: number): void {
	const t = progress(pulse.at, now, WHEEL_MS);
	if (t === null) return;
	const { style, pointer } = scene;
	// The indicator rides with the pointer; before any move, it stays where the wheel turned.
	const p = pointer.known
		? toPage(scene.rect, pointer.x, pointer.y)
		: toPage(scene.rect, pulse.x, pulse.y);
	const e = easeOutCubic(t);
	const fade = t < 0.3 ? 1 : 1 - (t - 0.3) / 0.7;
	const color = rgba(style.color, 1);
	const size = style.radius * 1.05;
	const count = Math.max(1, Math.min(3, Math.round(pulse.notches)));
	const start = style.radius * 1.3 + size * 0.3;
	const travel = style.radius * 0.6 * e;
	ctx.save();
	ctx.lineCap = 'round';
	ctx.lineJoin = 'round';
	for (let i = 0; i < count; i++) {
		const d = start + travel + i * size * 0.5;
		let x = p.x;
		let y = p.y;
		if (pulse.direction === 'up') y -= d;
		else if (pulse.direction === 'down') y += d;
		else if (pulse.direction === 'left') x -= d;
		else x += d;
		chevron(ctx, x, y, size, pulse.direction);
		rimmedStroke(ctx, color, style.line * 1.4, fade * (1 - i * 0.2));
	}
	ctx.restore();
}

/**
 * The drag trail: each stroke is drawn as ONE path (no beads where segments
 * overlap), its alpha running from the tail to the head with a gradient along
 * the stroke, so the older end fades out first.
 */
export function drawTrail(ctx: Ctx, scene: Scene, now: number): void {
	const pts = scene.trail;
	if (pts.length < 2) return;
	const { style } = scene;
	const life = (p: TrailPoint) => Math.max(0, Math.min(1, 1 - (now - p.at) / TRAIL_MS));
	ctx.save();
	ctx.lineCap = 'round';
	ctx.lineJoin = 'round';
	let start = 0;
	while (start < pts.length) {
		let end = start;
		while (
			end + 1 < pts.length &&
			(pts[end + 1] as TrailPoint).stroke === (pts[start] as TrailPoint).stroke
		)
			end += 1;
		if (end > start) {
			const tail = pts[start] as TrailPoint;
			const head = pts[end] as TrailPoint;
			const headLife = life(head);
			if (headLife > 0) {
				const a = toPage(scene.rect, tail.x, tail.y);
				const b = toPage(scene.rect, head.x, head.y);
				const fadeIn = (c: Rgba, alpha: number) => {
					if (Math.hypot(b.x - a.x, b.y - a.y) < 1) return rgba(c, alpha * headLife);
					const g = ctx.createLinearGradient(a.x, a.y, b.x, b.y);
					g.addColorStop(0, rgba(c, alpha * life(tail)));
					g.addColorStop(1, rgba(c, alpha * headLife));
					return g;
				};
				ctx.beginPath();
				for (let i = start; i <= end; i++) {
					const p = pts[i] as TrailPoint;
					const q = toPage(scene.rect, p.x, p.y);
					if (i === start) ctx.moveTo(q.x, q.y);
					else ctx.lineTo(q.x, q.y);
				}
				ctx.globalAlpha = 1;
				ctx.strokeStyle = fadeIn({ r: 0, g: 0, b: 0, a: 1 }, 0.3 * style.opacity);
				ctx.lineWidth = style.line * 1.4 + 2;
				ctx.stroke();
				ctx.strokeStyle = fadeIn(style.color, 0.95 * style.opacity);
				ctx.lineWidth = style.line * 1.4;
				ctx.stroke();
			}
		}
		start = end + 1;
	}
	ctx.restore();
}

/**
 * Draws one frame (the caller clears the canvas first). Returns true while
 * anything is still moving, so the caller asks for another frame; false
 * means the picture is final until the next input.
 */
export function drawScene(ctx: Ctx, scene: Scene, now: number): boolean {
	const { cursor } = scene;
	let moving = false;
	if (cursor.trail && scene.trail.length > 0) {
		drawTrail(ctx, scene, now);
		moving = true;
	}
	drawHighlight(ctx, scene, now);
	if (scene.pointer.known && !pressSettled(scene.buttons, now)) moving = true;
	if (cursor.ripples) {
		for (const c of scene.clicks) {
			if (progress(c.at, now, RIPPLE_MS) === null) continue;
			drawRipple(ctx, scene, c, now);
			moving = true;
		}
	}
	if (cursor.wheel) {
		for (const w of scene.wheels) {
			if (progress(w.at, now, WHEEL_MS) === null) continue;
			drawWheel(ctx, scene, w, now);
			moving = true;
		}
	}
	return moving;
}
