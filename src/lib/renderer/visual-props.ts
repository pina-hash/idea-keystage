/**
 * Props of the three visuals the renderer places but the visuals surface
 * draws (src/lib/renderer/keyboard/KeyboardVisual.svelte,
 * src/lib/renderer/MouseDiagram.svelte, src/lib/renderer/CursorFx.svelte).
 * Stage.svelte passes exactly these. Colors come from the theme tokens
 * (`--ks-*`, see src/lib/themes/themes.ts THEME_TOKENS), which are set on the
 * `.ks-stage` element every visual is mounted inside.
 */
import type { CursorConfig, KeyboardLayout, KeyboardSize, Legends } from '../protocol';
import type { KeyStates, MouseButtons, MouseState, WheelPulse } from './logic/inputs';

export interface KeyboardVisualProps {
	layout: KeyboardLayout;
	/** Live key states keyed by physical id ("sc:1E", "sc:E01D"). */
	keys: KeyStates;
	/** Legends for the active Windows layout, or null for the US defaults. */
	legends: Legends | null;
	size: KeyboardSize;
	/**
	 * Pixels per key unit (1u key pitch), already multiplied by the scale.
	 * The drawing (housing included, and the mouse when showMouse is on) must
	 * fit KEYBOARD_FOOTPRINTS[layout] (+ KEYBOARD_MOUSE_W) times this, see
	 * src/lib/renderer/metrics.ts.
	 */
	unitPx: number;
	/** How long a released key takes to fade from lit to idle. */
	decayMs: number;
	/** Draw the mouse diagram beside the keyboard. */
	showMouse: boolean;
	/** Mouse buttons and wheel for the diagram beside the keyboard. */
	buttons: MouseButtons;
	wheel: WheelPulse | null;
}

export interface MouseDiagramProps {
	buttons: MouseButtons;
	/** The latest wheel pulse; a new `seq` means a new notch to animate. */
	wheel: WheelPulse | null;
	/** Height of the drawing in px (width is height * MOUSE_DIAGRAM.aspect). */
	size: number;
}

export interface CursorFxProps {
	cursor: CursorConfig;
	/**
	 * Pointer position (normalized to the chosen monitor), buttons, recent
	 * clicks (for ripples, by `seq`) and recent wheel pulses.
	 */
	mouse: MouseState;
	/** Width over height of the chosen monitor (the page fills it). */
	aspect: number;
	/** config.scale times any preview zoom; sizes in config are at scale 1. */
	scale: number;
}
