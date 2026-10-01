// @vitest-environment node
/// <reference types="node" />
/**
 * Cross-language contract test for wire protocol v1 (docs/ARCHITECTURE.md
 * section 5).
 *
 * keystage-core writes one JSON sample per message to
 * crates/keystage-core/tests/fixtures/protocol/ (tests/protocol_samples.rs,
 * real Rust serialization and real Processor output). Every sample must pass
 * the strict TypeScript guards in src/lib/protocol.ts, decode to exactly the
 * same value, and carry exactly the fields the contract names: a field the
 * Rust side adds or drops is a failure here, not a silent default in the
 * renderer.
 *
 * The second half reads the Rust declarations themselves (serde enums, the
 * theme list, the limits and the constants) and compares them with the
 * TypeScript value lists, so the two sides cannot drift apart unnoticed.
 */
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { basename, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import {
	ANCHORS,
	APP_NAME,
	CALLOUT_KINDS,
	CALLOUT_SIZES,
	CHIP_KINDS,
	CLIENT_HOSTS,
	CURSOR_STYLES,
	DEFAULT_PORT,
	DIRECTIONS,
	FONTS,
	KEYBOARD_LAYOUTS,
	KEYBOARD_SIZES,
	LIMITS,
	MODIFIERS,
	MOUSE_BUTTONS,
	PROTOCOL_VERSION,
	ROUTES,
	SOLIDWORKS_MODES,
	SUPPRESSIONS,
	THEME_IDS,
	decodeServerMsg,
	defaultAppState,
	defaultOverlayConfig,
	encodeClientMsg,
	isClientMsg,
	isServerMsg,
	type ClientMsg,
} from '../../src/lib/protocol';

const ROOT = fileURLToPath(new URL('../..', import.meta.url));
const FIXTURE_DIR = resolve(ROOT, 'crates/keystage-core/tests/fixtures/protocol');
const RUST_PROTOCOL = resolve(ROOT, 'crates/keystage-core/src/protocol.rs');
const RUST_EVENT = resolve(ROOT, 'crates/keystage-core/src/event.rs');

/** Every server tag of section 5, in contract order. */
const SERVER_TAGS = [
	'hello',
	'config',
	'state',
	'legends',
	'callout',
	'key',
	'button',
	'wheel',
	'move',
	'release_all',
	'clear',
	'pong',
] as const;
const CLIENT_TAGS = ['hello', 'ping'] as const;

/** The exact top-level fields of each message, as section 5 spells them. */
const SERVER_FIELDS: Record<(typeof SERVER_TAGS)[number], readonly string[]> = {
	hello: ['t', 'v', 'app', 'version', 'config', 'state', 'legends'],
	config: ['t', 'config'],
	state: ['t', 'state'],
	legends: ['t', 'layout', 'legends'],
	callout: ['t', 'callout'],
	key: ['t', 'phys', 'key', 'down', 'modifier', 't_ms'],
	button: ['t', 'button', 'down', 'x', 'y', 't_ms'],
	wheel: ['t', 'dx', 'dy', 'x', 'y', 't_ms'],
	move: ['t', 'x', 'y', 't_ms'],
	release_all: ['t', 't_ms'],
	clear: ['t'],
	pong: ['t'],
};
const CLIENT_FIELDS: Record<(typeof CLIENT_TAGS)[number], readonly string[]> = {
	hello: ['t', 'route', 'host'],
	ping: ['t'],
};
const CALLOUT_FIELDS = [
	'id',
	'kind',
	'steps',
	'text',
	'count',
	'caption',
	'context',
	'profile',
	't_ms',
	'updated_ms',
];
const CHIP_FIELDS = ['key', 'label', 'kind'];
const CUSTOM_THEME_FIELDS = ['accent', 'face', 'ink', 'glow'];
const RECORDING_FIELDS = ['active', 'paused', 'timecode'];
const SESSION_FIELDS = ['active', 'started_ms', 'markers'];
const PROFILE_FIELDS = ['id', 'name'];

type Json = null | boolean | number | string | Json[] | { [k: string]: Json };
type Obj = { [k: string]: Json };

function isObj(v: unknown): v is Obj {
	return typeof v === 'object' && v !== null && !Array.isArray(v);
}

function sortedKeys(v: Obj): string[] {
	return Object.keys(v).sort();
}

/**
 * Field-by-field shape comparison against a reference object: same keys at
 * every level, and the same JSON type for every leaf. A null on either side
 * is not compared (the nullable fields get their own exact-key checks).
 */
function shapeProblems(actual: Json, expected: Json, path: string, out: string[]): void {
	if (isObj(expected)) {
		if (!isObj(actual)) {
			out.push(`${path}: expected an object`);
			return;
		}
		const a = sortedKeys(actual);
		const e = sortedKeys(expected);
		for (const k of e) if (!a.includes(k)) out.push(`${path}.${k}: missing`);
		for (const k of a) if (!e.includes(k)) out.push(`${path}.${k}: not in the contract`);
		for (const k of e) {
			if (a.includes(k)) shapeProblems(actual[k] as Json, expected[k] as Json, `${path}.${k}`, out);
		}
		return;
	}
	if (expected === null || actual === null) return;
	if (typeof actual !== typeof expected) {
		out.push(`${path}: ${typeof actual} where the contract has ${typeof expected}`);
	}
}

function exactKeys(v: unknown, fields: readonly string[], path: string, out: string[]): void {
	if (!isObj(v)) {
		out.push(`${path}: expected an object`);
		return;
	}
	const have = sortedKeys(v);
	const want = [...fields].sort();
	for (const k of want) if (!have.includes(k)) out.push(`${path}.${k}: missing`);
	for (const k of have) if (!want.includes(k)) out.push(`${path}.${k}: not in the contract`);
}

/** Checks every nested object of a server message against the contract. */
function contractProblems(msg: Obj): string[] {
	const out: string[] = [];
	const t = msg.t as (typeof SERVER_TAGS)[number];
	exactKeys(msg, SERVER_FIELDS[t], t, out);
	const config = defaultOverlayConfig() as unknown as Obj;
	const state = defaultAppState() as unknown as Obj;
	const checkConfig = (c: Json, path: string) => {
		shapeProblems(c, config, path, out);
		if (isObj(c) && isObj(c.custom))
			exactKeys(c.custom, CUSTOM_THEME_FIELDS, `${path}.custom`, out);
	};
	const checkState = (s: Json, path: string) => {
		shapeProblems(s, state, path, out);
		if (!isObj(s)) return;
		if (isObj(s.profile)) exactKeys(s.profile, PROFILE_FIELDS, `${path}.profile`, out);
		if (isObj(s.recording)) exactKeys(s.recording, RECORDING_FIELDS, `${path}.recording`, out);
		if (isObj(s.session)) exactKeys(s.session, SESSION_FIELDS, `${path}.session`, out);
	};
	if (t === 'hello') {
		checkConfig(msg.config as Json, 'hello.config');
		checkState(msg.state as Json, 'hello.state');
		if (isObj(msg.legends)) exactKeys(msg.legends, ['layout', 'legends'], 'hello.legends', out);
	}
	if (t === 'config') checkConfig(msg.config as Json, 'config.config');
	if (t === 'state') checkState(msg.state as Json, 'state.state');
	if (t === 'callout' && isObj(msg.callout)) {
		exactKeys(msg.callout, CALLOUT_FIELDS, 'callout.callout', out);
		const steps = msg.callout.steps;
		if (Array.isArray(steps)) {
			steps.forEach((step, i) => {
				if (!Array.isArray(step)) return;
				step.forEach((chip, j) =>
					exactKeys(chip, CHIP_FIELDS, `callout.callout.steps[${i}][${j}]`, out)
				);
			});
		}
	}
	return out;
}

interface Fixture {
	file: string;
	stem: string;
	value: unknown;
}

function loadFixtures(): Fixture[] {
	if (!existsSync(FIXTURE_DIR)) return [];
	return readdirSync(FIXTURE_DIR)
		.filter((f) => f.endsWith('.json'))
		.sort()
		.map((file) => ({
			file,
			stem: basename(file, '.json'),
			value: JSON.parse(readFileSync(resolve(FIXTURE_DIR, file), 'utf8')) as unknown,
		}));
}

const fixtures = loadFixtures();
const clientFixtures = fixtures.filter((f) => f.stem.startsWith('client_'));
const serverFixtures = fixtures.filter((f) => !f.stem.startsWith('client_'));

/** `callout_mouse` names a `callout`, `release_all` names `release_all`. */
function tagFromStem(stem: string): string | undefined {
	return [...SERVER_TAGS]
		.sort((a, b) => b.length - a.length)
		.find((t) => stem === t || stem.startsWith(`${t}_`));
}

describe('keystage-core protocol samples', () => {
	it('exist, so this contract test can never pass by finding nothing', () => {
		expect(existsSync(FIXTURE_DIR), FIXTURE_DIR).toBe(true);
		expect(serverFixtures.length).toBeGreaterThan(0);
		expect(clientFixtures.length).toBeGreaterThan(0);
	});

	it('cover every server and client message of the contract', () => {
		const serverTags = new Set(serverFixtures.map((f) => (f.value as Obj).t));
		for (const t of SERVER_TAGS) expect(serverTags, `a sample for "${t}"`).toContain(t);
		const clientTags = new Set(clientFixtures.map((f) => (f.value as Obj).t));
		for (const t of CLIENT_TAGS) expect(clientTags, `a client sample for "${t}"`).toContain(t);
	});

	it('cover every callout kind and chip kind', () => {
		const callouts = serverFixtures
			.map((f) => f.value as Obj)
			.filter((m) => m.t === 'callout')
			.map((m) => m.callout as Obj);
		const kinds = new Set(callouts.map((c) => c.kind));
		for (const k of CALLOUT_KINDS) expect(kinds, `a callout of kind "${k}"`).toContain(k);
		const chipKinds = new Set(
			callouts.flatMap((c) => (c.steps as Obj[][]).flat().map((chip) => chip.kind))
		);
		for (const k of CHIP_KINDS) expect(chipKinds, `a chip of kind "${k}"`).toContain(k);
	});

	it.each(serverFixtures.map((f) => [f.file, f] as const))(
		'%s passes the strict guard and decodes to the same value',
		(_file, f) => {
			const msg = f.value;
			expect(isObj(msg), `${f.file} holds one JSON object`).toBe(true);
			const t = (msg as Obj).t;
			expect(tagFromStem(f.stem), `${f.file} is named after its "t"`).toBe(t);
			expect(isServerMsg(msg), `${f.file} fails isServerMsg`).toBe(true);
			expect(isClientMsg(msg) && !isServerMsg(msg)).toBe(false);
			// Decoding must not drop, add, clamp or default anything.
			expect(decodeServerMsg(msg)).toEqual(msg);
			expect(decodeServerMsg(JSON.stringify(msg))).toEqual(msg);
			expect(contractProblems(msg as Obj)).toEqual([]);
		}
	);

	it.each(clientFixtures.map((f) => [f.file, f] as const))(
		'%s passes the client guard and encodes back to the same JSON',
		(_file, f) => {
			const msg = f.value;
			expect(isClientMsg(msg), `${f.file} fails isClientMsg`).toBe(true);
			const t = (msg as Obj).t as (typeof CLIENT_TAGS)[number];
			expect(f.stem).toBe(`client_${t}`);
			const out: string[] = [];
			exactKeys(msg, CLIENT_FIELDS[t], `client ${t}`, out);
			expect(out).toEqual([]);
			expect(JSON.parse(encodeClientMsg(msg as ClientMsg))).toEqual(msg);
		}
	);

	it('hello_minimal carries exactly the TypeScript defaults', () => {
		const f = serverFixtures.find((x) => x.stem === 'hello_minimal');
		expect(f, 'hello_minimal.json').toBeDefined();
		const msg = f?.value as Obj;
		expect(msg.config).toEqual(defaultOverlayConfig());
		expect(msg.state).toEqual(defaultAppState());
		expect(msg.v).toBe(PROTOCOL_VERSION);
		expect(msg.app).toBe(APP_NAME);
	});

	it('every hello and config sample uses a theme the renderer ships', () => {
		for (const f of serverFixtures) {
			const m = f.value as Obj;
			const config = m.t === 'hello' || m.t === 'config' ? (m.config as Obj) : null;
			if (config) expect(THEME_IDS as readonly string[], f.file).toContain(config.theme);
		}
	});
});

/* ------------------------------------------------------------------------ */
/* The Rust declarations against the TypeScript value lists                  */
/* ------------------------------------------------------------------------ */

const rustProtocol = readFileSync(RUST_PROTOCOL, 'utf8');
const rustEvent = readFileSync(RUST_EVENT, 'utf8');

function serdeCase(name: string, rule: string | null): string {
	const words = name.replace(/([a-z0-9])([A-Z])/g, '$1 $2').split(' ');
	switch (rule) {
		case 'lowercase':
			return name.toLowerCase();
		case 'snake_case':
			return words.map((w) => w.toLowerCase()).join('_');
		case 'kebab-case':
			return words.map((w) => w.toLowerCase()).join('-');
		case null:
			return name;
		default:
			throw new Error(`serde rule ${rule} is not handled by this test`);
	}
}

/**
 * The wire names of a Rust enum: its variants after `rename_all` and any
 * per-variant `rename`. Reads only the variants at the enum's top level.
 */
function rustEnumWireNames(src: string, name: string): string[] {
	const header = new RegExp(`pub enum ${name}\\s*\\{`);
	const m = header.exec(src);
	if (!m) throw new Error(`pub enum ${name} not found`);
	// The attributes right above the enum (derive, serde), up to the item before it.
	const before = src.slice(0, m.index).split('\n');
	let rule: string | null = null;
	for (let i = before.length - 1; i >= 0; i--) {
		const line = (before[i] ?? '').trim();
		if (line === '' || line.startsWith('pub enum')) continue;
		if (!line.startsWith('#[') && !line.startsWith('//')) break;
		const r = /rename_all\s*=\s*"([^"]+)"/.exec(line);
		if (r) rule = r[1] ?? null;
	}
	const names: string[] = [];
	let depth = 1;
	let pending: string | null = null;
	const body = src.slice(m.index + m[0].length).split('\n');
	for (const raw of body) {
		const line = raw.replace(/\/\/.*$/, '').trim();
		if (depth === 1) {
			const rename = /^#\[serde\(rename\s*=\s*"([^"]+)"\)\]/.exec(line);
			if (rename) pending = rename[1] ?? null;
			const variant = /^([A-Z][A-Za-z0-9]*)\s*(?:[,{(]|$)/.exec(line);
			if (variant && variant[1]) {
				names.push(pending ?? serdeCase(variant[1], rule));
				pending = null;
			}
		}
		for (const ch of line) {
			if (ch === '{' || ch === '(') depth++;
			if (ch === '}' || ch === ')') depth--;
		}
		if (depth <= 0) break;
	}
	return names;
}

function rustConst(src: string, name: string): string {
	const m = new RegExp(`pub const ${name}\\s*:[^=]+=\\s*([^;]+);`).exec(src);
	if (!m || !m[1]) throw new Error(`pub const ${name} not found`);
	return m[1].trim();
}

function rustPair(src: string, name: string): [number, number] {
	const v = rustConst(src, name);
	const m = /^\(\s*(-?[\d_.]+)\s*,\s*(-?[\d_.]+)\s*\)$/.exec(v);
	if (!m || !m[1] || !m[2]) throw new Error(`${name} is not a (min, max) pair: ${v}`);
	return [Number(m[1].replace(/_/g, '')), Number(m[2].replace(/_/g, ''))];
}

describe('Rust declarations match the TypeScript lists', () => {
	it.each([
		['Route', ROUTES, rustProtocol],
		['ClientHost', CLIENT_HOSTS, rustProtocol],
		['Suppressed', SUPPRESSIONS, rustProtocol],
		['SolidworksMode', SOLIDWORKS_MODES, rustProtocol],
		['Anchor', ANCHORS, rustProtocol],
		['Font', FONTS, rustProtocol],
		['CalloutSize', CALLOUT_SIZES, rustProtocol],
		['KeyboardSize', KEYBOARD_SIZES, rustProtocol],
		['StackDirection', DIRECTIONS, rustProtocol],
		['KeyboardLayout', KEYBOARD_LAYOUTS, rustProtocol],
		['CursorStyle', CURSOR_STYLES, rustProtocol],
		['CalloutKind', CALLOUT_KINDS, rustEvent],
		['ChipKind', CHIP_KINDS, rustEvent],
		['MouseButton', MOUSE_BUTTONS, rustEvent],
		['Mod', MODIFIERS, rustEvent],
	] as const)('enum %s', (name, list, src) => {
		expect(rustEnumWireNames(src, name)).toEqual([...list]);
	});

	it('ServerMsg and ClientMsg tags', () => {
		expect(rustEnumWireNames(rustProtocol, 'ServerMsg')).toEqual([...SERVER_TAGS]);
		expect(rustEnumWireNames(rustProtocol, 'ClientMsg')).toEqual([...CLIENT_TAGS]);
	});

	it('theme ids', () => {
		const m = /pub const THEMES:[^=]+=\s*\[([^\]]*)\]/.exec(rustProtocol);
		expect(m, 'pub const THEMES').not.toBeNull();
		const rust = [...(m?.[1] ?? '').matchAll(/"([^"]+)"/g)].map((x) => x[1]);
		expect(rust).toEqual([...THEME_IDS]);
	});

	it('limits', () => {
		const pairs: [string, { min: number; max: number }][] = [
			['SCALE', LIMITS.scale],
			['MAX_STACK', LIMITS.maxStack],
			['LINGER_MS', LIMITS.lingerMs],
			['FADE_MS', LIMITS.fadeMs],
			['DECAY_MS', LIMITS.decayMs],
			['OFFSET', LIMITS.offset],
			['CURSOR_SIZE', LIMITS.cursorSize],
		];
		const block = /pub mod limits\s*\{([\s\S]*?)\n\}/.exec(rustProtocol)?.[1] ?? '';
		expect(block, 'pub mod limits').not.toBe('');
		for (const [name, ts] of pairs) {
			expect(rustPair(block, name), name).toEqual([ts.min, ts.max]);
		}
		const declared = [...block.matchAll(/pub const ([A-Z_]+)/g)].map((x) => x[1]).sort();
		expect(declared).toEqual(pairs.map(([n]) => n).sort());
	});

	it('protocol version, port and app name', () => {
		expect(Number(rustConst(rustProtocol, 'PROTOCOL_VERSION'))).toBe(PROTOCOL_VERSION);
		expect(Number(rustConst(rustProtocol, 'DEFAULT_PORT'))).toBe(DEFAULT_PORT);
		expect(rustConst(rustProtocol, 'APP_NAME')).toBe(JSON.stringify(APP_NAME));
	});
});

describe('the Rust source reader in this test', () => {
	it('applies serde renaming rules the way serde does', () => {
		expect(serdeCase('ReleaseAll', 'snake_case')).toBe('release_all');
		expect(serdeCase('TopLeft', 'kebab-case')).toBe('top-left');
		expect(serdeCase('ShareTechMono', 'kebab-case')).toBe('share-tech-mono');
		expect(serdeCase('X1', 'lowercase')).toBe('x1');
		expect(serdeCase('Xl', 'lowercase')).toBe('xl');
		expect(() => serdeCase('A', 'SCREAMING_SNAKE_CASE')).toThrow();
	});

	it('reads variants, renames and nested fields correctly', () => {
		const src = [
			'#[derive(Serialize)]',
			'#[serde(tag = "t", rename_all = "snake_case")]',
			'pub enum Sample {',
			'    /// Doc comment { with braces }',
			'    FirstOne { a: u32, b: Option<String> },',
			'    #[serde(rename = "75")]',
			'    Second,',
			'    Third(Inner),',
			'    FourthItem {',
			'        nested: Vec<(u8, u8)>,',
			'    },',
			'}',
			'pub enum Other { A }',
		].join('\n');
		expect(rustEnumWireNames(src, 'Sample')).toEqual(['first_one', '75', 'third', 'fourth_item']);
		expect(() => rustEnumWireNames(src, 'Missing')).toThrow();
	});

	it('reports a field the contract does not name, a missing one and a wrong type', () => {
		const good = {
			t: 'callout',
			callout: {
				id: 1,
				kind: 'key',
				steps: [[{ key: 'F5', label: 'F5', kind: 'key' }]],
				text: null,
				count: 1,
				caption: null,
				context: null,
				profile: null,
				t_ms: 1,
				updated_ms: 1,
			},
		};
		expect(contractProblems(good)).toEqual([]);
		const extra = JSON.parse(JSON.stringify(good)) as Obj;
		(extra.callout as Obj).extra = 1;
		expect(contractProblems(extra)).toEqual(['callout.callout.extra: not in the contract']);
		const chip = JSON.parse(JSON.stringify(good)) as Obj;
		(((chip.callout as Obj).steps as Obj[][])[0] as Obj[])[0] = { key: 'F5', kind: 'key' };
		expect(contractProblems(chip)).toEqual(['callout.callout.steps[0][0].label: missing']);
		const config = defaultOverlayConfig() as unknown as Obj;
		const badConfig = { t: 'config', config: { ...config, scale: '1' } };
		expect(contractProblems(badConfig as Obj)).toEqual([
			'config.config.scale: string where the contract has number',
		]);
	});
});
