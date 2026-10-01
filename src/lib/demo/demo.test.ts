/// <reference types="node" />
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import {
	defaultOverlayConfig,
	isServerMsg,
	type Callout,
	type OverlayConfig,
	type Route,
	type ServerMsg,
} from '../protocol';
import { CLEARANCE_PX, calloutsOffsetY, placedBox } from '../renderer/layout';
import {
	CALLOUT_GEOMETRY,
	CALLOUT_UNIT_PX,
	KEYBOARD_UNIT_PX,
	calloutHeightU,
} from '../renderer/metrics';
import { keyboardDrawingSize, mouseDrawingSize } from '../renderer/sizes';
import { getLayout } from '../renderer/keyboard/layouts';
import { indexLayout, resolveKey } from '../renderer/keyboard/model';
import { buildDemoScript, demoOverlayConfig } from './index';
import {
	asProfileData,
	captionForKeys,
	captionForMouse,
	splitCombo,
	type ProfileData,
} from './profile-data';
import { DEMO_SCAN_CODES } from './scancodes';
import { Timeline, orderCombo } from './timeline';

// Synthetic data for the matching rules: fixture names only.
const FIXTURE: ProfileData = {
	id: 'fixture',
	name: 'Fixture',
	shortcuts: [
		{ id: 'k1', keys: ['Ctrl', 'Q'], command: 'Fixture Command One', context: 'Somewhere' },
		{ id: 'k2', keys: ['Ctrl', 'Q'], command: 'Fixture Command Two', context: '' },
		{ id: 'k3', keys: ['Shift', 'Ctrl', 'Q'], command: 'Fixture Command Three', context: 'A' },
		{ id: 'k4', keys: ['Q'], command: 'Fixture Command Four', context: 'B' },
		{ id: 'k5', keys: ['Q'], command: 'Fixture Command Five', context: 'C' },
		{ id: 'k6', keys: ['Alt', 'Q', 'W'], command: 'Fixture Chord', context: '' },
	],
	mouse: [
		{
			id: 'm1',
			button: 'middle',
			action: 'drag',
			modifiers: [],
			direction: null,
			command: 'Fixture Drag',
			context: 'X',
		},
		{
			id: 'm2',
			button: 'unspecified',
			action: 'drag',
			modifiers: ['Alt'],
			direction: null,
			command: 'Fixture Alt Drag',
			context: '',
		},
		{
			id: 'm3',
			button: 'middle',
			action: 'wheel',
			modifiers: [],
			direction: null,
			command: 'Fixture Wheel',
			context: '',
		},
		{
			id: 'm4',
			button: 'right',
			action: 'gesture',
			modifiers: [],
			direction: 'up',
			command: 'Fixture Gesture Up',
			context: '',
		},
		{
			id: 'm5',
			button: 'left',
			action: 'drag',
			modifiers: [],
			direction: 'up',
			command: 'Fixture Directed Drag',
			context: '',
		},
	],
};

describe('profile caption lookup (the core rules)', () => {
	it('prefers an empty context, else the first entry in file order', () => {
		expect(captionForKeys(FIXTURE, ['Ctrl', 'Q'])).toEqual({
			id: 'k2',
			caption: 'Fixture Command Two',
			context: null,
		});
		expect(captionForKeys(FIXTURE, ['Q'])).toEqual({
			id: 'k4',
			caption: 'Fixture Command Four',
			context: 'B',
		});
		expect(captionForKeys(FIXTURE, ['Ctrl', 'Shift', 'Q'])?.id).toBe('k3');
		expect(captionForKeys(FIXTURE, ['Ctrl', 'W'])).toBeNull();
		expect(captionForKeys(FIXTURE, ['Alt', 'Q', 'W'])).toBeNull();
	});

	it('matches mouse entries by button, action, modifiers and direction', () => {
		expect(captionForMouse(FIXTURE, 'middle', 'drag', [])).toEqual({
			id: 'm1',
			caption: 'Fixture Drag',
			context: 'X',
		});
		expect(captionForMouse(FIXTURE, 'left', 'drag', ['Alt'])?.id).toBe('m2');
		expect(captionForMouse(FIXTURE, 'right', 'drag', ['Alt'])).toBeNull();
		expect(captionForMouse(FIXTURE, 'middle', 'wheel', [], 'down')?.id).toBe('m3');
		expect(captionForMouse(FIXTURE, 'right', 'gesture', [], 'up')?.id).toBe('m4');
		expect(captionForMouse(FIXTURE, 'right', 'gesture', [], 'left')).toBeNull();
		expect(captionForMouse(FIXTURE, 'left', 'drag', [])).toBeNull();
	});

	it('splits combos and rejects bad data', () => {
		expect(splitCombo(['Ctrl', 'Shift', 'S'])).toEqual({ mods: ['Ctrl', 'Shift'], key: 'S' });
		expect(splitCombo(['Alt'])).toEqual({ mods: [], key: 'Alt' });
		expect(splitCombo(['Alt', 'F', 'C'])).toBeNull();
		expect(() => asProfileData({})).toThrow();
		expect(orderCombo(['S', 'Shift', 'Ctrl'])).toEqual(['Ctrl', 'Shift', 'S']);
	});
});

describe('Timeline', () => {
	it('emits combos the way a person types them', () => {
		const t = new Timeline();
		const c = t.combo(['Ctrl', 'S'], {});
		const steps = t.build();
		expect(steps.map((s) => s.msg.t)).toEqual(['key', 'key', 'callout', 'key', 'key']);
		expect(c.kind).toBe('combo');
		expect(c.steps[0]?.map((x) => x.kind)).toEqual(['modifier', 'key']);
		const again = t.repeat(c, ['Ctrl', 'S']);
		expect(again.count).toBe(2);
		expect(again.id).toBe(c.id);
	});

	it('emits pointer moves only when asked, and always advances time', () => {
		const a = new Timeline({ pointer: true });
		a.glide(0.9, 0.9, 400);
		const b = new Timeline({ pointer: false });
		b.glide(0.9, 0.9, 400);
		expect(a.t).toBe(400);
		expect(b.t).toBe(400);
		expect(a.length).toBe(10);
		expect(b.length).toBe(0);
		expect(a.x).toBe(0.9);
	});

	it('every demo key has a scan code id in the wire format', () => {
		for (const id of Object.values(DEMO_SCAN_CODES)) expect(id).toMatch(/^sc:(E0)?[0-9A-F]{2}$/);
	});
});

const solidworks = asProfileData(
	JSON.parse(readFileSync(resolve(process.cwd(), 'data/profiles/solidworks.json'), 'utf8'))
);

function callouts(steps: readonly { msg: ServerMsg }[]): Callout[] {
	return steps.flatMap((s) => (s.msg.t === 'callout' ? [s.msg.callout] : []));
}

describe('demo scripts', () => {
	it.each(['tour', 'solidworks', 'general', 'states'] as const)(
		'%s is a valid protocol stream',
		(id) => {
			const script = buildDemoScript(id, 'all', solidworks);
			expect(isServerMsg(script.hello)).toBe(true);
			let last = -1;
			for (const s of script.steps) {
				expect(isServerMsg(s.msg), JSON.stringify(s.msg).slice(0, 100)).toBe(true);
				expect(s.at).toBeGreaterThanOrEqual(last);
				last = s.at;
			}
			expect(script.durationMs).toBeGreaterThan(last);
		}
	);

	it('takes every SolidWorks caption from the data file, never from code', () => {
		const script = buildDemoScript('solidworks', 'keys', solidworks);
		const commands = new Set([
			...solidworks.shortcuts.map((s) => s.command.trim()),
			...solidworks.mouse.map((m) => m.command.trim()),
		]);
		const captioned = callouts(script.steps).filter((c) => c.caption !== null);
		expect(captioned.length).toBeGreaterThanOrEqual(5);
		for (const c of captioned) expect(commands.has(c.caption ?? '')).toBe(true);
	});

	it('the general segment is shortcuts only: no captions, Typing is masked with no key messages', () => {
		const script = buildDemoScript('general', 'keyboard', null);
		const cs = callouts(script.steps);
		expect(cs.every((c) => c.caption === null)).toBe(true);
		const masked = cs.filter((c) => c.kind === 'masked');
		expect(masked.length).toBeGreaterThan(0);
		expect(masked.every((c) => c.text === null)).toBe(true);
		// While the masked tag is open, the only key message is the Enter that closes it.
		const first = script.steps.findIndex(
			(s) => s.msg.t === 'callout' && s.msg.callout.kind === 'masked'
		);
		const enterAt = script.steps.findIndex((s, i) => i > first && s.msg.t === 'key');
		const between = script.steps.slice(first + 1, enterAt).filter((s) => s.msg.t === 'key');
		expect(between).toHaveLength(0);
		const enter = script.steps[enterAt]?.msg;
		expect(enter?.t === 'key' && enter.key).toBe('Enter');
	});

	it('emits pointer moves only for the cursor and all routes', () => {
		const moves = (route: 'keys' | 'cursor') =>
			buildDemoScript('general', route, null).steps.filter((s) => s.msg.t === 'move').length;
		expect(moves('keys')).toBe(0);
		expect(moves('cursor')).toBeGreaterThan(20);
	});

	it('needs the data for the SolidWorks scripts', () => {
		expect(() => buildDemoScript('solidworks', 'keys', null)).toThrow();
	});
});

describe('the demo composition', () => {
	function helloConfig(route: Route): OverlayConfig {
		const hello = buildDemoScript('tour', route, solidworks).hello;
		if (hello.t !== 'hello') throw new Error('the demo starts with a hello');
		return hello.config;
	}

	it('turns every piece on and keeps the defaults on the dedicated routes', () => {
		for (const route of ['keys', 'keyboard', 'mouse', 'cursor'] as const) {
			const c = helloConfig(route);
			const d = defaultOverlayConfig();
			expect(c.callouts).toEqual(d.callouts);
			expect(c.keyboard).toEqual({ ...d.keyboard, enabled: true });
			expect(c.cursor.enabled).toBe(true);
		}
		expect(helloConfig('all')).toEqual(demoOverlayConfig('all'));
	});

	it.each([
		['1080p at scale 1', { width: 1920, height: 1080 }, 1],
		['4K at scale 1', { width: 3840, height: 2160 }, 1],
		['4K at scale 2', { width: 3840, height: 2160 }, 2],
	] as const)('on all, nothing has to move out of the way at %s', (_name, stage, scale) => {
		const c = helloConfig('all');
		const kbSize = keyboardDrawingSize(c.keyboard, KEYBOARD_UNIT_PX[c.keyboard.size] * scale);
		const others = { keyboard: kbSize, mouse: c.mouse.enabled ? mouseDrawingSize(scale) : null };
		// The callouts stay where the config puts them...
		expect(calloutsOffsetY(c, others, stage, scale)).toBe(c.callouts.offset_y);
		// ...and the widest, tallest stack they may draw misses the keyboard.
		const u = CALLOUT_UNIT_PX[c.callouts.size] * scale;
		const g = CALLOUT_GEOMETRY;
		const n = c.callouts.max_stack;
		const stack = placedBox(
			c.callouts.anchor,
			c.callouts.offset_x,
			c.callouts.offset_y,
			scale,
			{
				width: g.maxWidthU * u,
				height: (n * calloutHeightU(c.callouts.captions) + (n - 1) * g.stackGapU) * u,
			},
			stage
		);
		const kb = placedBox(
			c.keyboard.anchor,
			c.keyboard.offset_x,
			c.keyboard.offset_y,
			scale,
			kbSize,
			stage
		);
		expect(stack.left).toBeGreaterThan(kb.right + CLEARANCE_PX * scale);
	});

	it.each(['tour', 'solidworks', 'general', 'states'] as const)(
		'every key the %s demo presses on all is on the keyboard it draws',
		(id) => {
			const c = helloConfig('all');
			const index = indexLayout(getLayout(c.keyboard.layout));
			const keys = buildDemoScript(id, 'all', solidworks).steps.flatMap((s) =>
				s.msg.t === 'key' ? [s.msg] : []
			);
			const missing = keys
				.filter((k) => resolveKey(index, k.phys, k.key) === null)
				.map((k) => k.key);
			expect(missing).toEqual([]);
		}
	);
});
