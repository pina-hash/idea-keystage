/**
 * Anchor names in plain words, and how the stack direction follows the
 * anchor (a top anchor stacks downward, the renderer's `directionForAnchor`).
 */
import type { Anchor, StackDirection } from '../../lib/protocol';

export const ANCHOR_LABELS: Readonly<Record<Anchor, string>> = {
	'top-left': 'Top left',
	top: 'Top center',
	'top-right': 'Top right',
	left: 'Middle left',
	center: 'Center',
	right: 'Middle right',
	'bottom-left': 'Bottom left',
	bottom: 'Bottom center',
	'bottom-right': 'Bottom right',
};

/** The natural stack direction for an anchor: new callouts grow away from the edge. */
export function naturalDirection(anchor: Anchor): StackDirection {
	return anchor.startsWith('top') ? 'down' : 'up';
}

/** What the two offsets mean at this anchor, in plain words. */
export function offsetWords(anchor: Anchor): { x: string; y: string } {
	const h = anchor.endsWith('left')
		? 'From the left edge'
		: anchor.endsWith('right')
			? 'From the right edge'
			: 'Shift right';
	const v = anchor.startsWith('top')
		? 'From the top edge'
		: anchor.startsWith('bottom')
			? 'From the bottom edge'
			: 'Shift down';
	return { x: h, y: v };
}
