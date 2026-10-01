/**
 * Recommended OBS Browser Source sizes (ARCHITECTURE.md section 6):
 * `cursor` and `all` are the chosen monitor's pixel size; `keys`, `keyboard`
 * and `mouse` are their content box at the chosen scale, rounded up to even
 * numbers. The settings window shows these next to each source URL.
 */
import type { OverlayConfig, Route } from '../protocol';
import {
	CALLOUT_GEOMETRY,
	CALLOUT_UNIT_PX,
	KEYBOARD_FOOTPRINTS,
	KEYBOARD_MOUSE_W,
	KEYBOARD_UNIT_PX,
	MOUSE_DIAGRAM,
	calloutHeightU,
} from './metrics';

export interface MonitorSize {
	/** Physical pixels. */
	width: number;
	height: number;
}

export interface SourceSize {
	width: number;
	height: number;
}

/** OBS accepts 1 to 8192 for both fields. */
export const OBS_MAX = 8192;

export function evenCeil(n: number): number {
	const c = Math.ceil(n - 1e-9);
	const even = c % 2 === 0 ? c : c + 1;
	return Math.min(OBS_MAX, Math.max(2, even));
}

function scaleOf(config: OverlayConfig): number {
	return Number.isFinite(config.scale) && config.scale > 0 ? config.scale : 1;
}

/** The content box of the keystroke callouts, unrounded, in px. */
export function calloutsBox(config: OverlayConfig): SourceSize {
	const g = CALLOUT_GEOMETRY;
	const s = scaleOf(config);
	const u = CALLOUT_UNIT_PX[config.callouts.size] * s;
	const n = Math.max(1, Math.round(config.callouts.max_stack));
	const rows = n * calloutHeightU(config.callouts.captions) + (n - 1) * g.stackGapU;
	const width = (g.maxWidthU + 2 * g.bleedU) * u + Math.abs(config.callouts.offset_x) * s;
	const height =
		(rows + g.stackGapU + g.indicatorU + 2 * g.bleedU) * u + Math.abs(config.callouts.offset_y) * s;
	return { width, height };
}

/** The content box of the keyboard visual (and the mouse beside it), in px. */
export function keyboardBox(config: OverlayConfig): SourceSize {
	const s = scaleOf(config);
	const unit = KEYBOARD_UNIT_PX[config.keyboard.size] * s;
	const fp = KEYBOARD_FOOTPRINTS[config.keyboard.layout];
	const w = fp.w + (config.keyboard.mouse ? KEYBOARD_MOUSE_W : 0);
	return {
		width: w * unit + Math.abs(config.keyboard.offset_x) * s,
		height: fp.h * unit + Math.abs(config.keyboard.offset_y) * s,
	};
}

/** The content box of the standalone mouse diagram, in px. */
export function mouseBox(config: OverlayConfig): SourceSize {
	const s = scaleOf(config);
	const h = MOUSE_DIAGRAM.heightPx * s;
	const bleed = MOUSE_DIAGRAM.bleedPx * s;
	return {
		width: h * MOUSE_DIAGRAM.aspect + 2 * bleed + Math.abs(config.mouse.offset_x) * s,
		height: h + 2 * bleed + Math.abs(config.mouse.offset_y) * s,
	};
}

export function obsSourceSize(
	route: Route,
	config: OverlayConfig,
	monitor: MonitorSize
): SourceSize {
	switch (route) {
		case 'cursor':
		case 'all':
			return {
				width: Math.min(OBS_MAX, Math.max(1, Math.round(monitor.width))),
				height: Math.min(OBS_MAX, Math.max(1, Math.round(monitor.height))),
			};
		case 'keys': {
			const b = calloutsBox(config);
			return { width: evenCeil(b.width), height: evenCeil(b.height) };
		}
		case 'keyboard': {
			const b = keyboardBox(config);
			return { width: evenCeil(b.width), height: evenCeil(b.height) };
		}
		case 'mouse': {
			const b = mouseBox(config);
			return { width: evenCeil(b.width), height: evenCeil(b.height) };
		}
	}
}
