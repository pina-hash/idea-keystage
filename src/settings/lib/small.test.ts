import { describe, expect, it } from 'vitest';
import { KEYBOARD_LAYOUTS, defaultOverlayConfig, type Anchor } from '../../lib/protocol';
import { defaultStatus } from '../defaults';
import type { Monitor, Status } from '../types';
import {
	elapsedSince,
	formatClock,
	formatDateTime,
	formatDuration,
	formatMs,
	formatPercent,
	formatScale,
	formatTime,
	formatTimecode,
	plural,
	safeFileName,
} from './format';
import {
	chosenMonitor,
	displayLabel,
	isMissing,
	logicalSize,
	monitorMap,
	scaleLabel,
	FALLBACK_MONITOR,
} from './monitors';
import {
	addExcludedApp,
	exeFileName,
	normalizeExcludedApps,
	removeExcludedApp,
	sameApp,
} from './excluded';
import { ANCHOR_LABELS, naturalDirection, offsetWords } from './anchors';
import { OUTPUT_INFO, WIZARD_STEPS, applyOutput, outputOf, stepIndex } from './wizard';
import { LAYOUT_INFO } from './layouts';
import { APPEARANCE_KEY, applyAppearance, readAppearance, saveAppearance } from './appearance';
import {
	PAGES,
	PAGE_INFO,
	activityWord,
	isPageId,
	obsWord,
	pageFromLocation,
	recordingText,
	serverWord,
} from './nav';

describe('format', () => {
	it('formats milliseconds', () => {
		expect(formatMs(450)).toBe('450 ms');
		expect(formatMs(2200)).toBe('2.2 s');
		expect(formatMs(1000)).toBe('1 s');
		expect(formatMs(15000)).toBe('15 s');
		expect(formatMs(12500)).toBe('12.5 s');
		expect(formatMs(1250)).toBe('1.25 s');
		expect(formatMs(Number.NaN)).toBe('');
	});

	it('formats clocks and durations', () => {
		expect(formatClock(0)).toBe('0:00');
		expect(formatClock(245_000)).toBe('4:05');
		expect(formatClock(3_723_000)).toBe('1:02:03');
		expect(formatDuration(45_000)).toBe('45 s');
		expect(formatDuration(26 * 60_000 + 17_000)).toBe('26 min');
		expect(formatDuration(64 * 60_000)).toBe('1 h 4 min');
		expect(formatDuration(120 * 60_000)).toBe('2 h');
	});

	it('trims OBS timecodes and leaves other text alone', () => {
		expect(formatTimecode('00:12:41.233')).toBe('00:12:41');
		expect(formatTimecode('1:02:03')).toBe('01:02:03');
		expect(formatTimecode(' live ')).toBe('live');
	});

	it('formats dates in local time and keeps unreadable ones', () => {
		const d = new Date(2026, 8, 30, 14, 5);
		expect(formatDateTime(d.toISOString())).toBe('Sep 30, 2026, 2:05 PM');
		expect(formatTime(new Date(2026, 0, 1, 0, 7).toISOString())).toBe('12:07 AM');
		expect(formatDateTime('soon')).toBe('soon');
		expect(elapsedSince(new Date(1000).toISOString(), 61_000)).toBe(60_000);
		expect(elapsedSince('soon', 5)).toBe(0);
	});

	it('pluralizes, formats percents, scales and file names', () => {
		expect(plural(1, 'marker')).toBe('1 marker');
		expect(plural(1200, 'callout')).toBe('1,200 callouts');
		expect(formatPercent(0.6)).toBe('60%');
		expect(formatScale(1.25)).toBe('1.25x');
		expect(safeFileName('a:b/c?.')).toBe('a-b-c-');
		expect(safeFileName('  ')).toBe('keystage');
	});
});

const MONITORS: Monitor[] = [
	{
		id: '\\\\.\\DISPLAY1',
		name: 'A',
		x: 0,
		y: 0,
		width: 2560,
		height: 1440,
		scale: 1,
		primary: true,
	},
	{
		id: '\\\\.\\DISPLAY2',
		name: 'B',
		x: 2560,
		y: 360,
		width: 1920,
		height: 1200,
		scale: 1.25,
		primary: false,
	},
];

describe('monitors', () => {
	it('chooses the stored monitor, else the primary', () => {
		expect(chosenMonitor(MONITORS, '\\\\.\\DISPLAY2')?.name).toBe('B');
		expect(chosenMonitor(MONITORS, null)?.name).toBe('A');
		expect(chosenMonitor(MONITORS, 'gone')?.name).toBe('A');
		expect(chosenMonitor([], null)).toBeNull();
		expect(isMissing(MONITORS, 'gone')).toBe(true);
		expect(isMissing(MONITORS, null)).toBe(false);
		expect(isMissing([], 'gone')).toBe(false);
	});

	it('gives the page size at the DPI scale and plain labels', () => {
		expect(logicalSize(MONITORS[1]!)).toEqual({ width: 1536, height: 960 });
		expect(logicalSize({ width: 100, height: 50, scale: 0 })).toEqual({ width: 100, height: 50 });
		expect(displayLabel(MONITORS[1]!, 0)).toBe('Display 2');
		expect(displayLabel({ id: 'other' }, 2)).toBe('Display 3');
		expect(scaleLabel(1.25)).toBe('125%');
		expect(FALLBACK_MONITOR.width).toBe(1920);
	});

	it('maps the arrangement in proportion', () => {
		const map = monitorMap(MONITORS, 0);
		expect(map.aspect).toBeCloseTo(4480 / 1560);
		const [a, b] = map.boxes;
		expect(a).toMatchObject({ left: 0, top: 0 });
		expect(a!.width).toBeCloseTo((2560 / 4480) * 100);
		expect(b!.left).toBeCloseTo((2560 / 4480) * 100);
		expect(b!.top).toBeCloseTo((360 / 1560) * 100);
		expect(monitorMap([]).boxes).toEqual([]);
		const single = monitorMap([MONITORS[0]!], 2);
		expect(single.boxes[0]).toMatchObject({ left: 0, top: 0, width: 100, height: 100 });
	});
});

describe('excluded apps', () => {
	it('keeps only the file name of a path', () => {
		expect(exeFileName('C:\\Tools\\Vault.exe')).toBe('Vault.exe');
		expect(exeFileName('"C:/x/y.exe"')).toBe('y.exe');
	});

	it('adds, explains and refuses entries the way core cleans them', () => {
		const r = addExcludedApp(['KeePass.exe'], 'C:\\Program Files\\Vault\\Vault.exe');
		expect(r).toEqual({
			ok: true,
			list: ['KeePass.exe', 'Vault.exe'],
			added: 'Vault.exe',
			note: 'Saved as Vault.exe: only the file name is used.',
		});
		expect(addExcludedApp(['KeePass.exe'], 'keepass')).toEqual({
			ok: false,
			error: 'keepass is already in the list.',
		});
		expect(addExcludedApp([], ' ').ok).toBe(false);
		expect(addExcludedApp([], 'bad|name').ok).toBe(false);
		expect(addExcludedApp([], 'C:\\dir\\').ok).toBe(false);
		expect(sameApp('Secrets', 'secrets.EXE')).toBe(true);
		expect(removeExcludedApp(['a', 'b'], 'a')).toEqual(['b']);
		expect(
			normalizeExcludedApps([' KeePass.exe ', 'keepass.EXE', '', 'C:\\Tools\\Vault.exe', 'Secrets'])
		).toEqual(['KeePass.exe', 'Vault.exe', 'Secrets']);
	});
});

describe('anchors', () => {
	it('names every anchor and picks the natural stack direction', () => {
		const anchors: Anchor[] = [
			'top-left',
			'top',
			'top-right',
			'left',
			'center',
			'right',
			'bottom-left',
			'bottom',
			'bottom-right',
		];
		for (const a of anchors) expect(ANCHOR_LABELS[a].length).toBeGreaterThan(3);
		expect(naturalDirection('top-right')).toBe('down');
		expect(naturalDirection('bottom')).toBe('up');
		expect(naturalDirection('center')).toBe('up');
		expect(offsetWords('bottom-left')).toEqual({
			x: 'From the left edge',
			y: 'From the bottom edge',
		});
		expect(offsetWords('center')).toEqual({ x: 'Shift right', y: 'Shift down' });
	});
});

describe('wizard', () => {
	it('round-trips the output choice through the host settings', () => {
		const s = {
			host: {
				native_overlay: true,
				hide_native_when_obs: true,
				monitor: null,
				exclude_from_capture: false,
			},
		};
		for (const c of ['obs', 'native', 'both'] as const) {
			applyOutput(s, c);
			expect(outputOf(s)).toBe(c);
			expect(OUTPUT_INFO[c].title.length).toBeGreaterThan(3);
		}
		expect(WIZARD_STEPS.map((x) => x.id)).toEqual(['monitor', 'look', 'output', 'done']);
		expect(stepIndex('output')).toBe(2);
	});

	it('describes every keyboard layout', () => {
		for (const l of KEYBOARD_LAYOUTS) expect(LAYOUT_INFO[l].name.length).toBeGreaterThan(2);
	});
});

describe('appearance', () => {
	it('reads the query first, then storage, else dark', () => {
		const store = new Map<string, string>();
		const storage = {
			getItem: (k: string) => store.get(k) ?? null,
			setItem: (k: string, v: string) => void store.set(k, v),
		};
		expect(readAppearance('', storage)).toBe('dark');
		saveAppearance('light', storage);
		expect(store.get(APPEARANCE_KEY)).toBe('light');
		expect(readAppearance('', storage)).toBe('light');
		expect(readAppearance('?appearance=dark', storage)).toBe('dark');
		const broken = {
			getItem: () => {
				throw new Error('blocked');
			},
			setItem: () => {
				throw new Error('blocked');
			},
		};
		expect(readAppearance('', broken)).toBe('dark');
		expect(() => saveAppearance('light', broken)).not.toThrow();
		const root = document.createElement('html');
		applyAppearance('light', root);
		expect(root.dataset.plate).toBe('space-white');
		applyAppearance('dark', root);
		expect(root.dataset.plate).toBeUndefined();
	});
});

function status(over: Partial<Status> = {}): Status {
	return { ...defaultStatus(), hook: { installed: true, dropped: 0 }, ...over };
}

describe('nav and status words', () => {
	it('opens the page named by the hash or the query', () => {
		expect(pageFromLocation('#obs', '')).toBe('obs');
		expect(pageFromLocation('', '?page=sound')).toBe('sound');
		expect(pageFromLocation('#nope', '?page=nope')).toBe('home');
		expect(isPageId('general')).toBe(true);
		for (const p of PAGES) expect(PAGE_INFO[p].id).toBe(p);
	});

	it('says Active, Paused or Hidden, never naming an app', () => {
		expect(activityWord(status()).word).toBe('Active');
		expect(activityWord(status({ paused: true })).word).toBe('Paused');
		const hidden = activityWord(status({ suppressed: 'excluded' }));
		expect(hidden.word).toBe('Hidden');
		expect(hidden.detail).toBe('An excluded app is in front.');
		expect(activityWord(status({ suppressed: 'password' })).detail).toBe(
			'A password field has focus.'
		);
		expect(activityWord(status({ hook: { installed: false, dropped: 0 } })).word).toBe('Starting');
	});

	it('describes OBS and the server', () => {
		const rec = status({
			obs: {
				enabled: true,
				state: 'connected',
				error: null,
				recording: { active: true, paused: false, timecode: '00:01:02.500' },
			},
		});
		expect(recordingText(rec)).toBe('Recording 00:01:02');
		expect(obsWord(rec)).toMatchObject({
			word: 'Connected',
			tone: 'ok',
			detail: 'Recording 00:01:02',
		});
		const paused = status({
			obs: {
				enabled: true,
				state: 'connected',
				error: null,
				recording: { active: true, paused: true, timecode: '00:01:02.500' },
			},
		});
		expect(recordingText(paused)).toBe('Recording paused 00:01:02');
		expect(
			obsWord(
				status({ obs: { enabled: true, state: 'auth_failed', error: null, recording: null } })
			).word
		).toBe('Sign-in failed');
		expect(obsWord(status()).word).toBe('Off');
		expect(
			serverWord(status({ server: { port: 47820, running: true, error: null, clients: 1 } })).detail
		).toBe('Port 47820, 1 source connected.');
		expect(
			serverWord(status({ server: { port: 47820, running: false, error: 'busy', clients: 0 } }))
		).toMatchObject({ word: 'Stopped', detail: 'busy', tone: 'danger' });
	});
});

describe('page defaults for the preview', () => {
	it('every page names a preview route', () => {
		const routes = new Set(['keys', 'keyboard', 'mouse', 'cursor', 'all']);
		for (const p of PAGES) expect(routes.has(PAGE_INFO[p].previewRoute)).toBe(true);
		expect(defaultOverlayConfig().callouts.anchor).toBe('bottom');
	});
});
