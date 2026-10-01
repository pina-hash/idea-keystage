/**
 * Builds a demo script for DemoTransport. Loaded lazily (only demo pages pay
 * for it), and the SolidWorks profile data is loaded lazily again, only by
 * the scripts that show SolidWorks captions.
 */
import {
	defaultOverlayConfig,
	APP_NAME,
	PROTOCOL_VERSION,
	type OverlayConfig,
	type Route,
} from '../protocol';
import type { DemoScriptId } from '../renderer/options';
import type { DemoScript } from '../renderer/transport';
import { asProfileData, type ProfileData } from './profile-data';
import { generalSegment, solidworksSegment, stateMsg, statesSegment } from './scripts';
import { Timeline } from './timeline';

export const DEMO_VERSION = 'demo';

async function loadSolidworks(): Promise<ProfileData> {
	const mod = await import('$data/profiles/solidworks.json');
	return asProfileData(mod.default);
}

/** The pause between the end of one loop and the start of the next. */
export const LOOP_GAP_MS = 1200;

export function buildDemoScript(
	id: DemoScriptId,
	route: Route,
	solidworks: ProfileData | null
): DemoScript {
	const pointer = route === 'cursor' || route === 'all';
	const t = new Timeline({ pointer });
	if ((id === 'solidworks' || id === 'tour') && !solidworks) {
		throw new Error('the SolidWorks demo needs the profile data');
	}
	if (id === 'solidworks' && solidworks) solidworksSegment(t, solidworks);
	if (id === 'tour' && solidworks) {
		solidworksSegment(t, solidworks);
		generalSegment(t);
	}
	if (id === 'general') generalSegment(t);
	if (id === 'states') statesSegment(t);
	t.emit(stateMsg({ profile: null }));

	const config = demoOverlayConfig(route);
	return {
		hello: {
			t: 'hello',
			v: PROTOCOL_VERSION,
			app: APP_NAME,
			version: DEMO_VERSION,
			config,
			state: { ...stateMsgState(), profile: null },
			legends: null,
		},
		steps: t.build(),
		durationMs: Math.round(t.t + LOOP_GAP_MS),
	};
}

/**
 * The config the demo's hello carries: the defaults with every piece on.
 *
 * On `all`, where the pieces share one page, the demo also sets a composition
 * in which nothing has to move out of the way at 1080p and 4K (scale 1 or
 * 2): a tenkeyless keyboard with the mouse at bottom-left, and the callouts at
 * bottom-right. The full keyboard with the mouse (1045 px at size m) reaches
 * past the middle of a 1920 px page, so with the callouts at bottom center
 * the stage has to lift them above it (layout.ts calloutsOffsetY). Every key
 * the demo presses is on the tenkeyless drawing (checked in demo.test.ts).
 * The dedicated routes keep the defaults, so a `keys` or `keyboard` source
 * shows the demo in the box its recommended size was computed for.
 */
export function demoOverlayConfig(route: Route): OverlayConfig {
	const config = defaultOverlayConfig();
	config.keyboard.enabled = true;
	config.cursor.enabled = true;
	if (route === 'all') {
		config.keyboard.layout = 'tkl';
		config.callouts.anchor = 'bottom-right';
		config.callouts.offset_x = 32;
	}
	return config;
}

function stateMsgState() {
	const m = stateMsg({});
	if (m.t !== 'state') throw new Error('unreachable');
	return m.state;
}

export async function loadDemoScript(id: DemoScriptId, route: Route): Promise<DemoScript> {
	const needsData = id === 'solidworks' || id === 'tour';
	const data = needsData ? await loadSolidworks() : null;
	return buildDemoScript(id, route, data);
}
