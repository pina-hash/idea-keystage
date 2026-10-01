/**
 * Which pieces a page draws.
 *
 * A dedicated OBS route draws its piece whatever the `enabled` switches say
 * (a source is fully configured by its URL); runtime state still applies:
 * the keyboard hotkey hides the keyboard everywhere. The `all` route (Host A
 * and an all-in-one source) follows each piece's `enabled` switch. Host A
 * draws nothing while `native_visible` is false (OBS sources in use, or the
 * native overlay turned off), because it is never hidden at the OS level.
 */
import type { AppState, ClientHost, FontId, OverlayConfig, Route, Suppression } from '../protocol';

export interface Pieces {
	callouts: boolean;
	keyboard: boolean;
	mouse: boolean;
	cursor: boolean;
	/** Status pills (Paused, Hidden) next to the callouts. */
	indicators: boolean;
}

export const NO_PIECES: Pieces = {
	callouts: false,
	keyboard: false,
	mouse: false,
	cursor: false,
	indicators: false,
};

export function visiblePieces(
	route: Route,
	config: OverlayConfig,
	state: AppState,
	host: ClientHost
): Pieces {
	if (host === 'native' && !state.native_visible) return { ...NO_PIECES };
	const kbVisible = state.keyboard_visible;
	switch (route) {
		case 'keys':
			return { ...NO_PIECES, callouts: true, indicators: true };
		case 'keyboard':
			return { ...NO_PIECES, keyboard: kbVisible };
		case 'mouse':
			return { ...NO_PIECES, mouse: true };
		case 'cursor':
			return { ...NO_PIECES, cursor: true };
		case 'all':
			return {
				callouts: config.callouts.enabled,
				keyboard: config.keyboard.enabled && kbVisible,
				mouse: config.mouse.enabled,
				cursor: config.cursor.enabled,
				indicators: true,
			};
	}
}

export type IndicatorKind = 'paused' | 'hidden';

export interface IndicatorView {
	kind: IndicatorKind;
	/** Short reason under "Hidden"; never names an app. */
	detail: string | null;
}

const SUPPRESSION_WORDS: Readonly<Record<Suppression, string>> = {
	password: 'Password field',
	excluded: 'Excluded app',
};

/** The status pills to show, in order, or an empty list. */
export function indicatorsFor(config: OverlayConfig, state: AppState): IndicatorView[] {
	const out: IndicatorView[] = [];
	if (state.paused && config.indicators.paused) out.push({ kind: 'paused', detail: null });
	else if (state.suppressed && config.indicators.suppressed)
		out.push({ kind: 'hidden', detail: SUPPRESSION_WORDS[state.suppressed] });
	return out;
}

/** CSS font stacks for the keycap legends (`config.font`). */
export const FONT_STACKS: Readonly<Record<FontId, string>> = {
	rajdhani: "'Rajdhani', 'Segoe UI', sans-serif",
	orbitron: "'Orbitron', 'Rajdhani', sans-serif",
	'share-tech-mono': "'Share Tech Mono', 'Consolas', monospace",
};
