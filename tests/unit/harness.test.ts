/// <reference types="node" />
/**
 * Tests for the pure parts of the screenshot and performance harnesses
 * (scripts/lib/harness.mjs) and for the in-page latency probe
 * (scripts/lib/probe.mjs), which runs here under jsdom against a fake
 * WebSocket and a hand-built callout stack.
 */
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import {
	MOMENTS,
	calloutLabel,
	chromiumProcessType,
	cpuPercent,
	isModifierDrag,
	isThreeKeyCombo,
	isTypedValueWithEnter,
	parseDemoLine,
	parseProcStat,
	parseProcStatus,
	parseSmapsRollup,
	percentileSorted,
	pool,
	readConstList,
	summarize,
	summarizeSamples,
} from '../../scripts/lib/harness.mjs';
import { installLatencyProbe, probeLatencies } from '../../scripts/lib/probe.mjs';
import { calloutText } from '../../src/lib/renderer/glyphs';
import type { Callout, Chip } from '../../src/lib/protocol';

// Under jsdom import.meta.url is not a file URL; vitest provides __dirname.
const ROOT = resolve(__dirname, '../..');
const FIXTURES = resolve(ROOT, 'crates/keystage-core/tests/fixtures/protocol');

const chip = (key: string, kind: Chip['kind'] = 'key', label = key): Chip => ({ key, label, kind });
const callout = (over: Partial<Callout>): Callout => ({
	id: 1,
	kind: 'combo',
	steps: [[chip('Ctrl', 'modifier'), chip('B')]],
	text: null,
	count: 1,
	caption: null,
	context: null,
	profile: null,
	t_ms: 0,
	updated_ms: 0,
	...over,
});

describe('statistics', () => {
	it('nearest-rank percentiles', () => {
		const v = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
		expect(percentileSorted(v, 50)).toBe(5);
		expect(percentileSorted(v, 95)).toBe(10);
		expect(percentileSorted(v, 0)).toBe(1);
		expect(percentileSorted([], 50)).toBeNull();
		const hundred = Array.from({ length: 100 }, (_, i) => i + 1);
		expect(percentileSorted(hundred, 95)).toBe(95);
	});

	it('summarize skips nulls and non-finite values', () => {
		expect(summarize([3, null, 1, Number.NaN, 2])).toEqual({
			count: 3,
			min: 1,
			median: 2,
			p95: 3,
			max: 3,
			mean: 2,
		});
		expect(summarize([])).toEqual({
			count: 0,
			min: null,
			median: null,
			p95: null,
			max: null,
			mean: null,
		});
	});
});

describe('/proc parsing', () => {
	// proc(5) layout: pid (comm) state ppid pgrp session tty tpgid flags
	// minflt cminflt majflt cmajflt utime stime ... starttime is field 22.
	const stat = (comm: string) =>
		`4242 (${comm}) S 17 4242 4242 0 -1 4194560 1200 0 3 0 250 75 0 0 20 0 9 0 123456 5000000 1500 18446744073709551615`;

	it('reads utime, stime, ppid and starttime', () => {
		expect(parseProcStat(stat('demo'))).toEqual({
			pid: 4242,
			comm: 'demo',
			state: 'S',
			ppid: 17,
			utime: 250,
			stime: 75,
			starttime: 123456,
		});
	});

	it('copes with spaces and parentheses in the command name', () => {
		const s = parseProcStat(stat('Web Content (x) )'));
		expect(s.comm).toBe('Web Content (x) )');
		expect(s.utime).toBe(250);
		expect(() => parseProcStat('garbage')).toThrow();
	});

	it('reads VmRSS, VmHWM and Pss', () => {
		const status = 'Name:\tdemo\nVmHWM:\t   9000 kB\nVmRSS:\t   6144 kB\nThreads:\t9\n';
		expect(parseProcStatus(status)).toEqual({ rssKb: 6144, hwmKb: 9000 });
		expect(parseProcStatus('Name:\tx\n')).toEqual({ rssKb: null, hwmKb: null });
		expect(parseSmapsRollup('Rss:   100 kB\nPss:    42 kB\n')).toBe(42);
		expect(parseSmapsRollup('')).toBeNull();
	});

	it('computes percent of one core', () => {
		const a = { utime: 100, stime: 50 };
		const b = { utime: 150, stime: 100 };
		// 100 ticks at 100 per second over 2 s: half a core.
		expect(cpuPercent(a, b, 100, 2000)).toBe(50);
		expect(cpuPercent(a, b, 100, 0)).toBeNull();
		expect(cpuPercent(null, b, 100, 1000)).toBeNull();
	});

	it('summarizes samples over a window', () => {
		const samples = [
			{ at: 0, utime: 0, stime: 0, rssKb: 1000, pssKb: 500 },
			{ at: 1000, utime: 10, stime: 0, rssKb: 1100, pssKb: 520 },
			{ at: 2000, utime: 10, stime: 30, rssKb: 1050, pssKb: null },
		];
		const s = summarizeSamples(samples, 100);
		expect(s.samples).toBe(3);
		expect(s.seconds).toBe(2);
		expect(s.cpu_percent_mean).toBe(20);
		expect(s.cpu_percent_max_interval).toBe(30);
		expect(s.cpu_seconds).toBe(0.4);
		expect(s.rss_kb_median).toBe(1050);
		expect(s.rss_kb_max).toBe(1100);
		expect(s.pss_kb_max).toBe(520);
		expect(summarizeSamples(samples.slice(0, 1), 100)).toEqual({
			samples: 1,
			error: 'process not sampled twice',
		});
	});

	it('names Chromium process types, NUL-separated or rewritten by the zygote', () => {
		expect(chromiumProcessType('/opt/chrome\0--headless\0--remote-debugging-pipe')).toBe('browser');
		expect(chromiumProcessType('/opt/chrome --type=renderer --lang=en-US')).toBe('renderer');
		expect(chromiumProcessType('/opt/chrome\0--type=gpu-process\0--no-sandbox')).toBe(
			'gpu-process'
		);
		expect(chromiumProcessType('/opt/chrome --subtype=x --type=utility')).toBe('utility');
	});
});

describe('demo server output', () => {
	it('recognizes every line the harness waits for', () => {
		expect(parseDemoLine('listening http://127.0.0.1:47820')).toEqual({
			kind: 'listening',
			port: 47820,
		});
		expect(parseDemoLine('overlay connected')).toEqual({ kind: 'connected' });
		expect(parseDemoLine('script burst started')).toEqual({ kind: 'started', script: 'burst' });
		expect(parseDemoLine('burst: sent 401 messages (200 callout messages)').kind).toBe('done');
		expect(
			parseDemoLine('script general done: sent 9 messages (9 callout messages) in 3.0 s').kind
		).toBe('done');
		expect(parseDemoLine('RESULT {"script":"burst","perf":200}')).toEqual({
			kind: 'result',
			result: { script: 'burst', perf: 200 },
		});
		expect(parseDemoLine('RESULT {broken').kind).toBe('other');
		expect(parseDemoLine('  http://127.0.0.1:47820/overlay/keys').kind).toBe('other');
	});

	it('matches the demo binary source, so a reworded line fails here first', () => {
		const demo = readFileSync(resolve(ROOT, 'crates/keystage-server/examples/demo.rs'), 'utf8');
		expect(demo).toContain('println!("listening http://127.0.0.1:{port}")');
		expect(demo).toContain('println!("overlay connected")');
		expect(demo).toContain('println!("script {name} started")');
		expect(demo).toContain('"burst: sent {} messages');
		expect(demo).toContain('"script {name} done: sent');
		expect(demo).toContain('"RESULT {{');
		expect(demo).toContain('{"t":"perf","seq":{},"sent_ms":{:.3},"id":{},"count":{}}');
	});
});

describe('callout labels and capture moments', () => {
	const samples = readdirSync(FIXTURES)
		.filter((f) => f.startsWith('callout'))
		.map(
			(f) =>
				(JSON.parse(readFileSync(resolve(FIXTURES, f), 'utf8')) as { callout: Callout }).callout
		);

	it('calloutLabel equals the renderer calloutText for every core sample and edge case', () => {
		expect(samples.length).toBeGreaterThan(0);
		const extra = [
			callout({ count: 4 }),
			callout({ kind: 'text', text: '25.4', steps: [[chip('Enter')]] }),
			callout({ kind: 'text', text: null, steps: [] }),
			callout({ kind: 'masked', steps: [[chip('Tab')]] }),
			callout({ kind: 'chord', steps: [[chip('Alt', 'modifier')], [chip('F')], [chip('C')]] }),
			callout({ steps: [[], [chip('A')]] }),
		];
		for (const c of [...samples, ...extra])
			expect(calloutLabel(c), JSON.stringify(c)).toBe(calloutText(c));
	});

	it('classifies the scripted moments', () => {
		const drag = callout({
			kind: 'mouse',
			steps: [[chip('Ctrl', 'modifier'), chip('MiddleDrag', 'mouse')]],
		});
		const plainDrag = callout({ kind: 'mouse', steps: [[chip('MiddleDrag', 'mouse')]] });
		const wheel = callout({
			kind: 'mouse',
			steps: [[chip('Ctrl', 'modifier'), chip('WheelUp', 'mouse')]],
		});
		expect(isModifierDrag(drag)).toBe(true);
		expect(isModifierDrag(plainDrag)).toBe(false);
		expect(isModifierDrag(wheel)).toBe(false);
		const typed = callout({ kind: 'text', text: '25.4', steps: [[chip('Enter')]] });
		expect(isTypedValueWithEnter(typed)).toBe(true);
		expect(isTypedValueWithEnter(callout({ kind: 'text', text: '25.4', steps: [] }))).toBe(false);
		const three = callout({
			steps: [[chip('Ctrl', 'modifier'), chip('Shift', 'modifier'), chip('N')]],
		});
		expect(isThreeKeyCombo(three)).toBe(true);
		expect(isThreeKeyCombo(callout({}))).toBe(false);
	});

	it('the typed moment waits for two new callouts after the value', () => {
		const typed = callout({ id: 7, kind: 'text', text: '25.4', steps: [[chip('Enter')]] });
		const seq = [callout({ id: 6 }), callout({ id: 7, kind: 'text', text: '2', steps: [] }), typed];
		expect(MOMENTS.typed(seq)).toBe(false);
		seq.push(callout({ id: 7, kind: 'text', text: '25.4', steps: [[chip('Enter')]] }));
		seq.push(callout({ id: 8 }));
		expect(MOMENTS.typed(seq)).toBe(false);
		seq.push(callout({ id: 8, count: 2 }));
		expect(MOMENTS.typed(seq)).toBe(false);
		seq.push(callout({ id: 9 }));
		expect(MOMENTS.typed(seq)).toBe(true);
		expect(MOMENTS.drag(seq)).toBe(false);
		expect(MOMENTS.combo3(seq)).toBe(false);
	});
});

describe('small utilities', () => {
	it('pool never runs more than the limit and keeps result order', async () => {
		let running = 0;
		let peak = 0;
		const out = await pool([5, 1, 4, 2, 3], 2, async (n: number) => {
			running++;
			peak = Math.max(peak, running);
			await new Promise((r) => setTimeout(r, n));
			running--;
			return n * 10;
		});
		expect(out).toEqual([50, 10, 40, 20, 30]);
		expect(peak).toBe(2);
		expect(await pool([], 4, async () => 1)).toEqual([]);
	});

	it('readConstList reads the value lists the screenshot harness iterates', () => {
		const protocol = readFileSync(resolve(ROOT, 'src/lib/protocol.ts'), 'utf8');
		expect(readConstList(protocol, 'ROUTES')).toEqual([
			'keys',
			'keyboard',
			'mouse',
			'cursor',
			'all',
		]);
		expect(readConstList(protocol, 'THEME_IDS')).toContain('idea');
		const nav = readFileSync(resolve(ROOT, 'src/settings/lib/nav.ts'), 'utf8');
		expect(readConstList(nav, 'PAGES')).toContain('obs');
		const fixtures = readFileSync(resolve(ROOT, 'src/settings/mock/fixtures.ts'), 'utf8');
		expect(readConstList(fixtures, 'SCENARIOS')).toContain('first-run');
		expect(() => readConstList(protocol, 'NOPE')).toThrow();
		expect(readConstList(`export const X = ["a", 'b'] as const;`, 'X')).toEqual(['a', 'b']);
	});
});

/* ------------------------------------------------------------------------ */
/* The in-page probe under jsdom                                            */
/* ------------------------------------------------------------------------ */

interface ProbeRecord {
	seq: number | null;
	sent: number | null;
	recv: number;
	dom: number | null;
	raf1: number | null;
	raf2: number | null;
	update: boolean;
}
interface Probe {
	stamps: number;
	callouts: number;
	unpaired: number;
	records: ProbeRecord[];
	pending: unknown[];
}

class FakeSocket extends EventTarget {
	static instances: FakeSocket[] = [];
	static OPEN = 1;
	readonly url: string;
	constructor(url: string) {
		super();
		this.url = url;
		FakeSocket.instances.push(this);
	}
	deliver(msg: unknown): void {
		this.dispatchEvent(new MessageEvent('message', { data: JSON.stringify(msg) }));
	}
}

const win = window as unknown as {
	WebSocket: unknown;
	__ksProbe?: Probe;
	requestAnimationFrame: (cb: FrameRequestCallback) => number;
};

describe('latency probe', () => {
	let original: unknown;
	let stack: HTMLElement;
	const frames: FrameRequestCallback[] = [];
	let originalRaf: typeof win.requestAnimationFrame;
	const flushFrame = () => {
		const run = frames.splice(0);
		for (const f of run) f(performance.now());
	};
	const mutations = () => new Promise((r) => setTimeout(r, 0));

	beforeEach(() => {
		original = win.WebSocket;
		originalRaf = win.requestAnimationFrame;
		win.requestAnimationFrame = (cb) => frames.push(cb);
		win.WebSocket = FakeSocket;
		delete win.__ksProbe;
		FakeSocket.instances = [];
		document.body.innerHTML = '<div class="ks-stack"></div>';
		stack = document.querySelector('.ks-stack') as HTMLElement;
		installLatencyProbe();
	});
	afterEach(() => {
		win.WebSocket = original;
		win.requestAnimationFrame = originalRaf;
		frames.length = 0;
		delete win.__ksProbe;
	});

	const addCallout = (label: string) => {
		const slot = document.createElement('div');
		slot.className = 'ks-slot';
		const el = document.createElement('div');
		el.className = 'ks-callout';
		el.setAttribute('aria-label', label);
		slot.appendChild(el);
		stack.appendChild(slot);
		return el;
	};
	const combo = (id: number, key: string, count = 1) => ({
		t: 'callout',
		callout: callout({ id, count, steps: [[chip('Ctrl', 'modifier'), chip(key)]] }),
	});
	const perf = (seq: number, id: number, count = 1) => ({
		t: 'perf',
		seq,
		sent_ms: 1000 + seq,
		id,
		count,
	});

	it('wraps WebSocket so the page still gets a real socket', () => {
		const Ctor = win.WebSocket as new (url: string) => FakeSocket;
		const s = new Ctor('ws://127.0.0.1:1/ws');
		expect(s).toBeInstanceOf(FakeSocket);
		expect(s.url).toBe('ws://127.0.0.1:1/ws');
		expect((win.WebSocket as { OPEN: number }).OPEN).toBe(1);
	});

	it('pairs stamps with callouts and times the DOM change and two frames', async () => {
		const Ctor = win.WebSocket as new (url: string) => FakeSocket;
		const s = new Ctor('ws://x/ws');
		s.deliver({ t: 'hello' });
		s.deliver(perf(1, 5));
		s.deliver(combo(5, 'B'));
		const probe = win.__ksProbe as Probe;
		expect(probe.stamps).toBe(1);
		expect(probe.callouts).toBe(1);
		expect(probe.unpaired).toBe(0);
		expect(probe.records).toHaveLength(0);

		// A different callout appearing does not count.
		addCallout('Ctrl + C');
		await mutations();
		expect(probe.records).toHaveLength(0);

		addCallout('Ctrl + B. Rebuild caption');
		await mutations();
		expect(probe.records).toHaveLength(1);
		const r = probe.records[0] as ProbeRecord;
		expect(r.seq).toBe(1);
		expect(r.sent).toBe(1001);
		expect(r.dom).not.toBeNull();
		expect(r.raf1).toBeNull();
		flushFrame();
		expect(r.raf1).not.toBeNull();
		expect(r.raf2).toBeNull();
		flushFrame();
		expect(r.raf2).not.toBeNull();
		const [lat] = probeLatencies(probe.records);
		expect(lat?.seq).toBe(1);
		expect(lat?.frame).toBeGreaterThanOrEqual(lat?.dom ?? 0);
		expect(lat?.nextFrame).toBeGreaterThanOrEqual(lat?.frame ?? 0);
	});

	it('an update matches only through a label change on the existing callout', async () => {
		const Ctor = win.WebSocket as new (url: string) => FakeSocket;
		const s = new Ctor('ws://x/ws');
		s.deliver(perf(1, 5));
		s.deliver(combo(5, 'Z'));
		const el = addCallout('Ctrl + Z');
		await mutations();
		s.deliver(perf(2, 5, 2));
		s.deliver(combo(5, 'Z', 2));
		const probe = win.__ksProbe as Probe;
		expect(probe.records).toHaveLength(1);
		el.setAttribute('aria-label', 'Ctrl + Z x2');
		await mutations();
		expect(probe.records.map((x) => x.seq)).toEqual([1, 2]);
		expect(probe.records[1]?.update).toBe(true);
		expect(probe.pending).toHaveLength(0);
	});

	it('a new callout with the label of one already shown needs a new element', async () => {
		const Ctor = win.WebSocket as new (url: string) => FakeSocket;
		const s = new Ctor('ws://x/ws');
		s.deliver(perf(1, 1));
		s.deliver(combo(1, 'A'));
		const first = addCallout('Ctrl + A');
		await mutations();
		s.deliver(perf(2, 2));
		s.deliver(combo(2, 'A'));
		// The old element changing does not satisfy a new id.
		first.setAttribute('aria-label', 'Ctrl + A');
		first.setAttribute('aria-label', 'Ctrl + A. x');
		await mutations();
		const probe = win.__ksProbe as Probe;
		expect(probe.records).toHaveLength(1);
		addCallout('Ctrl + A');
		await mutations();
		expect(probe.records.map((x) => x.seq)).toEqual([1, 2]);
	});

	it('counts callouts without a stamp and leaves unseen ones pending', async () => {
		const Ctor = win.WebSocket as new (url: string) => FakeSocket;
		const s = new Ctor('ws://x/ws');
		s.deliver(perf(1, 9));
		s.deliver(combo(3, 'Q'));
		s.deliver('not json' as unknown);
		const probe = win.__ksProbe as Probe;
		expect(probe.unpaired).toBe(1);
		expect(probe.pending).toHaveLength(1);
		const lat = probeLatencies([
			{ seq: null, sent: null, recv: 5, dom: 6, raf1: 7, raf2: 8, update: false },
		]);
		expect(lat[0]).toEqual({
			seq: null,
			transport: null,
			dom: null,
			frame: null,
			nextFrame: null,
			pageToFrame: 2,
		});
	});
});
