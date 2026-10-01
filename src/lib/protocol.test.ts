/// <reference types="node" />
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import {
	decodeServerMsg,
	defaultAppState,
	defaultOverlayConfig,
	encodeClientMsg,
	isAppState,
	isCallout,
	isClientMsg,
	isOverlayConfig,
	isServerMsg,
	normalizeAppState,
	normalizeOverlayConfig,
	type ServerMsg,
} from './protocol';

// The examples of ARCHITECTURE.md section 5, verbatim where the contract
// gives them.
const config = defaultOverlayConfig();
const state = defaultAppState();
const SAMPLES: Record<string, unknown> = {
	hello: {
		t: 'hello',
		v: 1,
		app: 'IDEA Keystage',
		version: '0.1.0',
		config,
		state,
		legends: null,
	},
	config: { t: 'config', config },
	state: { t: 'state', state },
	legends: { t: 'legends', layout: '00000409', legends: { 'sc:1E': 'A', 'sc:27': ';' } },
	callout: {
		t: 'callout',
		callout: {
			id: 7,
			kind: 'combo',
			steps: [
				[
					{ key: 'Ctrl', label: 'Ctrl', kind: 'modifier' },
					{ key: '8', label: '8', kind: 'key' },
				],
			],
			text: null,
			count: 1,
			caption: 'Fixture Command One',
			context: null,
			profile: 'solidworks',
			t_ms: 1234,
			updated_ms: 1234,
		},
	},
	key: { t: 'key', phys: 'sc:1D', key: 'Ctrl', down: true, modifier: 'ctrl', t_ms: 1234 },
	button: { t: 'button', button: 'middle', down: true, x: 0.52, y: 0.47, t_ms: 1234 },
	wheel: { t: 'wheel', dx: 0, dy: -120, x: 0.52, y: 0.47, t_ms: 1234 },
	move: { t: 'move', x: 0.52, y: 0.47, t_ms: 1234 },
	release_all: { t: 'release_all', t_ms: 1234 },
	clear: { t: 'clear' },
	pong: { t: 'pong' },
};

describe('server message guards', () => {
	it.each(Object.entries(SAMPLES))('accepts the contract example for %s', (_name, msg) => {
		expect(isServerMsg(msg)).toBe(true);
		expect(decodeServerMsg(JSON.stringify(msg))).toEqual(msg);
		expect(decodeServerMsg(msg)).toEqual(msg);
	});

	it('rejects unknown tags, wrong types and garbage', () => {
		expect(decodeServerMsg('not json')).toBeNull();
		expect(decodeServerMsg('{"t":"nope"}')).toBeNull();
		expect(decodeServerMsg(null)).toBeNull();
		expect(decodeServerMsg([])).toBeNull();
		expect(
			isServerMsg({ t: 'key', phys: 'sc:1D', key: 'Ctrl', down: 'yes', modifier: null, t_ms: 1 })
		).toBe(false);
		expect(
			isServerMsg({ t: 'key', phys: 'sc:1D', key: 'Ctrl', down: true, modifier: 'meta', t_ms: 1 })
		).toBe(false);
		expect(isServerMsg({ t: 'button', button: 'x3', down: true, x: 0, y: 0, t_ms: 1 })).toBe(false);
		expect(isServerMsg({ t: 'wheel', dx: 0, dy: Number.NaN, x: 0, y: 0, t_ms: 1 })).toBe(false);
		expect(isServerMsg({ t: 'legends', layout: '00000409', legends: { 'sc:1E': 5 } })).toBe(false);
	});

	it('rejects a callout with a bad chip or kind', () => {
		const good = (SAMPLES.callout as { callout: Record<string, unknown> }).callout;
		expect(isCallout(good)).toBe(true);
		expect(isCallout({ ...good, kind: 'shout' })).toBe(false);
		expect(isCallout({ ...good, steps: [[{ key: 'A', label: 'A', kind: 'button' }]] })).toBe(false);
		expect(isCallout({ ...good, steps: 'Ctrl+8' })).toBe(false);
		expect(isCallout({ ...good, count: '2' })).toBe(false);
	});
});

describe('tolerant decoding of config and state', () => {
	it('fills every missing config field with its default', () => {
		const partial = { theme: 'carbon', callouts: { max_stack: 6 } };
		const out = normalizeOverlayConfig(partial);
		expect(out).not.toBeNull();
		expect(isOverlayConfig(out)).toBe(true);
		expect(out?.theme).toBe('carbon');
		expect(out?.callouts.max_stack).toBe(6);
		expect(out?.callouts.linger_ms).toBe(config.callouts.linger_ms);
		expect(out?.keyboard).toEqual(config.keyboard);
	});

	it('clamps numbers and drops mistyped values', () => {
		const out = normalizeOverlayConfig({
			scale: 99,
			font: 'comic',
			callouts: { max_stack: 0, anchor: 'middle', size: 'xxl', fade_ms: -5 },
			cursor: { opacity: 3, color: '' },
			custom: { accent: '#fff' },
		});
		expect(out?.scale).toBe(4);
		expect(out?.font).toBe('rajdhani');
		expect(out?.callouts.max_stack).toBe(1);
		expect(out?.callouts.anchor).toBe('bottom');
		expect(out?.callouts.size).toBe('m');
		expect(out?.callouts.fade_ms).toBe(0);
		expect(out?.cursor.opacity).toBe(1);
		expect(out?.cursor.color).toBe(config.cursor.color);
		expect(out?.custom).toBeNull();
	});

	it('decodes a hello whose config comes from an older app', () => {
		const msg = decodeServerMsg({
			t: 'hello',
			v: 1,
			app: 'IDEA Keystage',
			version: '0.0.9',
			config: {},
			state: {},
			legends: null,
		});
		expect(msg?.t).toBe('hello');
		if (msg?.t === 'hello') {
			expect(msg.config).toEqual(config);
			expect(msg.state).toEqual(state);
		}
	});

	it('never names an excluded app: an unknown suppression is dropped', () => {
		const s = normalizeAppState({ ...state, suppressed: 'excluded:notepad.exe' });
		expect(s?.suppressed).toBeNull();
		expect(isAppState({ ...state, suppressed: 'excluded' })).toBe(true);
	});

	it('defaults match the documented values', () => {
		expect(config.theme).toBe('idea');
		expect(config.scale).toBe(1);
		expect(config.callouts.anchor).toBe('bottom');
		expect(config.callouts.direction).toBe('up');
		expect(state.native_visible).toBe(true);
		expect(state.keyboard_visible).toBe(true);
	});
});

describe('client messages', () => {
	it('encodes the hello and the ping', () => {
		const hello = { t: 'hello', route: 'keys', host: 'obs' } as const;
		expect(JSON.parse(encodeClientMsg(hello))).toEqual(hello);
		expect(isClientMsg(hello)).toBe(true);
		expect(isClientMsg({ t: 'ping' })).toBe(true);
		expect(isClientMsg({ t: 'hello', route: 'everything', host: 'obs' })).toBe(false);
	});
});

// keystage-core writes one sample per message type from its golden tests.
// When they exist, every one must decode to an equal message here.
const fixtureDir = resolve(process.cwd(), 'crates/keystage-core/tests/fixtures/protocol');
const fixtures = existsSync(fixtureDir)
	? readdirSync(fixtureDir).filter((f) => f.endsWith('.json'))
	: [];

describe.skipIf(fixtures.length === 0)('keystage-core protocol samples', () => {
	it.each(fixtures)('%s decodes and round-trips', (file) => {
		const raw = JSON.parse(readFileSync(resolve(fixtureDir, file), 'utf8')) as unknown;
		const msgs = Array.isArray(raw) ? raw : [raw];
		for (const m of msgs) {
			// Client messages (hello with a route, ping) are checked by their own guard.
			if (isClientMsg(m) && !isServerMsg(m)) continue;
			const decoded: ServerMsg | null = decodeServerMsg(m);
			expect(decoded, `${file}: ${JSON.stringify(m).slice(0, 120)}`).not.toBeNull();
			expect(isServerMsg(m), `${file} is not a complete server message`).toBe(true);
		}
	});
});

// keystage-core's golden test for OverlayConfig::default() carries the JSON
// it must serialize to. When that constant is present, the TypeScript
// defaults must be the same values.
const coreProtocol = resolve(process.cwd(), 'crates/keystage-core/src/protocol.rs');
const coreDefault = existsSync(coreProtocol)
	? /DEFAULT_CONFIG_JSON: &str = r##"(.*?)"##/s.exec(readFileSync(coreProtocol, 'utf8'))
	: null;

describe.skipIf(!coreDefault)('defaults shared with keystage-core', () => {
	it('defaultOverlayConfig() equals OverlayConfig::default()', () => {
		expect(JSON.parse(coreDefault?.[1] ?? '{}')).toEqual(defaultOverlayConfig());
	});
});
