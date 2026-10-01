/**
 * Page options read from the URL (ARCHITECTURE.md sections 5 and 6).
 *
 *   /overlay/<keys|keyboard|mouse|cursor|all>   or   overlay.html?route=<route>
 *   host=native|preview   Tauri events (Host A, settings preview)
 *   demo=1                built-in demo (also demo=solidworks, demo=general)
 *   status=1              show the connection card even inside OBS
 *   theme, scale, anchor, x, y, kb, stack, size   per-source config overrides
 *
 * Overrides apply on top of every config the app sends, so a source is fully
 * configured by its URL. `anchor`, `x` and `y` move the piece the route is
 * about: callouts on `keys` and `all`, the keyboard on `keyboard`, the mouse
 * diagram on `mouse`; `cursor` ignores them. `size` is the callout size on
 * `keys` and `all` (s, m, l, xl) and the keyboard size on `keyboard` (s, m, l).
 */
import {
	LIMITS,
	isAnchor,
	isCalloutSize,
	isKeyboardLayout,
	isKeyboardSize,
	isRoute,
	type Anchor,
	type CalloutSize,
	type ClientHost,
	type KeyboardLayout,
	type KeyboardSize,
	type OverlayConfig,
	type Route,
} from '../protocol';

export type TransportKind = 'tauri' | 'demo' | 'ws';
/**
 * tour: SolidWorks then general. solidworks: captions from the profile data.
 * general: shortcuts-only behavior. states: the Paused and Hidden pills.
 */
export type DemoScriptId = 'tour' | 'solidworks' | 'general' | 'states';
export const DEMO_SCRIPTS: readonly DemoScriptId[] = ['tour', 'solidworks', 'general', 'states'];

export interface ConfigOverrides {
	theme?: string;
	scale?: number;
	anchor?: Anchor;
	x?: number;
	y?: number;
	kb?: KeyboardLayout;
	stack?: number;
	size?: CalloutSize | KeyboardSize;
}

export interface PageOptions {
	route: Route;
	/** What the page tells the server it is. */
	host: ClientHost;
	transport: TransportKind;
	demo: DemoScriptId | null;
	/** status=1: show the connection card even inside OBS. */
	status: boolean;
	overrides: ConfigOverrides;
}

export interface LocationLike {
	pathname: string;
	search: string;
}

const THEME_PATTERN = /^[a-z0-9-]{1,32}$/;

function parseNumber(raw: string | null, lo: number, hi: number): number | undefined {
	if (raw === null || raw.trim() === '') return undefined;
	const n = Number(raw);
	if (!Number.isFinite(n)) return undefined;
	return Math.min(hi, Math.max(lo, n));
}

export function routeFromPath(pathname: string): Route | null {
	const m = /\/overlay\/([a-z]+)\/?$/.exec(pathname);
	const candidate = m ? m[1] : undefined;
	return isRoute(candidate) ? candidate : null;
}

export function parsePageOptions(loc: LocationLike, inObs: boolean): PageOptions {
	const q = new URLSearchParams(loc.search);
	const routeParam = q.get('route');
	const route: Route = routeFromPath(loc.pathname) ?? (isRoute(routeParam) ? routeParam : 'all');

	const hostParam = q.get('host');
	const demoParam = q.get('demo');
	let demo: DemoScriptId | null = null;
	if (demoParam === '1' || demoParam === 'true' || demoParam === '') demo = 'tour';
	else if (demoParam !== null && (DEMO_SCRIPTS as readonly string[]).includes(demoParam))
		demo = demoParam as DemoScriptId;

	let transport: TransportKind;
	let host: ClientHost;
	if (hostParam === 'native' || hostParam === 'preview') {
		transport = 'tauri';
		host = hostParam;
	} else {
		transport = demo ? 'demo' : 'ws';
		host = inObs ? 'obs' : 'browser';
	}
	// A Tauri page asked for a demo still plays the demo (settings preview).
	if (demo && transport === 'tauri') transport = 'demo';

	const statusParam = q.get('status');
	return {
		route,
		host,
		transport,
		demo,
		status: statusParam === '1' || statusParam === 'true',
		overrides: parseOverrides(q, route),
	};
}

export function parseOverrides(q: URLSearchParams, route: Route): ConfigOverrides {
	const o: ConfigOverrides = {};
	const theme = q.get('theme');
	if (theme !== null && THEME_PATTERN.test(theme)) o.theme = theme;
	const scale = parseNumber(q.get('scale'), LIMITS.scale.min, LIMITS.scale.max);
	if (scale !== undefined) o.scale = scale;
	const anchor = q.get('anchor');
	if (isAnchor(anchor)) o.anchor = anchor;
	const x = parseNumber(q.get('x'), LIMITS.offset.min, LIMITS.offset.max);
	if (x !== undefined) o.x = x;
	const y = parseNumber(q.get('y'), LIMITS.offset.min, LIMITS.offset.max);
	if (y !== undefined) o.y = y;
	const kb = q.get('kb');
	if (isKeyboardLayout(kb)) o.kb = kb;
	const stack = parseNumber(q.get('stack'), LIMITS.maxStack.min, LIMITS.maxStack.max);
	if (stack !== undefined) o.stack = Math.round(stack);
	const size = q.get('size');
	if (route === 'keyboard') {
		if (isKeyboardSize(size)) o.size = size;
	} else if (isCalloutSize(size)) {
		o.size = size;
	}
	return o;
}

/** Returns a new config with the URL overrides applied for this route. */
export function applyOverrides(
	config: OverlayConfig,
	o: ConfigOverrides,
	route: Route
): OverlayConfig {
	const out: OverlayConfig = {
		...config,
		callouts: { ...config.callouts },
		keyboard: { ...config.keyboard },
		mouse: { ...config.mouse },
		cursor: { ...config.cursor },
		indicators: { ...config.indicators },
	};
	if (o.theme !== undefined) out.theme = o.theme;
	if (o.scale !== undefined) out.scale = o.scale;
	if (o.kb !== undefined) out.keyboard.layout = o.kb;
	if (o.stack !== undefined) out.callouts.max_stack = o.stack;

	const placed =
		route === 'keyboard'
			? out.keyboard
			: route === 'mouse'
				? out.mouse
				: route === 'cursor'
					? null
					: out.callouts;
	if (placed) {
		if (o.anchor !== undefined) {
			placed.anchor = o.anchor;
			if (placed === out.callouts) out.callouts.direction = directionForAnchor(o.anchor);
		}
		if (o.x !== undefined) placed.offset_x = o.x;
		if (o.y !== undefined) placed.offset_y = o.y;
	}
	if (o.size !== undefined) {
		if (route === 'keyboard') {
			if (isKeyboardSize(o.size)) out.keyboard.size = o.size;
		} else if (isCalloutSize(o.size)) {
			out.callouts.size = o.size;
		}
	}
	return out;
}

/** The natural stack direction for an anchor: grow away from the edge. */
export function directionForAnchor(anchor: Anchor): 'up' | 'down' {
	return anchor.startsWith('top') ? 'down' : 'up';
}

export function isInObs(w: unknown): boolean {
	return typeof w === 'object' && w !== null && 'obsstudio' in w;
}
