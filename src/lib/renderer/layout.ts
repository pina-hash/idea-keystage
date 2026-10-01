/**
 * Placement of a piece (callouts, keyboard, mouse diagram) inside the page.
 *
 * Offsets are distances INWARD from the anchored edge: on a bottom anchor
 * `offset_y` lifts the piece off the bottom, on a right anchor `offset_x`
 * pulls it in from the right. On the centered axis an offset shifts the
 * piece right (x) or down (y). Offsets are in px at scale 1 and grow with
 * `scale`, like everything else (1 = sized for 1080p).
 *
 * Pieces are placed independently, except that the callouts move inward,
 * clear of the keyboard and the mouse diagram, when those sit in the row the
 * stack grows from (`calloutsOffsetY`).
 */
import type { Anchor, OverlayConfig } from '../protocol';
import { CALLOUT_GEOMETRY, CALLOUT_UNIT_PX, calloutHeightU } from './metrics';

export type HAlign = 'start' | 'center' | 'end';
export type VAlign = 'start' | 'center' | 'end';

export function horizontalOf(anchor: Anchor): HAlign {
	if (anchor.endsWith('left')) return 'start';
	if (anchor.endsWith('right')) return 'end';
	return 'center';
}

export function verticalOf(anchor: Anchor): VAlign {
	if (anchor.startsWith('top')) return 'start';
	if (anchor.startsWith('bottom')) return 'end';
	return 'center';
}

function px(n: number): string {
	const r = Math.round(n * 100) / 100;
	return `${Object.is(r, -0) ? 0 : r}px`;
}

/** Inline style that places an absolutely positioned box at an anchor. */
export function anchorStyle(
	anchor: Anchor,
	offsetX: number,
	offsetY: number,
	scale: number
): string {
	const s = Number.isFinite(scale) && scale > 0 ? scale : 1;
	const x = (Number.isFinite(offsetX) ? offsetX : 0) * s;
	const y = (Number.isFinite(offsetY) ? offsetY : 0) * s;
	const h = horizontalOf(anchor);
	const v = verticalOf(anchor);
	const parts: string[] = [];
	let tx = '0px';
	let ty = '0px';
	if (h === 'start') parts.push(`left: ${px(x)}`);
	else if (h === 'end') parts.push(`right: ${px(x)}`);
	else {
		parts.push('left: 50%');
		tx = `calc(-50% + ${px(x)})`;
	}
	if (v === 'start') parts.push(`top: ${px(y)}`);
	else if (v === 'end') parts.push(`bottom: ${px(y)}`);
	else {
		parts.push('top: 50%');
		ty = `calc(-50% + ${px(y)})`;
	}
	if (tx !== '0px' || ty !== '0px') parts.push(`transform: translate(${tx}, ${ty})`);
	return parts.join('; ');
}

/** Flex alignment of stacked items for an anchor (left anchors align left). */
export function flexAlign(anchor: Anchor): 'flex-start' | 'center' | 'flex-end' {
	const h = horizontalOf(anchor);
	return h === 'start' ? 'flex-start' : h === 'end' ? 'flex-end' : 'center';
}

/* ------------------------------------------------------------------------ */
/* Keeping the callouts clear of the other pieces                            */
/* ------------------------------------------------------------------------ */

/** A size in px. */
export interface Size {
	width: number;
	height: number;
}

/** A box in stage px. */
export interface Box {
	left: number;
	top: number;
	right: number;
	bottom: number;
}

function finiteOr0(n: number): number {
	return Number.isFinite(n) ? n : 0;
}

function scaleOr1(scale: number): number {
	return Number.isFinite(scale) && scale > 0 ? scale : 1;
}

/**
 * Where a box of `size` lands when `anchorStyle(anchor, offsetX, offsetY,
 * scale)` places it in a stage of `stage` size (the same arithmetic).
 */
export function placedBox(
	anchor: Anchor,
	offsetX: number,
	offsetY: number,
	scale: number,
	size: Size,
	stage: Size
): Box {
	const s = scaleOr1(scale);
	const x = finiteOr0(offsetX) * s;
	const y = finiteOr0(offsetY) * s;
	const w = Math.max(0, finiteOr0(size.width));
	const h = Math.max(0, finiteOr0(size.height));
	const ha = horizontalOf(anchor);
	const va = verticalOf(anchor);
	const left =
		ha === 'start' ? x : ha === 'end' ? stage.width - x - w : stage.width / 2 - w / 2 + x;
	const top =
		va === 'start' ? y : va === 'end' ? stage.height - y - h : stage.height / 2 - h / 2 + y;
	return { left, top, right: left + w, bottom: top + h };
}

/** Room kept between the callouts and a piece they move clear of, px at scale 1. */
export const CLEARANCE_PX = 16;

/** What `avoidOffsetY` needs to know about the callout stack. */
export interface CalloutsFootprint {
	anchor: Anchor;
	/** Configured offsets, px at scale 1 (as anchorStyle takes them). */
	offsetX: number;
	offsetY: number;
	/** The widest the stack may grow, px (its CSS max-width). */
	width: number;
	/** The tallest single row at the anchored edge, px (newest callout or a status pill). */
	row: number;
}

/**
 * The callouts' `offset_y` (px at scale 1, for anchorStyle) that keeps the
 * stack clear of the other pieces on the page (`obstacles`, stage px).
 *
 * Every piece is placed at its own anchor, so with the defaults and the
 * keyboard on, the full keyboard at bottom-left runs under the callouts at
 * bottom center. When the callouts sit on the top or bottom edge and a piece
 * covers the row the stack grows from (within `clearancePx`), across the
 * widest the stack may grow, the stack moves inward past that piece. The
 * widest width, not the current one, decides, so the callouts do not jump as
 * their content changes. The result is never nearer the edge than the
 * configured offset and never puts the first row off the stage. Callouts on
 * the middle row (`left`, `center`, `right`) keep their place: there is no
 * edge to move away from. Pieces of zero size are ignored.
 */
export function avoidOffsetY(
	callouts: CalloutsFootprint,
	obstacles: readonly Box[],
	stage: Size,
	scale: number,
	clearancePx: number = CLEARANCE_PX
): number {
	const s = scaleOr1(scale);
	const configured = finiteOr0(callouts.offsetY);
	const va = verticalOf(callouts.anchor);
	if (va === 'center' || obstacles.length === 0) return configured;
	const span = placedBox(
		callouts.anchor,
		callouts.offsetX,
		0,
		s,
		{ width: callouts.width, height: 0 },
		stage
	);
	const row = Math.max(0, finiteOr0(callouts.row));
	const gap = Math.max(0, clearancePx) * s;
	const solid = obstacles.filter(
		(o) => o.right > o.left && o.bottom > o.top && o.right > span.left && o.left < span.right
	);
	// Distance of the stack's anchored edge from the stage edge, px.
	const start = configured * s;
	let inset = start;
	// A piece moved past stays behind (the inset only grows), so each pass
	// that moves clears at least one more piece and the loop ends.
	for (let pass = 0; pass <= solid.length; pass++) {
		let moved = false;
		for (const o of solid) {
			const rowTop = va === 'end' ? stage.height - inset - row : inset;
			const rowBottom = rowTop + row;
			if (o.bottom + gap <= rowTop || o.top - gap >= rowBottom) continue;
			const past = va === 'end' ? stage.height - o.top + gap : o.bottom + gap;
			if (past > inset) {
				inset = past;
				moved = true;
			}
		}
		if (!moved) break;
	}
	if (inset === start) return configured;
	const furthest = Math.max(start, stage.height - row);
	return Math.min(inset, furthest) / s;
}

/** The pieces drawn beside the callouts on a page, by drawn size; null when not drawn. */
export interface OtherPieces {
	keyboard: Size | null;
	mouse: Size | null;
}

/**
 * The composition Stage.svelte draws: the callouts' `offset_y` (px at scale
 * 1) once they are kept clear of the keyboard and the mouse diagram, each at
 * its configured anchor. `scale` is the stage's (config.scale times zoom).
 */
export function calloutsOffsetY(
	config: OverlayConfig,
	others: OtherPieces,
	stage: Size,
	scale: number
): number {
	const s = scaleOr1(scale);
	const unit = CALLOUT_UNIT_PX[config.callouts.size] * s;
	const obstacles: Box[] = [];
	const kb = config.keyboard;
	if (others.keyboard) {
		obstacles.push(placedBox(kb.anchor, kb.offset_x, kb.offset_y, s, others.keyboard, stage));
	}
	const m = config.mouse;
	if (others.mouse) {
		obstacles.push(placedBox(m.anchor, m.offset_x, m.offset_y, s, others.mouse, stage));
	}
	const g = CALLOUT_GEOMETRY;
	return avoidOffsetY(
		{
			anchor: config.callouts.anchor,
			offsetX: config.callouts.offset_x,
			offsetY: config.callouts.offset_y,
			width: g.maxWidthU * unit,
			row: Math.max(calloutHeightU(config.callouts.captions), g.indicatorU) * unit,
		},
		obstacles,
		stage,
		s
	);
}
