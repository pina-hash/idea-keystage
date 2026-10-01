/**
 * Overlay theme registry and the theme token contract.
 *
 * A theme is a block of `--ks-*` custom properties on `.ks-stage[data-theme]`
 * (src/lib/themes/<id>.css). Geometry is never in a theme: the renderer's
 * CSS draws every theme with one construction and only colors change, the
 * rule ported from IDEA Classroom's plate (one geometry, per-theme color
 * blocks). The keyboard visual, the mouse diagram and the cursor read the
 * same tokens.
 */
import type { CustomTheme, OverlayConfig, ThemeId } from '../protocol';
import {
	contrastRatio,
	ensureContrast,
	parseColor,
	shiftLightness,
	toCss,
	toHex,
	withAlpha,
	mix,
	type Rgba,
} from './color';

export interface ThemeInfo {
	id: ThemeId;
	name: string;
	description: string;
	/** True when the plate is dark (for the settings window's previews). */
	dark: boolean;
}

export const THEMES: readonly ThemeInfo[] = [
	{
		id: 'idea',
		name: 'IDEA',
		description: 'IDEA green on dark machined charcoal with a soft green glow.',
		dark: true,
	},
	{
		id: 'studio',
		name: 'Studio',
		description: 'Soft layered white and gray with green modifier keys.',
		dark: false,
	},
	{
		id: 'graphite',
		name: 'Graphite',
		description: 'Neutral dark slate with a silver accent.',
		dark: true,
	},
	{
		id: 'blueprint',
		name: 'Blueprint',
		description: 'Deep drafting blue with crisp white line work.',
		dark: true,
	},
	{
		id: 'carbon',
		name: 'Carbon',
		description: 'Black and white at maximum contrast for readability.',
		dark: true,
	},
	{
		id: 'glass',
		name: 'Glass',
		description: 'A translucent frosted panel with strong white type.',
		dark: true,
	},
	{
		id: 'custom',
		name: 'Custom',
		description: 'Your own accent, face, ink and glow colors.',
		dark: true,
	},
];

/** Themes that ship as a CSS file (every theme except custom). */
export const FILE_THEMES: readonly ThemeId[] = [
	'idea',
	'studio',
	'graphite',
	'blueprint',
	'carbon',
	'glass',
];

export function themeInfo(id: string): ThemeInfo {
	return THEMES.find((t) => t.id === id) ?? (THEMES[0] as ThemeInfo);
}

/**
 * Every token a theme declares. The order is the order of the theme files.
 * Values must be literal colors (hex or rgb/rgba) except the two lengths.
 */
export const THEME_TOKENS = [
	'--ks-edge-w',
	'--ks-plate-top',
	'--ks-plate-bot',
	'--ks-plate-edge',
	'--ks-plate-edge-top',
	'--ks-plate-hi',
	'--ks-plate-shade',
	'--ks-plate-shade-far',
	'--ks-plate-glow',
	'--ks-plate-blur',
	'--ks-tab',
	'--ks-cap-top',
	'--ks-cap-bot',
	'--ks-cap-ink',
	'--ks-cap-edge',
	'--ks-cap-hi',
	'--ks-cap-skirt',
	'--ks-cap-shade',
	'--ks-mod-top',
	'--ks-mod-bot',
	'--ks-mod-ink',
	'--ks-mod-edge',
	'--ks-mod-hi',
	'--ks-mod-skirt',
	'--ks-mod-lamp',
	'--ks-press-top',
	'--ks-press-bot',
	'--ks-press-ink',
	'--ks-press-edge',
	'--ks-press-glow',
	'--ks-ctrl',
	'--ks-shift',
	'--ks-alt',
	'--ks-win',
	'--ks-accent',
	'--ks-accent-ink',
	'--ks-glow',
	'--ks-caption-ink',
	'--ks-context-ink',
	'--ks-joiner-ink',
	'--ks-inset-top',
	'--ks-inset-bot',
	'--ks-inset-ink',
	'--ks-inset-edge',
	'--ks-inset-shade',
	'--ks-inset-lip',
	'--ks-badge-ink',
	'--ks-screen-top',
	'--ks-screen-bot',
	'--ks-screen-ink',
	'--ks-screen-glow',
	'--ks-screen-rim',
	'--ks-lamp-paused',
	'--ks-lamp-hidden',
	'--ks-lamp-live',
	'--ks-lamp-ring',
	'--ks-mouse-body',
	'--ks-mouse-line',
	'--ks-mouse-hit',
] as const;
export type ThemeToken = (typeof THEME_TOKENS)[number];
export type ThemeVars = Record<ThemeToken, string>;

/** Tokens that are lengths, not colors. */
export const LENGTH_TOKENS: readonly ThemeToken[] = ['--ks-edge-w', '--ks-plate-blur'];

/**
 * Text and the face it sits on. `layers` is the stack under the text, bottom
 * first, on top of the video behind the overlay: a translucent face is
 * measured composited over the plate and over each test background.
 */
export interface ContrastPair {
	label: string;
	ink: ThemeToken;
	layers: readonly ThemeToken[];
	/** WCAG minimum: 4.5 for text. */
	min: number;
}

export const CONTRAST_PAIRS: readonly ContrastPair[] = [
	{
		label: 'caption on plate (top)',
		ink: '--ks-caption-ink',
		layers: ['--ks-plate-top'],
		min: 4.5,
	},
	{
		label: 'caption on plate (foot)',
		ink: '--ks-caption-ink',
		layers: ['--ks-plate-bot'],
		min: 4.5,
	},
	{ label: 'context on plate', ink: '--ks-context-ink', layers: ['--ks-plate-bot'], min: 4.5 },
	{
		label: 'context on plate (top)',
		ink: '--ks-context-ink',
		layers: ['--ks-plate-top'],
		min: 4.5,
	},
	{ label: 'joiner on plate', ink: '--ks-joiner-ink', layers: ['--ks-plate-top'], min: 4.5 },
	{
		label: 'key legend (top)',
		ink: '--ks-cap-ink',
		layers: ['--ks-plate-top', '--ks-cap-top'],
		min: 4.5,
	},
	{
		label: 'key legend (foot)',
		ink: '--ks-cap-ink',
		layers: ['--ks-plate-bot', '--ks-cap-bot'],
		min: 4.5,
	},
	{
		label: 'modifier legend (top)',
		ink: '--ks-mod-ink',
		layers: ['--ks-plate-top', '--ks-mod-top'],
		min: 4.5,
	},
	{
		label: 'modifier legend (foot)',
		ink: '--ks-mod-ink',
		layers: ['--ks-plate-bot', '--ks-mod-bot'],
		min: 4.5,
	},
	{
		label: 'pressed legend (top)',
		ink: '--ks-press-ink',
		layers: ['--ks-plate-top', '--ks-press-top'],
		min: 4.5,
	},
	{
		label: 'pressed legend (foot)',
		ink: '--ks-press-ink',
		layers: ['--ks-plate-bot', '--ks-press-bot'],
		min: 4.5,
	},
	{
		label: 'status tag (top)',
		ink: '--ks-inset-ink',
		layers: ['--ks-plate-top', '--ks-inset-top'],
		min: 4.5,
	},
	{
		label: 'status tag (foot)',
		ink: '--ks-inset-ink',
		layers: ['--ks-plate-bot', '--ks-inset-bot'],
		min: 4.5,
	},
	{
		label: 'repeat badge',
		ink: '--ks-badge-ink',
		layers: ['--ks-plate-top', '--ks-inset-top'],
		min: 4.5,
	},
	{
		label: 'typed text (top)',
		ink: '--ks-screen-ink',
		layers: ['--ks-plate-top', '--ks-screen-top'],
		min: 4.5,
	},
	{
		label: 'typed text (foot)',
		ink: '--ks-screen-ink',
		layers: ['--ks-plate-bot', '--ks-screen-bot'],
		min: 4.5,
	},
	{ label: 'accent label', ink: '--ks-accent-ink', layers: ['--ks-accent'], min: 4.5 },
];

/** Backgrounds the overlay is measured over: a light gray CAD canvas, white, a dark one, black. */
export const TEST_BACKGROUNDS: readonly string[] = ['#ffffff', '#d3d6da', '#262a2e', '#000000'];

/* ------------------------------------------------------------------------ */
/* Custom theme                                                              */
/* ------------------------------------------------------------------------ */

export const DEFAULT_CUSTOM: CustomTheme = {
	accent: '#78b870',
	face: '#1c1f1d',
	ink: '#e7eae8',
	glow: '#78b870',
};

export interface CustomThemeResult {
	vars: ThemeVars;
	/** True when an ink had to be moved in lightness to stay readable. */
	adjusted: boolean;
	/** True when one of the four inputs was not a color and its default was used. */
	invalid: boolean;
}

const WHITE: Rgba = { r: 255, g: 255, b: 255, a: 1 };
const BLACK: Rgba = { r: 0, g: 0, b: 0, a: 1 };

function bestOn(ground: Rgba): Rgba {
	return contrastRatio(WHITE, ground) >= contrastRatio(BLACK, ground) ? WHITE : BLACK;
}

/**
 * Builds a full token set from four colors. Faces are derived from `face`,
 * the modifier key and the lamps from `accent`, the soft glow from `glow`.
 * Every ink is held to 4.5:1 against its faces by moving its lightness only;
 * `adjusted` tells the settings window to say so.
 */
export function customThemeVars(custom: CustomTheme | null): CustomThemeResult {
	const src = custom ?? DEFAULT_CUSTOM;
	let invalid = false;
	const pick = (v: string, fallback: string): Rgba => {
		const c = parseColor(v);
		if (c) return { ...c, a: 1 };
		invalid = true;
		return parseColor(fallback) as Rgba;
	};
	const accent = pick(src.accent, DEFAULT_CUSTOM.accent);
	const face = pick(src.face, DEFAULT_CUSTOM.face);
	const ink = pick(src.ink, DEFAULT_CUSTOM.ink);
	const glow = pick(src.glow, DEFAULT_CUSTOM.glow);

	const dark = contrastRatio(face, WHITE) > contrastRatio(face, BLACK);
	const lift = dark ? 1 : -1;
	const plateTop = shiftLightness(face, 0.025 * lift);
	const plateBot = face;
	const capTop = shiftLightness(face, 0.09 * lift);
	const capBot = shiftLightness(face, 0.06 * lift);
	const insetTop = shiftLightness(face, -0.035 * lift);
	const insetBot = shiftLightness(face, -0.02 * lift);
	const screenTop = dark ? shiftLightness(face, -0.04) : shiftLightness(face, -0.62);
	const screenBot = dark ? shiftLightness(face, -0.07) : shiftLightness(face, -0.66);
	const modTop = shiftLightness(accent, 0.03);
	const modBot = accent;
	const edge = mix(face, ink, 0.42);

	let adjusted = false;
	const hold = (c: Rgba, grounds: Rgba[]): Rgba => {
		const out = ensureContrast(c, grounds, 4.5);
		if (toHex(out) !== toHex(c)) adjusted = true;
		return out;
	};
	const caption = hold(ink, [plateTop, plateBot]);
	const capInk = hold(ink, [capTop, capBot, plateTop, plateBot]);
	const context = hold(mix(ink, face, 0.22), [plateTop, plateBot]);
	const joiner = hold(mix(ink, face, 0.3), [plateTop, plateBot]);
	const insetInk = hold(mix(ink, face, 0.15), [insetTop, insetBot]);
	const modInk = ensureContrast(bestOn(modBot), [modTop, modBot], 4.5);
	const screenInk = ensureContrast(
		dark ? accent : shiftLightness(accent, 0.25),
		[screenTop, screenBot],
		4.5
	);
	const badge = ensureContrast(accent, [insetTop, insetBot], 4.5);
	const pressTop = shiftLightness(accent, 0.06);
	const pressInk = ensureContrast(bestOn(accent), [pressTop, accent], 4.5);
	const accentInk = ensureContrast(bestOn(accent), [accent], 4.5);

	const css = (c: Rgba) => toCss(c);
	const vars: ThemeVars = {
		'--ks-edge-w': '1px',
		'--ks-plate-top': css(plateTop),
		'--ks-plate-bot': css(plateBot),
		'--ks-plate-edge': css(edge),
		'--ks-plate-edge-top': css(mix(edge, ink, 0.25)),
		'--ks-plate-hi': css(withAlpha(WHITE, dark ? 0.12 : 0.85)),
		'--ks-plate-shade': css(withAlpha(BLACK, dark ? 0.55 : 0.28)),
		'--ks-plate-shade-far': css(withAlpha(BLACK, dark ? 0.45 : 0.22)),
		'--ks-plate-glow': css(withAlpha(glow, 0.32)),
		'--ks-plate-blur': '0px',
		'--ks-tab': css(accent),
		'--ks-cap-top': css(capTop),
		'--ks-cap-bot': css(capBot),
		'--ks-cap-ink': css(capInk),
		'--ks-cap-edge': css(edge),
		'--ks-cap-hi': css(withAlpha(WHITE, dark ? 0.16 : 0.9)),
		'--ks-cap-skirt': css(withAlpha(BLACK, dark ? 0.5 : 0.3)),
		'--ks-cap-shade': css(withAlpha(BLACK, dark ? 0.55 : 0.25)),
		'--ks-mod-top': css(modTop),
		'--ks-mod-bot': css(modBot),
		'--ks-mod-ink': css(modInk),
		'--ks-mod-edge': css(shiftLightness(accent, dark ? 0.18 : -0.15)),
		'--ks-mod-hi': css(withAlpha(WHITE, 0.3)),
		'--ks-mod-skirt': css(withAlpha(BLACK, 0.35)),
		'--ks-mod-lamp': css(modInk),
		'--ks-press-top': css(pressTop),
		'--ks-press-bot': css(accent),
		'--ks-press-ink': css(pressInk),
		'--ks-press-edge': css(shiftLightness(accent, 0.2)),
		'--ks-press-glow': css(withAlpha(glow, 0.5)),
		'--ks-ctrl': css(accent),
		'--ks-shift': '#5abda8',
		'--ks-alt': '#c8a848',
		'--ks-win': '#a08ac7',
		'--ks-accent': css(accent),
		'--ks-accent-ink': css(accentInk),
		'--ks-glow': css(withAlpha(glow, 0.55)),
		'--ks-caption-ink': css(caption),
		'--ks-context-ink': css(context),
		'--ks-joiner-ink': css(joiner),
		'--ks-inset-top': css(insetTop),
		'--ks-inset-bot': css(insetBot),
		'--ks-inset-ink': css(insetInk),
		'--ks-inset-edge': css(withAlpha(dark ? WHITE : BLACK, 0.14)),
		'--ks-inset-shade': css(withAlpha(BLACK, dark ? 0.6 : 0.25)),
		'--ks-inset-lip': css(withAlpha(WHITE, dark ? 0.1 : 0.8)),
		'--ks-badge-ink': css(badge),
		'--ks-screen-top': css(screenTop),
		'--ks-screen-bot': css(screenBot),
		'--ks-screen-ink': css(screenInk),
		'--ks-screen-glow': css(withAlpha(glow, 0.55)),
		'--ks-screen-rim': css(withAlpha(accent, 0.2)),
		'--ks-lamp-paused': '#d08030',
		'--ks-lamp-hidden': '#c8a848',
		'--ks-lamp-live': css(accent),
		'--ks-lamp-ring': css(withAlpha(BLACK, 0.8)),
		'--ks-mouse-body': css(capBot),
		'--ks-mouse-line': css(capInk),
		'--ks-mouse-hit': css(accent),
	};
	return { vars, adjusted, invalid };
}

/* ------------------------------------------------------------------------ */
/* Resolution                                                                */
/* ------------------------------------------------------------------------ */

export interface ResolvedTheme {
	/** The data-theme attribute value. */
	id: ThemeId;
	/** Inline custom properties (custom theme only). */
	style: string;
}

export function styleFromVars(vars: Partial<Record<string, string>>): string {
	return Object.entries(vars)
		.filter((e): e is [string, string] => typeof e[1] === 'string')
		.map(([k, v]) => `${k}: ${v}`)
		.join('; ');
}

/** Unknown theme names fall back to IDEA, so a typo in a URL still renders. */
export function resolveTheme(config: Pick<OverlayConfig, 'theme' | 'custom'>): ResolvedTheme {
	const known = THEMES.find((t) => t.id === config.theme);
	if (!known) return { id: 'idea', style: '' };
	if (known.id === 'custom') {
		return { id: 'custom', style: styleFromVars(customThemeVars(config.custom).vars) };
	}
	return { id: known.id, style: '' };
}
