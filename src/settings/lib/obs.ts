/**
 * What the OBS page tells the user to type into OBS.
 *
 * Browser Source properties and their defaults come from the OBS knowledge
 * base (https://obsproject.com/kb/browser-source, read 2026-10-01) and the
 * obs-browser source pinned by OBS 32.2.2 (obs-browser-plugin.cpp at
 * 3f0a2cdf378939ebe3c6f9ab36d4ea100c25aac2): Width and Height 800 x 600 by
 * default (1 to 8192), "Shutdown source when not visible" and "Refresh
 * browser when scene becomes active" off by default, the default Custom CSS
 * below. The WebSocket server menu item and its defaults come from the
 * obs-websocket source (disabled by default, port 4455, authentication on).
 * Research notes: .work/research/obs.md, DECISIONS.md item 5.
 */
import { ROUTES, type OverlayConfig, type Route } from '../../lib/protocol';
import { obsSourceSize, type SourceSize } from '../../lib/renderer/sizes';
import type { Monitor, ServerUrls } from '../types';
import { FALLBACK_MONITOR } from './monitors';

export interface RouteInfo {
	route: Route;
	title: string;
	description: string;
}

export const ROUTE_INFO: Readonly<Record<Route, RouteInfo>> = {
	keys: {
		route: 'keys',
		title: 'Keystrokes',
		description: 'The keystroke callouts and the Paused and Hidden pills.',
	},
	keyboard: {
		route: 'keyboard',
		title: 'Keyboard',
		description: 'The keyboard visual, with the mouse beside it when that is on.',
	},
	mouse: {
		route: 'mouse',
		title: 'Mouse',
		description: 'The mouse diagram on its own.',
	},
	cursor: {
		route: 'cursor',
		title: 'Cursor',
		description: 'Cursor highlight, click ripples and the drag trail, sized to the whole monitor.',
	},
	all: {
		route: 'all',
		title: 'Everything',
		description: 'Every piece that is turned on, in one source the size of the monitor.',
	},
};

/** The OBS default Custom CSS, verbatim (obs-browser-plugin.cpp `default_css`). */
export const OBS_DEFAULT_CSS =
	'body { background-color: rgba(0, 0, 0, 0); margin: 0px auto; overflow: hidden; }';

export interface ObsField {
	field: string;
	value: string;
	note: string;
}

/** The Browser Source fields to set, in the order the OBS properties dialog lists them. */
export const OBS_FIELDS: readonly ObsField[] = [
	{ field: 'Local file', value: 'Off', note: 'Use the URL. A local file cannot reach the app.' },
	{ field: 'URL', value: 'The address on each card', note: 'Copy it with the Copy button.' },
	{ field: 'Width and Height', value: 'The size on each card', note: 'OBS starts at 800 x 600.' },
	{
		field: 'Use custom frame rate',
		value: 'Off',
		note: 'The source then follows the OBS frame rate.',
	},
	{ field: 'Control audio via OBS', value: 'Off', note: 'The sources make no sound.' },
	{
		field: 'Custom CSS',
		value: 'Leave the default',
		note: 'The overlay sets its own transparent page.',
	},
	{
		field: 'Shutdown source when not visible',
		value: 'Off',
		note: 'On, OBS unloads the page whenever the source is hidden.',
	},
	{
		field: 'Refresh browser when scene becomes active',
		value: 'Off',
		note: 'On, the page reloads at every scene switch.',
	},
];

export const OBS_WEBSOCKET_STEPS: readonly string[] = [
	'In OBS, open Tools > WebSocket Server Settings.',
	'Check Enable WebSocket server. It is off until you turn it on.',
	'Keep Enable Authentication checked, then click Show Connect Info and copy the server password.',
	'Paste the password below and click Save password. The port is 4455 unless you changed it.',
];

/** The URL of a route when the app has not reported one (same format as server_urls). */
export function routeUrl(port: number, route: Route): string {
	return `http://127.0.0.1:${port}/overlay/${route}`;
}

export interface SourceCard {
	route: Route;
	info: RouteInfo;
	url: string;
	size: SourceSize;
}

/**
 * One card per route: the URL the app reported (or the standard one for the
 * port in the settings) and the size computed from the settings being edited
 * and the chosen monitor (`obsSourceSize`, ARCHITECTURE.md section 6).
 */
export function sourceCards(
	urls: ServerUrls | null,
	port: number,
	config: OverlayConfig,
	monitor: Monitor | null
): SourceCard[] {
	const m = monitor ?? FALLBACK_MONITOR;
	const usePort = urls && urls.port === port;
	return ROUTES.map((route) => {
		const reported = usePort ? urls.routes.find((r) => r.route === route) : undefined;
		return {
			route,
			info: ROUTE_INFO[route],
			url: reported?.url ?? routeUrl(port, route),
			size: obsSourceSize(route, config, m),
		};
	});
}

/** A copyable block with every route's URL and size. */
export function sourcesSummary(cards: readonly SourceCard[]): string {
	return cards
		.map((c) => `${c.info.title}: ${c.url} (${c.size.width} x ${c.size.height})`)
		.join('\n');
}

export function isLoopbackHost(host: string): boolean {
	const h = host.trim().toLowerCase();
	return h === '127.0.0.1' || h === 'localhost' || h === '::1' || h === '[::1]';
}

/** A host field value the app accepts (core: not empty, no spaces, no slash). */
export function obsHostError(host: string): string | null {
	const h = host.trim();
	if (h.length === 0) return 'Type the computer OBS runs on, for example 127.0.0.1.';
	if (/[\s/]/.test(h)) return 'Type only the host name or address, without spaces or slashes.';
	return null;
}

export function portError(port: number): string | null {
	if (!Number.isInteger(port) || port < 1 || port > 65535) return 'Use a port from 1 to 65535.';
	return null;
}
