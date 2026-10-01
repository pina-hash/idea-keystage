/**
 * Builds a demo script for DemoTransport. Loaded lazily (only demo pages pay
 * for it), and the SolidWorks profile data is loaded lazily again, only by
 * the scripts that show SolidWorks captions.
 */
import { defaultOverlayConfig, APP_NAME, PROTOCOL_VERSION, type Route } from '../protocol';
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

	const config = defaultOverlayConfig();
	// The all-in-one demo shows every piece.
	config.keyboard.enabled = true;
	config.cursor.enabled = true;
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
