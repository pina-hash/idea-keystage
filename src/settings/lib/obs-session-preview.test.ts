import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { ROUTES, defaultOverlayConfig } from '../../lib/protocol';
import { calloutsBox, obsSourceSize } from '../../lib/renderer/sizes';
import type { Monitor, ServerUrls } from '../types';
import {
	OBS_DEFAULT_CSS,
	OBS_FIELDS,
	OBS_WEBSOCKET_STEPS,
	ROUTE_INFO,
	isLoopbackHost,
	obsHostError,
	portError,
	routeUrl,
	sourceCards,
	sourcesSummary,
} from './obs';
import { EXPORT_KINDS, cleanMarkerName, exportFileName, markerTime, readSession } from './session';
import { CLOSE_UP_STACK, closeUp, focusAnchor, focusBox } from './preview';

const MON: Monitor = {
	id: 'm',
	name: 'M',
	x: 0,
	y: 0,
	width: 2560,
	height: 1440,
	scale: 1,
	primary: true,
};

describe('OBS sources', () => {
	it('uses the reported URL when the port matches, else builds the standard one', () => {
		const config = defaultOverlayConfig();
		const urls: ServerUrls = {
			port: 47820,
			running: true,
			error: null,
			routes: [{ route: 'keys', url: 'http://127.0.0.1:47820/overlay/keys', width: 1, height: 1 }],
		};
		const cards = sourceCards(urls, 47820, config, MON);
		expect(cards.map((c) => c.route)).toEqual([...ROUTES]);
		expect(cards[0]!.url).toBe('http://127.0.0.1:47820/overlay/keys');
		expect(cards[1]!.url).toBe(routeUrl(47820, 'keyboard'));
		const moved = sourceCards(urls, 50000, config, MON);
		expect(moved[0]!.url).toBe('http://127.0.0.1:50000/overlay/keys');
	});

	it('takes every size from obsSourceSize for the chosen monitor', () => {
		const config = defaultOverlayConfig();
		for (const c of sourceCards(null, 47820, config, MON)) {
			expect(c.size).toEqual(obsSourceSize(c.route, config, MON));
		}
		const none = sourceCards(null, 47820, config, null);
		expect(none.find((c) => c.route === 'all')!.size).toEqual({ width: 1920, height: 1080 });
		expect(sourcesSummary(none).split('\n')).toHaveLength(ROUTES.length);
	});

	it('tells the user the OBS field values from the research', () => {
		const field = (name: string) => OBS_FIELDS.find((f) => f.field === name);
		expect(field('Shutdown source when not visible')?.value).toBe('Off');
		expect(field('Refresh browser when scene becomes active')?.value).toBe('Off');
		expect(field('Local file')?.value).toBe('Off');
		expect(field('Custom CSS')?.value).toBe('Leave the default');
		expect(OBS_DEFAULT_CSS).toBe(
			'body { background-color: rgba(0, 0, 0, 0); margin: 0px auto; overflow: hidden; }'
		);
		expect(OBS_WEBSOCKET_STEPS.join(' ')).toContain('Tools > WebSocket Server Settings');
		for (const r of ROUTES) expect(ROUTE_INFO[r].route).toBe(r);
	});

	it('checks hosts and ports', () => {
		expect(isLoopbackHost('LOCALHOST')).toBe(true);
		expect(isLoopbackHost('192.168.1.5')).toBe(false);
		expect(obsHostError('')).not.toBeNull();
		expect(obsHostError('my pc')).not.toBeNull();
		expect(obsHostError('http://x/')).not.toBeNull();
		expect(obsHostError('192.168.1.5')).toBeNull();
		expect(portError(0)).not.toBeNull();
		expect(portError(4455)).toBeNull();
		expect(portError(70000)).not.toBeNull();
	});
});

describe('reading sessions', () => {
	const golden = resolve(
		__dirname,
		'../../../crates/keystage-session/tests/golden/obs.json.golden'
	);

	it('reads the keystage-session golden file', () => {
		if (!existsSync(golden)) return;
		const raw = JSON.parse(readFileSync(golden, 'utf8')) as Record<string, unknown>;
		const s = readSession(raw)!;
		expect(s.id).toBe(raw.id);
		expect(s.started).toBe(raw.started);
		const start = raw.start_ms as number;
		const markers = raw.markers as { t_ms: number; name: string }[];
		expect(s.markers.map((m) => m.name)).toEqual(markers.map((m) => m.name));
		expect(s.markers.map((m) => m.offsetMs)).toEqual(markers.map((m) => m.t_ms - start));
		expect(s.durationMs).toBe((raw.end_ms as number) - start);
		expect(s.callouts).toBe((raw.callouts as unknown[]).length);
	});

	it('tolerates other marker clocks and missing fields', () => {
		const s = readSession({
			id: 'x',
			markers: [
				{ offset_ms: 5000, name: 'A' },
				{ t_ms: 9000, name: 'B', timecode: '00:00:09' },
				'junk',
			],
		})!;
		expect(s.markers.map((m) => [m.name, m.offsetMs])).toEqual([
			['A', 5000],
			['B', 9000],
		]);
		expect(markerTime(s.markers[0]!)).toBe('0:05');
		expect(markerTime(s.markers[1]!)).toBe('00:00:09');
		expect(s.durationMs).toBeNull();
		expect(readSession('nope')).toBeNull();
		expect(readSession({}, 'fallback')!.id).toBe('fallback');
	});

	it('names export files from the start time', () => {
		const started = new Date(2026, 8, 30, 14, 5).toISOString();
		const chapters = EXPORT_KINDS.find((k) => k.format === 'chapters')!;
		expect(exportFileName(started, chapters)).toBe('keystage-2026-09-30-1405-chapters.txt');
		const srt = EXPORT_KINDS.find((k) => k.format === 'srt')!;
		expect(exportFileName(started, srt)).toBe('keystage-2026-09-30-1405.srt');
		expect(exportFileName('soon', srt)).toBe('keystage-session.srt');
		expect(EXPORT_KINDS.map((k) => k.format).sort()).toEqual(['chapters', 'json', 'md', 'srt']);
		expect(cleanMarkerName('  Base \n sketch  ')).toBe('Base sketch');
		expect(cleanMarkerName('x'.repeat(200))).toHaveLength(120);
	});
});

describe('preview close-up', () => {
	const screen = { width: 1920, height: 1080 };
	const frame = { width: 640, height: 360 };

	it('follows the anchor of the piece the route shows', () => {
		const c = defaultOverlayConfig();
		c.keyboard.anchor = 'bottom-left';
		c.mouse.anchor = 'top-right';
		expect(focusAnchor('keys', c)).toBe('bottom');
		expect(focusAnchor('all', c)).toBe('bottom');
		expect(focusAnchor('keyboard', c)).toBe('bottom-left');
		expect(focusAnchor('mouse', c)).toBe('top-right');
		expect(focusAnchor('cursor', c)).toBe('center');
		expect(focusBox('cursor', c)).toBeNull();
	});

	it('draws at actual size when the piece fits, smaller when it does not', () => {
		const c = defaultOverlayConfig();
		c.keyboard.layout = 'left';
		const wide = { width: 700, height: 394 };
		const kb = closeUp('keyboard', c, screen, wide);
		expect(kb.zoom).toBe(1);
		expect(kb.label).toBe('Actual size');
		// Bottom left: the screen's bottom-left corner sits on the frame's.
		expect(kb.left).toBe(0);
		expect(kb.top).toBe(wide.height - screen.height);
		c.keyboard.layout = 'full';
		const full = closeUp('keyboard', c, screen, frame);
		expect(full.zoom).toBeLessThan(1);
		expect(full.label).toMatch(/% of actual size$/);
	});

	it('makes room for only the newest callouts', () => {
		const c = defaultOverlayConfig();
		c.callouts.max_stack = 10;
		const box = focusBox('keys', c)!;
		const two = calloutsBox({ ...c, callouts: { ...c.callouts, max_stack: CLOSE_UP_STACK } });
		expect(box).toEqual(two);
		const cu = closeUp('keys', c, screen, frame);
		expect(cu.left).toBeCloseTo((frame.width - screen.width * cu.zoom) / 2);
	});

	it('centers a screen smaller than the frame', () => {
		const tiny = closeUp('cursor', defaultOverlayConfig(), { width: 320, height: 180 }, frame);
		expect(tiny.left).toBe(160);
		expect(tiny.top).toBe(90);
	});
});
