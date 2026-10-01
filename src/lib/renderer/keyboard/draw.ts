/**
 * Turns a layout into drawing instructions for KeyboardVisual.svelte: SVG
 * path data in CSS px (the SVG's user units are px, so theme lengths such as
 * --ks-edge-w apply unchanged and nothing is scaled with a transform), legend
 * positions and sizes. Key edges land on whole pixels and hairlines on half
 * pixels, so the drawing is crisp at 1080p and at 4K.
 *
 * The keycap is the callout keycap (callouts.css .ks-cap) drawn in SVG: a
 * face lighter at the top, a hairline edge, a crisp top highlight, and the
 * key's front wall as a darker band at its foot with a light line above it.
 */
import type { KeyboardLayout } from '../../protocol';
import {
	contentOffset,
	housingOutline,
	insetOrtho,
	placePoly,
	polygonRects,
	roundedPath,
	uniform,
	HOUSING_RADIUS,
	type Pt,
} from './geometry';
import { footprintOf, type KeyDef, type KeyLegend, type LayoutDef } from './layouts';

/** Key geometry in key units. */
export const KEY = {
	/** Space between neighboring keys. */
	gap: 0.12,
	radius: 0.12,
	/** The front wall band at the foot of a raised key. */
	skirt: 0.1,
	/**
	 * A held key's face is inset on every side by the lit outline plus this
	 * gap: it has sunk into its socket, which shows as a dark ring between the
	 * outline and the face. The shape says "held" in every theme, even where
	 * the pressed face has the idle face's color (Carbon).
	 */
	pressGap: 0.06,
	/** Modifier corner marker leg and its distance from the face corner. */
	marker: 0.2,
	markerInset: 0.07,
	/** Lit outline width, and the soft halo width. */
	ring: 0.06,
	halo: 0.16,
} as const;

/** Legend sizes in key units. */
export const LEGEND = {
	char: 0.42,
	charMulti: 0.32,
	fn: 0.29,
	word: 0.21,
	glyph: 0.36,
	sub: 0.24,
	/** Word legends smaller than this (px) are not drawn: unreadable on video. */
	minWordPx: 6.5,
	/** Average advance of a glyph as a fraction of the font size. */
	advance: { sans: 0.54, mono: 0.62 },
	/** Room kept free on each side of a legend. */
	padU: 0.1,
} as const;

export interface LegendDraw {
	kind: KeyLegend['kind'];
	text: string;
	glyph: KeyLegend['glyph'] | null;
	/** Center of the legend, px. */
	x: number;
	y: number;
	/** Font size or glyph box, px. */
	size: number;
	/** The text uses the mono face (word legends). */
	mono: boolean;
}

export interface KeyDraw {
	def: KeyDef;
	/** Raised key outline (filled with the cap face, then the wall shade). */
	outer: string;
	/** The hairline edge, inset by half its width so it lies inside the key. */
	edge: string;
	/** The top surface (everything above the wall band). */
	face: string;
	/** The top surface of a held key: inset evenly, the wall showing all around. */
	faceDown: string;
	/** The top highlight, a stroked line 1px under the face's top edges. */
	hi: string;
	/** The light line above the wall band: the face moved down 1px. */
	lip: string;
	/** The lit outline (a thick stroke inside the key's edge). */
	ring: string;
	ringWidth: number;
	haloWidth: number;
	/** Modifier corner marker (a small triangle at the face's top-left), or ''. */
	marker: string;
	/** How far the legend moves when the key is held, px. */
	downDx: number;
	downDy: number;
	/** Face box used for legends, px. */
	faceBox: { x: number; y: number; w: number; h: number };
}

export interface KeyboardDrawing {
	layout: LayoutDef;
	/** Width and height of the keyboard (the footprint), px. */
	width: number;
	height: number;
	housing: string;
	/** The plate's inner highlight line, just inside its edge (stroked, fading down). */
	housingHi: string;
	/** The accent tab on the housing's top edge, and its two dots. */
	tab: string;
	dots: string;
	keys: KeyDraw[];
}

function snap(n: number): number {
	return Math.round(n);
}

function snapPoly(points: readonly Pt[]): Pt[] {
	return points.map(([x, y]) => [snap(x), snap(y)] as const);
}

function topHighlight(face: readonly Pt[], r: number, offset: number): string {
	const parts: string[] = [];
	for (let i = 0; i < face.length; i++) {
		const a = face[i] as Pt;
		const b = face[(i + 1) % face.length] as Pt;
		if (a[1] !== b[1] || b[0] <= a[0]) continue; // top-facing edges run left to right
		const x0 = a[0] + r;
		const x1 = b[0] - r;
		if (x1 - x0 < 1) continue;
		const y = a[1] + offset;
		parts.push(`M${x0} ${y}H${x1}`);
	}
	return parts.join('');
}

function bounds(points: readonly Pt[]): { x: number; y: number; w: number; h: number } {
	let x0 = Infinity;
	let y0 = Infinity;
	let x1 = -Infinity;
	let y1 = -Infinity;
	for (const [x, y] of points) {
		x0 = Math.min(x0, x);
		y0 = Math.min(y0, y);
		x1 = Math.max(x1, x);
		y1 = Math.max(y1, y);
	}
	return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
}

/**
 * Drawing instructions for one layout at one size. `edgeW` is the theme's
 * hairline width in px (--ks-edge-w).
 */
export function drawKeyboard(layout: LayoutDef, unitPx: number, edgeW = 1): KeyboardDrawing {
	const u = Number.isFinite(unitPx) && unitPx > 0 ? unitPx : 38;
	const e = Number.isFinite(edgeW) && edgeW > 0 ? edgeW : 1;
	const fp = footprintOf(layout.id as KeyboardLayout);
	const off = contentOffset(layout, fp);
	const ox = off.x * u;
	const oy = off.y * u;

	const r = Math.max(2, KEY.radius * u);
	const skirt = Math.max(2, Math.round(KEY.skirt * u));
	const ringWidth = Math.max(2, 2 * e, Math.round(KEY.ring * u));
	const haloWidth = Math.max(4, KEY.halo * u);
	const pressInset = ringWidth + Math.max(2, Math.round(KEY.pressGap * u));

	const keys: KeyDraw[] = layout.keys.map((def) => {
		const cellPx = placePoly(insetOrtho(def.outline, uniform(KEY.gap / 2)), u, ox, oy);
		const outerPts = snapPoly(cellPx);
		const edgePts = insetOrtho(outerPts, uniform(e / 2));
		const facePts = insetOrtho(outerPts, { top: 0, right: 0, bottom: skirt, left: 0 });
		const ringPts = insetOrtho(outerPts, uniform(ringWidth / 2));
		const downPts = insetOrtho(outerPts, uniform(pressInset));
		const lipPts = facePts.map(([x, y]) => [x, y + 1] as const);
		// The legend sits in the face's first band (the upper part of an ISO Enter).
		const firstBand = polygonRects(facePts)[0] ?? bounds(facePts);
		const downBand = polygonRects(downPts)[0] ?? bounds(downPts);
		let marker = '';
		if (def.mod) {
			const m = Math.max(5, KEY.marker * u);
			const inset = Math.max(e + 1.5, KEY.markerInset * u);
			const fx = firstBand.x + inset;
			const fy = firstBand.y + inset;
			marker = `M${fx} ${fy}H${fx + m}L${fx} ${fy + m}Z`;
		}
		return {
			def,
			outer: roundedPath(outerPts, r),
			edge: roundedPath(edgePts, Math.max(0, r - e / 2)),
			face: roundedPath(facePts, r),
			faceDown: roundedPath(downPts, Math.max(1, r - pressInset / 2)),
			hi: topHighlight(facePts, r, e + 0.5),
			lip: roundedPath(lipPts, r),
			ring: roundedPath(ringPts, Math.max(0, r - ringWidth / 2)),
			ringWidth,
			haloWidth,
			marker,
			downDx: downBand.x + downBand.w / 2 - (firstBand.x + firstBand.w / 2),
			downDy: downBand.y + downBand.h / 2 - (firstBand.y + firstBand.h / 2),
			faceBox: firstBand,
		};
	});

	const housingPts = placePoly(housingOutline(layout), u, ox, oy).map(
		([x, y]) => [snap(x), snap(y)] as const
	);
	const hb = bounds(housingPts);
	const tabX = hb.x + Math.max(8, 0.42 * u);
	const tabW = Math.max(18, 1.1 * u);
	const tabH = Math.max(2, Math.round(0.075 * u));
	const cut = tabH;
	const tab = `M${tabX} ${hb.y}H${tabX + tabW}L${tabX + tabW - cut} ${hb.y + tabH}H${tabX + cut}Z`;
	const dotW = tabH;
	const d0 = tabX + tabW + Math.max(4, 0.16 * u);
	const d1 = d0 + dotW * 2;
	const dots = `M${d0} ${hb.y}h${dotW}v${tabH}h${-dotW}ZM${d1} ${hb.y}h${dotW}v${tabH}h${-dotW}Z`;

	return {
		layout,
		width: fp.w * u,
		height: fp.h * u,
		housing: roundedPath(housingPts, HOUSING_RADIUS * u),
		housingHi: roundedPath(
			insetOrtho(housingPts, uniform(e + 0.5)),
			Math.max(0, HOUSING_RADIUS * u - e - 0.5)
		),
		tab,
		dots,
		keys,
	};
}

/**
 * Where and how big a legend is drawn on a key, or null when nothing fits
 * (a word legend at a size too small to read, or an empty legend).
 */
export function legendDraw(key: KeyDraw, legend: KeyLegend, unitPx: number): LegendDraw | null {
	const u = unitPx;
	const box = key.faceBox;
	const x = box.x + box.w / 2;
	const y = box.y + box.h / 2;
	const room = Math.max(1, box.w - 2 * LEGEND.padU * u);
	if (legend.kind === 'none' || (legend.kind !== 'glyph' && legend.text.length === 0)) return null;
	if (legend.kind === 'glyph') {
		const size = Math.min(LEGEND.glyph * u, box.h * 0.82, room);
		return {
			kind: 'glyph',
			text: legend.text,
			glyph: legend.glyph ?? null,
			x,
			y,
			size,
			mono: false,
		};
	}
	const chars = [...legend.text].length;
	const mono = legend.kind === 'word';
	let size: number;
	if (legend.kind === 'char') size = (chars <= 1 ? LEGEND.char : LEGEND.charMulti) * u;
	else if (legend.kind === 'fn') size = LEGEND.fn * u;
	else size = LEGEND.word * u;
	size = Math.min(size, box.h * 0.7);
	const advance = mono ? LEGEND.advance.mono + 0.06 : LEGEND.advance.sans;
	const wide = chars * size * advance;
	if (wide > room) size *= room / wide;
	if (mono && size < LEGEND.minWordPx) return null;
	return { kind: legend.kind, text: legend.text, glyph: null, x, y, size, mono };
}

/** The small corner legend's position (top-right of the face). */
export function subLegendDraw(key: KeyDraw, text: string, unitPx: number): LegendDraw | null {
	if (text.length === 0) return null;
	const size = LEGEND.sub * unitPx;
	if (size < LEGEND.minWordPx) return null;
	const box = key.faceBox;
	return {
		kind: 'char',
		text,
		glyph: null,
		x: box.x + box.w - Math.max(3, 0.16 * unitPx),
		y: box.y + Math.max(3, 0.2 * unitPx),
		size,
		mono: false,
	};
}
