/**
 * Geometry for the keyboard visual: insetting and rounding orthogonal
 * polygons (key outlines, the ISO Enter, the left-hand zone's housing) and
 * placing a layout inside its footprint. Pure functions, tested in
 * geometry.test.ts.
 *
 * Polygons are lists of points, clockwise on screen (y grows downward),
 * with every edge horizontal or vertical and no two consecutive edges
 * collinear.
 */
import type { KeyboardLayout } from '../../protocol';
import type { LayoutDef } from './layouts';

export type Pt = readonly [number, number];

export interface Insets {
	top: number;
	right: number;
	bottom: number;
	left: number;
}

export function uniform(d: number): Insets {
	return { top: d, right: d, bottom: d, left: d };
}

/**
 * Moves every edge of an orthogonal polygon inward: top-facing edges (the
 * interior below them) by `top`, bottom-facing edges by `bottom`, and so on.
 * Negative amounts grow the polygon.
 */
export function insetOrtho(points: readonly Pt[], by: Insets): Pt[] {
	const n = points.length;
	if (n < 4) return points.map((p) => [p[0], p[1]] as const);
	// Shifted coordinate of edge i (from point i to point i + 1).
	const shifted: { horizontal: boolean; v: number }[] = [];
	for (let i = 0; i < n; i++) {
		const a = points[i] as Pt;
		const b = points[(i + 1) % n] as Pt;
		const dx = b[0] - a[0];
		const dy = b[1] - a[1];
		if (dy === 0) {
			// Horizontal: moving right means the interior is below (a top edge).
			const amount = dx > 0 ? by.top : by.bottom;
			shifted.push({ horizontal: true, v: a[1] + (dx > 0 ? amount : -amount) });
		} else {
			// Vertical: moving down means the interior is to the left (a right edge).
			const amount = dy > 0 ? by.right : by.left;
			shifted.push({ horizontal: false, v: a[0] + (dy > 0 ? -amount : amount) });
		}
	}
	const out: Pt[] = [];
	for (let i = 0; i < n; i++) {
		const prev = shifted[(i - 1 + n) % n] as { horizontal: boolean; v: number };
		const next = shifted[i] as { horizontal: boolean; v: number };
		const x = prev.horizontal ? next.v : prev.v;
		const y = prev.horizontal ? prev.v : next.v;
		out.push([x, y]);
	}
	return out;
}

function fmt(n: number): string {
	const r = Math.round(n * 100) / 100;
	return String(Object.is(r, -0) ? 0 : r);
}

/**
 * SVG path data for a polygon with every corner rounded by `r` (clamped to
 * half of each adjacent edge). Convex and concave corners both get a
 * circular arc.
 */
export function roundedPath(points: readonly Pt[], r: number): string {
	const n = points.length;
	if (n < 3) return '';
	const parts: string[] = [];
	for (let i = 0; i < n; i++) {
		const p = points[(i - 1 + n) % n] as Pt;
		const v = points[i] as Pt;
		const q = points[(i + 1) % n] as Pt;
		const inX = v[0] - p[0];
		const inY = v[1] - p[1];
		const outX = q[0] - v[0];
		const outY = q[1] - v[1];
		const inLen = Math.hypot(inX, inY);
		const outLen = Math.hypot(outX, outY);
		if (inLen === 0 || outLen === 0) continue;
		const rr = Math.max(0, Math.min(r, inLen / 2, outLen / 2));
		const ax = v[0] - (inX / inLen) * rr;
		const ay = v[1] - (inY / inLen) * rr;
		const bx = v[0] + (outX / outLen) * rr;
		const by = v[1] + (outY / outLen) * rr;
		const cross = inX * outY - inY * outX;
		parts.push(`${i === 0 ? 'M' : 'L'}${fmt(ax)} ${fmt(ay)}`);
		if (rr > 0) parts.push(`A${fmt(rr)} ${fmt(rr)} 0 0 ${cross > 0 ? 1 : 0} ${fmt(bx)} ${fmt(by)}`);
	}
	parts.push('Z');
	return parts.join('');
}

/** Scales and translates a polygon (key units to px). */
export function placePoly(points: readonly Pt[], unit: number, ox: number, oy: number): Pt[] {
	return points.map(([x, y]) => [ox + x * unit, oy + y * unit] as const);
}

/** Area of a polygon (shoelace), positive for clockwise on screen. */
export function polygonArea(points: readonly Pt[]): number {
	let s = 0;
	for (let i = 0; i < points.length; i++) {
		const a = points[i] as Pt;
		const b = points[(i + 1) % points.length] as Pt;
		s += a[0] * b[1] - b[0] * a[1];
	}
	return s / 2;
}

/** Axis-aligned rectangles that tile an orthogonal polygon's interior (used by tests). */
export function polygonRects(
	points: readonly Pt[]
): { x: number; y: number; w: number; h: number }[] {
	const ys = [...new Set(points.map((p) => p[1]))].sort((a, b) => a - b);
	const out: { x: number; y: number; w: number; h: number }[] = [];
	for (let i = 0; i + 1 < ys.length; i++) {
		const y0 = ys[i] as number;
		const y1 = ys[i + 1] as number;
		const mid = (y0 + y1) / 2;
		// Crossings of the horizontal line y = mid with vertical edges.
		const xs: number[] = [];
		for (let j = 0; j < points.length; j++) {
			const a = points[j] as Pt;
			const b = points[(j + 1) % points.length] as Pt;
			if (a[0] !== b[0]) continue;
			const lo = Math.min(a[1], b[1]);
			const hi = Math.max(a[1], b[1]);
			if (mid > lo && mid < hi) xs.push(a[0]);
		}
		xs.sort((a, b) => a - b);
		for (let k = 0; k + 1 < xs.length; k += 2) {
			const x0 = xs[k] as number;
			const x1 = xs[k + 1] as number;
			out.push({ x: x0, y: y0, w: x1 - x0, h: y1 - y0 });
		}
	}
	return out;
}

/* ------------------------------------------------------------------------ */
/* The housing                                                                */
/* ------------------------------------------------------------------------ */

/** Housing padding around the keys, and its corner radius, in key units. */
export const HOUSING_PAD = 0.36;
export const HOUSING_RADIUS = 0.3;
/** Rows whose right ends differ by less than this share one straight edge. */
const STEP_MIN = 1;

/** Layouts whose housing hugs the rows instead of being a rectangle. */
export const STEPPED_HOUSINGS: readonly KeyboardLayout[] = ['left'];

interface Band {
	top: number;
	bottom: number;
	right: number;
}

/**
 * The housing outline in key units (the key area's coordinates). A
 * rectangle around the keys, or for the left-hand zone a stepped outline
 * that follows the right ends of its rows, so the zone reads as a shape of
 * its own instead of a mostly empty box.
 */
export function housingOutline(layout: LayoutDef, pad = HOUSING_PAD): Pt[] {
	let minX = Infinity;
	let minY = Infinity;
	let maxX = -Infinity;
	let maxY = -Infinity;
	for (const k of layout.keys) {
		minX = Math.min(minX, k.x);
		minY = Math.min(minY, k.y);
		maxX = Math.max(maxX, k.x + k.w);
		maxY = Math.max(maxY, k.y + k.h);
	}
	if (!Number.isFinite(minX)) return [];
	const left = minX - pad;
	const rect: Pt[] = [
		[left, minY - pad],
		[maxX + pad, minY - pad],
		[maxX + pad, maxY + pad],
		[left, maxY + pad],
	];
	if (!STEPPED_HOUSINGS.includes(layout.id)) return rect;

	// Rows by their top edge, with their right end.
	const rows = new Map<number, { top: number; bottom: number; right: number }>();
	for (const k of layout.keys) {
		const r = rows.get(k.y);
		if (r) {
			r.bottom = Math.max(r.bottom, k.y + k.h);
			r.right = Math.max(r.right, k.x + k.w);
		} else rows.set(k.y, { top: k.y, bottom: k.y + k.h, right: k.x + k.w });
	}
	const sorted = [...rows.values()].sort((a, b) => a.top - b.top);
	// Merge neighbors whose right ends are close into bands.
	const bands: Band[] = [];
	for (const r of sorted) {
		const last = bands[bands.length - 1];
		if (last && Math.abs(last.right - r.right) < STEP_MIN) {
			last.bottom = Math.max(last.bottom, r.bottom);
			last.right = Math.max(last.right, r.right);
		} else bands.push({ ...r });
	}
	if (bands.length < 2) return rect;
	const pts: Pt[] = [[left, minY - pad]];
	const first = bands[0] as Band;
	pts.push([first.right + pad, minY - pad]);
	for (let i = 0; i + 1 < bands.length; i++) {
		const a = bands[i] as Band;
		const b = bands[i + 1] as Band;
		// The wider band keeps its padding past the step.
		const y = b.right > a.right ? b.top - pad : a.bottom + pad;
		pts.push([a.right + pad, y]);
		pts.push([b.right + pad, y]);
	}
	const lastBand = bands[bands.length - 1] as Band;
	pts.push([lastBand.right + pad, maxY + pad]);
	pts.push([left, maxY + pad]);
	return pts;
}

/** Where the key area sits inside the footprint, in key units (centered). */
export function contentOffset(
	layout: LayoutDef,
	footprint: { w: number; h: number }
): { x: number; y: number } {
	return { x: (footprint.w - layout.width) / 2, y: (footprint.h - layout.height) / 2 };
}
