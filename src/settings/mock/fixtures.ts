/**
 * Data for the mock backend: a realistic machine (two monitors, a few saved
 * sessions, OBS on this computer) and the screenshot scenarios selected with
 * `?state=` on settings.html.
 *
 * Nothing here is SolidWorks data. Session callouts are generic shortcuts
 * with no caption; the sample Copy List is built from the data file at run
 * time (see sampleCopyList).
 */
import { DEFAULT_PORT } from '../../lib/protocol';
import { defaultSettings } from '../defaults';
import type { Monitor, ProfileData, SessionSummary, Settings, Status } from '../types';

export const SCENARIOS = [
	'normal',
	'paused',
	'obs-error',
	'port-busy',
	'first-run',
	'import-warnings',
	'empty',
] as const;
export type Scenario = (typeof SCENARIOS)[number];

export function scenarioFrom(search: string): Scenario {
	const v = new URLSearchParams(search).get('state');
	return (SCENARIOS as readonly string[]).includes(v ?? '') ? (v as Scenario) : 'normal';
}

export const MOCK_VERSION = '0.1.0';
export const MOCK_DOCS = 'C:\\Users\\Teacher\\Documents';
/** A stand-in for a DPAPI blob: base64 text, never a password. */
export const MOCK_DPAPI_BLOB = 'AQAAANCMnd8BFdERjHoAwE/Cl+sBAAAAkzI0dW5sb2NrZWQ=';

export function mockMonitors(scenario: Scenario): Monitor[] {
	if (scenario === 'empty') return [];
	return [
		{
			id: '\\\\.\\DISPLAY1',
			name: 'Generic PnP Monitor',
			x: 0,
			y: 0,
			width: 2560,
			height: 1440,
			scale: 1,
			primary: true,
		},
		{
			id: '\\\\.\\DISPLAY2',
			name: 'Built-in display',
			x: 2560,
			y: 360,
			width: 1920,
			height: 1200,
			scale: 1.25,
			primary: false,
		},
	];
}

export function mockSettings(scenario: Scenario): Settings {
	const s = defaultSettings();
	if (scenario === 'first-run') return s;
	s.first_run_done = true;
	if (scenario === 'empty') return s;
	s.host.monitor = '\\\\.\\DISPLAY1';
	s.overlay.keyboard.enabled = true;
	s.overlay.keyboard.layout = 'left';
	s.overlay.cursor.enabled = true;
	s.obs.enabled = true;
	s.obs.password_dpapi = MOCK_DPAPI_BLOB;
	return s;
}

const MINUTE = 60_000;

/** "now" for the fixtures, so the session list reads as recent. */
function isoMinutesAgo(now: number, minutes: number): string {
	return new Date(now - minutes * MINUTE).toISOString();
}

export interface MockSession {
	summary: SessionSummary;
	markers: { t_ms: number; name: string }[];
}

/** The session id format keystage-session uses: the UTC start, `2026-10-01T14-00-00Z`. */
export function sessionId(iso: string): string {
	return `${iso.slice(0, 19).replace(/:/g, '-')}Z`;
}

export function mockSessions(scenario: Scenario, now: number): MockSession[] {
	if (scenario === 'empty' || scenario === 'first-run') return [];
	// Class-hour starts on earlier days, in the viewer's local time.
	const make = (
		daysAgo: number,
		at: [hour: number, minute: number],
		durationMin: number,
		callouts: number,
		markers: [number, string][]
	): MockSession => {
		const start = new Date(now);
		start.setDate(start.getDate() - daysAgo);
		start.setHours(at[0], at[1], 0, 0);
		const started = start.toISOString();
		return {
			summary: {
				id: sessionId(started),
				started,
				duration_ms: durationMin * MINUTE + 17_000,
				callouts,
				markers: markers.length,
			},
			markers: markers.map(([min, name]) => ({ t_ms: Math.round(min * MINUTE), name })),
		};
	};
	return [
		make(1, [13, 40], 26, 214, [
			[0, 'Introduction'],
			[1.6, 'Base sketch'],
			[6.25, 'First feature'],
			[11.4, 'Patterning the holes'],
			[18.9, 'Details and fillets'],
			[24.2, 'Wrap up'],
		]),
		make(3, [10, 15], 41, 388, [
			[0, 'Introduction'],
			[2.1, 'Setting up the assembly'],
			[9.5, 'Mates'],
			[17.75, 'Exploded view'],
			[29.3, 'Drawing sheet'],
			[38.0, 'Wrap up'],
		]),
		make(6, [9, 5], 12, 97, []),
	];
}

/** The monotonic clock value the mock uses for a session's start. */
const MOCK_START_MS = 5000;

/**
 * A session log in the keystage.session/1 shape crates/keystage-session
 * writes (callouts are wire callout objects; marker and callout times are
 * on the monotonic clock that starts at `start_ms`).
 */
export function sessionJson(s: MockSession): Record<string, unknown> {
	const combos = [
		['Ctrl', 'S'],
		['Ctrl', 'Z'],
		['Ctrl', 'C'],
		['Ctrl', 'V'],
		['Ctrl', 'Y'],
	] as const;
	const step = Math.max(1, Math.floor(s.summary.duration_ms / Math.max(1, s.summary.callouts)));
	const callouts = Array.from({ length: s.summary.callouts }, (_, i) => {
		const keys = combos[i % combos.length]!;
		const t = MOCK_START_MS + i * step;
		return {
			id: i + 1,
			kind: 'combo',
			steps: [keys.map((k) => ({ key: k, label: k, kind: k === 'Ctrl' ? 'modifier' : 'key' }))],
			text: null,
			count: 1,
			caption: null,
			context: null,
			profile: null,
			t_ms: t,
			updated_ms: t,
		};
	});
	const startedUnix = new Date(s.summary.started).getTime();
	const endedUnix = startedUnix + s.summary.duration_ms;
	return {
		schema: 'keystage.session/1',
		id: s.summary.id,
		app: { name: 'IDEA Keystage', version: MOCK_VERSION },
		started: s.summary.started,
		started_unix_ms: startedUnix,
		ended: new Date(endedUnix).toISOString(),
		ended_unix_ms: endedUnix,
		start_ms: MOCK_START_MS,
		end_ms: MOCK_START_MS + s.summary.duration_ms,
		recording: { events: [], samples: [] },
		markers: s.markers.map((m) => ({ t_ms: MOCK_START_MS + m.t_ms, name: m.name })),
		callouts,
		truncated: false,
	};
}

export function mockStatus(scenario: Scenario, settings: Settings, now: number): Status {
	const recording = scenario === 'normal' || scenario === 'paused';
	const base: Status = {
		paused: scenario === 'paused',
		suppressed: null,
		profile:
			scenario === 'empty' || scenario === 'first-run'
				? null
				: { id: 'solidworks', name: 'SolidWorks' },
		solidworks_mode: settings.profiles.solidworks_mode,
		keyboard_visible: true,
		server: {
			port: settings.server.port,
			running: true,
			error: null,
			clients: scenario === 'empty' || scenario === 'first-run' ? 0 : 2,
		},
		obs: {
			enabled: settings.obs.enabled,
			state: settings.obs.enabled ? 'connected' : 'off',
			error: null,
			recording: recording ? { active: true, paused: false, timecode: '00:12:41.233' } : null,
		},
		session: recording
			? {
					id: 'live',
					active: true,
					started: isoMinutesAgo(now, 12.7),
					callouts: 58,
					markers: 2,
				}
			: null,
		hook: { installed: scenario !== 'paused', dropped: 0 },
		version: MOCK_VERSION,
	};
	if (scenario === 'obs-error') {
		base.obs = {
			enabled: true,
			state: 'auth_failed',
			error: 'OBS did not accept the password (close code 4009). Save the password again.',
			recording: null,
		};
	}
	if (scenario === 'port-busy') {
		base.server = {
			port: DEFAULT_PORT,
			running: false,
			error: `Port ${DEFAULT_PORT} is in use by another program. Choose another port.`,
			clients: 0,
		};
	}
	return base;
}

/**
 * A Copy List as the Keyboard tab's Copy List would paste it (tab separated,
 * Category, Command, Shortcut(s)), built from entries in the data file so no
 * command name is written here: some keep their documented keys (unchanged),
 * some are bound to other keys (added), one documented key is bound to a
 * different documented command (changed). With `withWarnings`, rows the
 * importer warns about are added: a key it does not know, a binding used
 * twice, and a lowercase letter.
 */
export function sampleCopyList(profile: ProfileData, withWarnings: boolean): string {
	const docs = profile.shortcuts.filter(
		(s) =>
			s.source !== 'user-import' &&
			s.keys.length === 2 &&
			s.keys[0] === 'Ctrl' &&
			/^[A-Z]$/.test(s.keys[1] ?? '') &&
			!s.context.trim() &&
			s.command.trim().length > 0
	);
	const rows: string[] = ['Category\tCommand\tShortcut(s)'];
	const used = new Set<string>();
	const pick = docs.filter((s) => {
		const k = s.keys.join('+');
		if (used.has(k)) return false;
		used.add(k);
		return true;
	});
	const [a, b, c, d, e] = pick;
	if (a) rows.push(`File\t${a.command}\t${a.keys.join('+')}`);
	if (b) rows.push(`Edit\t${b.command}\t${b.keys.join('+')}`);
	if (c)
		rows.push(`View\t${c.command}\tCtrl+Alt+${c.keys[1]}, Shift+F${(c.command.length % 9) + 2}`);
	if (d) rows.push(`Tools\t${d.command}\tAlt+Shift+${d.keys[1]}`);
	if (d && e) rows.push(`Edit\t${d.command}\t${e.keys.join('+')}`);
	if (withWarnings && a && b) {
		rows.push(`Tools\t${a.command}\tCtrl+Shift+Fn`);
		rows.push(`View\t${b.command}\tAlt+Shift+${(a.keys[1] ?? 'q').toLowerCase()}`);
		rows.push(`Edit\t${a.command}\tCtrl+Alt+${c?.keys[1] ?? 'Q'}`);
		rows.push('Window\t');
	}
	return rows.join('\r\n');
}
