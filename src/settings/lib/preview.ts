/**
 * Geometry of the live preview's close-up: the screen drawn at its actual
 * pixel size (or smaller when the piece would not fit), cropped to the
 * corner or edge the piece is anchored to, so the callouts, keyboard or
 * mouse diagram appear at the size viewers will see them.
 */
import type { Anchor, OverlayConfig, Route } from '../../lib/protocol';
import { horizontalOf, verticalOf } from '../../lib/renderer/layout';
import { calloutsBox, keyboardBox, mouseBox } from '../../lib/renderer/sizes';

export interface Box {
	width: number;
	height: number;
}

export interface CloseUp {
	/** Multiplies the overlay scale: 1 is actual size. */
	zoom: number;
	/** Where the zoomed screen sits inside the frame, in px (may be negative). */
	left: number;
	top: number;
	stageWidth: number;
	stageHeight: number;
	anchor: Anchor;
	label: string;
}

/** The anchor the close-up follows for a route. */
export function focusAnchor(route: Route, config: OverlayConfig): Anchor {
	switch (route) {
		case 'keyboard':
			return config.keyboard.anchor;
		case 'mouse':
			return config.mouse.anchor;
		case 'cursor':
			return 'center';
		default:
			return config.callouts.anchor;
	}
}

/** Callouts the close-up makes room for; a taller stack runs off the top of the frame. */
export const CLOSE_UP_STACK = 2;

/**
 * The on-screen box of the piece a route focuses on, offsets included, or
 * null for the cursor. For callouts only the newest CLOSE_UP_STACK rows
 * count, so a long stack does not shrink the close-up.
 */
export function focusBox(route: Route, config: OverlayConfig): Box | null {
	switch (route) {
		case 'keyboard':
			return keyboardBox(config);
		case 'mouse':
			return mouseBox(config);
		case 'cursor':
			return null;
		default: {
			const stack = Math.min(CLOSE_UP_STACK, config.callouts.max_stack);
			return calloutsBox({ ...config, callouts: { ...config.callouts, max_stack: stack } });
		}
	}
}

export function closeUp(route: Route, config: OverlayConfig, screen: Box, frame: Box): CloseUp {
	const anchor = focusAnchor(route, config);
	const box = focusBox(route, config);
	let zoom = 1;
	if (box && box.width > 0 && box.height > 0) {
		zoom = Math.min(1, (frame.width - 12) / box.width, (frame.height - 12) / box.height);
	}
	zoom = Math.max(0.15, zoom);
	const stageWidth = screen.width * zoom;
	const stageHeight = screen.height * zoom;
	const place = (align: 'start' | 'center' | 'end', inner: number, outer: number): number => {
		if (inner <= outer) return (outer - inner) / 2;
		if (align === 'start') return 0;
		if (align === 'end') return outer - inner;
		return (outer - inner) / 2;
	};
	const left = place(horizontalOf(anchor), stageWidth, frame.width);
	const top = place(verticalOf(anchor), stageHeight, frame.height);
	const pct = Math.round(zoom * 100);
	return {
		zoom,
		left,
		top,
		stageWidth,
		stageHeight,
		anchor,
		label: pct >= 100 ? 'Actual size' : `${pct}% of actual size`,
	};
}
