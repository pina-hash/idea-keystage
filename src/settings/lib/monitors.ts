/**
 * Monitors: which one the overlay uses, its size for the preview and for the
 * OBS source sizes, and the proportional map the Display page draws.
 */
import type { Monitor } from '../types';

/** Used for sizes when the app reports no monitors (a 1080p screen, the overlay's design size). */
export const FALLBACK_MONITOR: Readonly<Monitor> = {
	id: '',
	name: 'Primary monitor',
	x: 0,
	y: 0,
	width: 1920,
	height: 1080,
	scale: 1,
	primary: true,
};

/** The chosen monitor: the stored device name, else the primary, else the first. */
export function chosenMonitor(monitors: readonly Monitor[], id: string | null): Monitor | null {
	if (monitors.length === 0) return null;
	if (id) {
		const hit = monitors.find((m) => m.id === id);
		if (hit) return hit;
	}
	return monitors.find((m) => m.primary) ?? monitors[0] ?? null;
}

/** True when the stored device name is set but no longer connected. */
export function isMissing(monitors: readonly Monitor[], id: string | null): boolean {
	return id !== null && monitors.length > 0 && !monitors.some((m) => m.id === id);
}

/** CSS pixels of the overlay window on that monitor (physical size over the DPI scale). */
export function logicalSize(m: Pick<Monitor, 'width' | 'height' | 'scale'>): {
	width: number;
	height: number;
} {
	const s = Number.isFinite(m.scale) && m.scale > 0 ? m.scale : 1;
	return { width: Math.round(m.width / s), height: Math.round(m.height / s) };
}

/** "\\.\DISPLAY2" -> "Display 2"; anything else as given. */
export function displayLabel(m: Pick<Monitor, 'id'>, index: number): string {
	const n = /DISPLAY(\d+)$/i.exec(m.id);
	return `Display ${n ? n[1] : index + 1}`;
}

export function scaleLabel(scale: number): string {
	return `${Math.round((Number.isFinite(scale) ? scale : 1) * 100)}%`;
}

export interface MapBox {
	id: string;
	/** Percent of the map's width and height. */
	left: number;
	top: number;
	width: number;
	height: number;
}

export interface MonitorMap {
	boxes: MapBox[];
	/** Width over height of the whole arrangement. */
	aspect: number;
}

/**
 * Lays the monitors out the way Windows arranges them, scaled into one box:
 * every position and size is a percent of the union rectangle, so the map
 * keeps their real proportions at any size.
 */
export function monitorMap(monitors: readonly Monitor[], gapPercent = 0.6): MonitorMap {
	if (monitors.length === 0) return { boxes: [], aspect: 16 / 9 };
	const minX = Math.min(...monitors.map((m) => m.x));
	const minY = Math.min(...monitors.map((m) => m.y));
	const maxX = Math.max(...monitors.map((m) => m.x + m.width));
	const maxY = Math.max(...monitors.map((m) => m.y + m.height));
	const w = Math.max(1, maxX - minX);
	const h = Math.max(1, maxY - minY);
	const g = monitors.length > 1 ? gapPercent : 0;
	const boxes = monitors.map((m) => ({
		id: m.id,
		left: ((m.x - minX) / w) * 100 + g / 2,
		top: ((m.y - minY) / h) * 100 + g / 2,
		width: (m.width / w) * 100 - g,
		height: (m.height / h) * 100 - g,
	}));
	return { boxes, aspect: w / h };
}
