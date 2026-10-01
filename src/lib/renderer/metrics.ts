/**
 * Geometry shared by the CSS (through custom properties set in Stage.svelte)
 * and by obsSourceSize (sizes.ts), so a recommended OBS source size always
 * matches what is drawn. Lengths are at scale 1 (sized for 1080p).
 */
import type { CalloutSize, KeyboardLayout, KeyboardSize } from '../protocol';

/** Keycap height in px for each callout size. Everything else is in units of it. */
export const CALLOUT_UNIT_PX: Readonly<Record<CalloutSize, number>> = {
	s: 34,
	m: 46,
	l: 58,
	xl: 76,
};

/** Callout geometry in keycap units (u). */
export const CALLOUT_GEOMETRY = {
	/** Widest a callout plate may grow; longer content truncates. */
	maxWidthU: 18,
	padXU: 0.32,
	padYU: 0.24,
	capU: 1,
	captionGapU: 0.14,
	/** Caption font size and line box. */
	captionFontU: 0.44,
	captionLineU: 0.54,
	stackGapU: 0.22,
	/** Room kept around the stack for the glow and the drop shadow. */
	bleedU: 0.6,
	/** Height reserved for the Paused or Hidden indicator row. */
	indicatorU: 0.9,
} as const;

export function calloutHeightU(captions: boolean): number {
	const g = CALLOUT_GEOMETRY;
	const caption = captions ? g.captionGapU + g.captionLineU : 0;
	return g.padYU * 2 + g.capU + caption;
}

/** Key unit (one 1u key pitch) in px for each keyboard size. */
export const KEYBOARD_UNIT_PX: Readonly<Record<KeyboardSize, number>> = {
	s: 28,
	m: 38,
	l: 50,
};

/**
 * The box each keyboard layout is drawn in, in key units, housing included.
 * KeyboardVisual (visuals surface) must render inside footprint * unitPx.
 */
export const KEYBOARD_FOOTPRINTS: Readonly<Record<KeyboardLayout, { w: number; h: number }>> = {
	full: { w: 23.5, h: 7.5 },
	tkl: { w: 19.25, h: 7.5 },
	'75': { w: 17, h: 7.25 },
	'60': { w: 16, h: 6 },
	laptop: { w: 16, h: 7 },
	left: { w: 11.75, h: 7.5 },
};

/** Extra width (key units) when the mouse diagram sits beside the keyboard. */
export const KEYBOARD_MOUSE_W = 4;

/** The standalone mouse diagram (route `mouse`), px at scale 1. */
export const MOUSE_DIAGRAM = {
	heightPx: 160,
	/** Width over height of the drawing. */
	aspect: 0.64,
	bleedPx: 16,
} as const;
