/**
 * Placement of a piece (callouts, keyboard, mouse diagram) inside the page.
 *
 * Offsets are distances INWARD from the anchored edge: on a bottom anchor
 * `offset_y` lifts the piece off the bottom, on a right anchor `offset_x`
 * pulls it in from the right. On the centered axis an offset shifts the
 * piece right (x) or down (y). Offsets are in px at scale 1 and grow with
 * `scale`, like everything else (1 = sized for 1080p).
 */
import type { Anchor } from '../protocol';

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
