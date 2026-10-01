#!/usr/bin/env node
// Host B latency and resource harness (OBS Browser Source path).
//
//   node scripts/perf.mjs [--idle-seconds=60] [--rate=20] [--duration=10]
//                         [--channel=chromium|headless-shell] [--build] [--dist=dist]
//                         [--out=.work/perf]
//
// 1. Builds dist/ if it is missing (--build rebuilds it; --dist=<folder>
//    builds a private copy there instead) and the demo server
//    (cargo build --release -p keystage-server --example demo).
// 2. Idle: the demo serves only its hello (--script idle); /overlay/keys is
//    open in Chromium. CPU and memory of the demo server and of the page's
//    Chromium renderer are sampled from /proc for --idle-seconds.
// 3. Burst: the demo presses --rate modifier combos per second for
//    --duration seconds (--script burst --perf). For every callout the page
//    records the server send time (from the perf stamp), the WebSocket
//    delivery, the DOM change, the first requestAnimationFrame after it and
//    the next one (a paint approximation; see scripts/lib/probe.mjs). The
//    same /proc sampling runs during the burst.
// 4. Writes <out>/results.json and <out>/summary.md and prints the summary.
//    Exits 1 when any callout sent was not rendered (dropped must be 0).
//
// Chromium comes from Playwright (devDependency, pinned); browsers are found
// through PLAYWRIGHT_BROWSERS_PATH or `npx playwright install chromium`.
// Linux only (/proc). Privacy: synthetic input; no key names are printed.

import { mkdirSync, writeFileSync } from 'node:fs';
import { cpus, release, totalmem } from 'node:os';
import { join, resolve } from 'node:path';
import { chromium } from 'playwright';
import {
	ROOT,
	buildDemo,
	chromiumTree,
	clockTicks,
	ensureFrontEnd,
	listProcesses,
	sleep,
	startDemo,
	startSampler,
	summarize,
} from './lib/harness.mjs';
import { installLatencyProbe, probeLatencies } from './lib/probe.mjs';

const args = Object.fromEntries(
	process.argv.slice(2).map((a) => {
		const [k, v] = a.replace(/^--/, '').split('=');
		return [k, v ?? 'true'];
	})
);
const IDLE_SECONDS = Number(args['idle-seconds'] ?? 60);
const RATE = Number(args.rate ?? 20);
const DURATION = Number(args.duration ?? 10);
const CHANNEL = args.channel ?? 'chromium';
const OUT = resolve(ROOT, args.out ?? '.work/perf');
const VIEWPORT = { width: 1920, height: 1080 };

for (const [name, v] of [
	['idle-seconds', IDLE_SECONDS],
	['rate', RATE],
	['duration', DURATION],
]) {
	if (!Number.isFinite(v) || v <= 0) {
		console.error(`--${name} must be a number above 0`);
		process.exit(2);
	}
}

mkdirSync(OUT, { recursive: true });
const DIST = ensureFrontEnd({ force: args.build === 'true', outDir: args.dist ?? 'dist' });
const demoExe = buildDemo();
const clkTck = clockTicks();

const launchOpts = CHANNEL === 'headless-shell' ? {} : { channel: CHANNEL };
const browser = await chromium.launch(launchOpts);
// Playwright starts Chromium as a child of this Node process.
const browserPid =
	listProcesses().find(
		(p) => p.ppid === process.pid && /chrom/i.test(p.cmdline) && !/--type=/.test(p.cmdline)
	)?.pid ?? null;
if (!browserPid) throw new Error('could not find the Chromium browser process');
console.log(`Chromium ${browser.version()} (${CHANNEL}, headless)`);

/** Renderer processes under the browser. */
const renderers = () => chromiumTree(browserPid).filter((p) => p.type === 'renderer');

/**
 * Opens /overlay/keys and returns the page and the renderer that runs it:
 * the renderer whose CPU time grew the most while the page loaded (Chromium
 * also keeps a spare renderer, which stays near zero).
 */
async function openOverlay(context, port, { probe }) {
	const before = new Map(renderers().map((p) => [p.pid, p.cpuTicks]));
	const page = await context.newPage();
	const errors = [];
	page.on('pageerror', (e) => errors.push(String(e)));
	page.on('console', (m) => {
		if (m.type() === 'error') errors.push(m.text());
	});
	if (probe) await page.addInitScript(installLatencyProbe);
	await page.goto(`http://127.0.0.1:${port}/overlay/keys`, { waitUntil: 'load' });
	await page.waitForFunction(
		() => !!document.querySelector('.ks-stage') && !document.querySelector('.ks-waiting'),
		null,
		{ timeout: 15_000 }
	);
	await page.evaluate(() => document.fonts.ready.then(() => true));
	const grown = renderers()
		.map((p) => ({ pid: p.pid, delta: p.cpuTicks - (before.get(p.pid) ?? 0) }))
		.sort((a, b) => b.delta - a.delta);
	const renderer = grown[0] && grown[0].delta > 0 ? grown[0] : null;
	return { page, errors, renderer, otherRenderers: grown.slice(1).map((p) => p.pid) };
}

function processSummary(stats, pids) {
	const out = {};
	for (const [name, pid] of Object.entries(pids)) out[name] = pid ? { pid, ...stats[pid] } : null;
	return out;
}

const context = await browser.newContext({ viewport: VIEWPORT, deviceScaleFactor: 1 });

/* ---------------------------------------------------------------- idle */
console.log(`idle: demo --script idle, /overlay/keys open, sampling ${IDLE_SECONDS} s`);
const idleDemo = await startDemo(demoExe, ['--script', 'idle'], { dist: DIST });
const idle = await openOverlay(context, idleDemo.port, { probe: false });
if (!idle.renderer) throw new Error('could not find the renderer process of the page');
await sleep(2000);
const gpu = chromiumTree(browserPid).find((p) => p.type === 'gpu-process')?.pid ?? null;
const idleSampler = startSampler(
	[idleDemo.pid, idle.renderer.pid, browserPid, gpu].filter(Boolean),
	{ clkTck }
);
await sleep(IDLE_SECONDS * 1000);
const idleStats = idleSampler.stop();
const idleResult = {
	seconds: IDLE_SECONDS,
	processes: processSummary(idleStats, {
		demo_server: idleDemo.pid,
		renderer: idle.renderer.pid,
		browser_main: browserPid,
		gpu_process: gpu,
	}),
	other_renderer_pids: idle.otherRenderers,
	page_errors: idle.errors.length,
};
await idle.page.close();
await idleDemo.kill();

/* --------------------------------------------------------------- burst */
console.log(`burst: demo --script burst --perf --rate ${RATE} --duration ${DURATION}`);
const burstDemo = await startDemo(
	demoExe,
	['--script', 'burst', '--perf', '--rate', String(RATE), '--duration', String(DURATION)],
	{ dist: DIST }
);
const burst = await openOverlay(context, burstDemo.port, { probe: true });
if (!burst.renderer) throw new Error('could not find the renderer process of the page');
await burstDemo.waitFor('started', 15_000);
const burstSampler = startSampler(
	[burstDemo.pid, burst.renderer.pid, browserPid, gpu].filter(Boolean),
	{ intervalMs: 500, clkTck }
);
await burstDemo.waitFor('done', (DURATION + 30) * 1000);
const burstStats = burstSampler.stop();
const result = (await burstDemo.waitFor('result', 10_000)).result;
// The last callouts reach the page within a frame or two; give them a second.
await sleep(1000);
const probe = await burst.page.evaluate(() => {
	const p = window.__ksProbe;
	return p
		? {
				stamps: p.stamps,
				callouts: p.callouts,
				unpaired: p.unpaired,
				records: p.records,
				pending: p.pending.length,
				longTasks: p.longTasks,
				dateSkewMs: p.dateSkewMs,
			}
		: null;
});
await burst.page.close();
await burstDemo.kill();
await context.close();
await browser.close();

if (!probe) throw new Error('the latency probe did not install in the page');

const lat = probeLatencies(probe.records);
const pick = (k) => summarize(lat.map((x) => x[k]));
const sent = result.perf;
const rendered = probe.records.filter((r) => r.seq !== null).length;
const dropped = sent - rendered;
const results = {
	when: new Date().toISOString(),
	machine: {
		cpus: cpus().length,
		cpu_model: cpus()[0]?.model ?? null,
		memory_gb: Math.round((totalmem() / 2 ** 30) * 10) / 10,
		kernel: release(),
		node: process.version,
	},
	browser: { engine: 'Chromium', version: browser.version(), channel: CHANNEL, headless: true },
	demo: {
		binary: demoExe,
		script: 'burst',
		rate_per_second: RATE,
		duration_seconds: DURATION,
		result,
	},
	instrument: {
		clock:
			'Server: Unix ms (SystemTime) in the perf stamp. Page: performance.timeOrigin + performance.now(). Same machine, so the two are one clock up to Chromium timer coarsening (about 0.1 ms).',
		transport_ms: 'perf stamp sent_ms to the WebSocket message event in the page',
		dom_ms: 'sent_ms to the MutationObserver callback that saw the callout in the DOM',
		frame_ms: 'sent_ms to the first requestAnimationFrame after the DOM change',
		next_frame_ms:
			'sent_ms to the second requestAnimationFrame: the frame with the callout has been produced (paint approximation, not display scan-out)',
		page_to_frame_ms:
			'WebSocket message event to the first requestAnimationFrame (page-side only, no cross-process clock)',
		date_minus_perf_clock_ms: probe.dateSkewMs,
	},
	latency: {
		counts: {
			sent,
			received_stamps: probe.stamps,
			received_callouts: probe.callouts,
			unpaired_callouts: probe.unpaired,
			rendered,
			dropped,
			still_pending: probe.pending,
		},
		transport_ms: pick('transport'),
		dom_ms: pick('dom'),
		frame_ms: pick('frame'),
		next_frame_ms: pick('nextFrame'),
		page_to_frame_ms: pick('pageToFrame'),
		long_tasks: probe.longTasks,
		page_errors: burst.errors.length,
	},
	idle: idleResult,
	burst: {
		seconds_sampled: burstStats[burstDemo.pid]?.seconds ?? null,
		processes: processSummary(burstStats, {
			demo_server: burstDemo.pid,
			renderer: burst.renderer.pid,
			browser_main: browserPid,
			gpu_process: gpu,
		}),
		other_renderer_pids: burst.otherRenderers,
	},
};

writeFileSync(join(OUT, 'results.json'), JSON.stringify(results, null, '\t') + '\n');
const summary = renderSummary(results);
writeFileSync(join(OUT, 'summary.md'), summary);
console.log('\n' + summary);
console.log(`wrote ${join(OUT, 'results.json')} and summary.md`);
if (dropped !== 0 || sent === 0) {
	console.error(`FAIL: ${sent} callouts sent, ${rendered} rendered, ${dropped} dropped`);
	process.exit(1);
}

function renderSummary(r) {
	const ms = (s) =>
		s.count ? `${s.median.toFixed(2)} / ${s.p95.toFixed(2)} / ${s.max.toFixed(2)}` : 'none';
	const proc = (p) =>
		p
			? `${p.cpu_percent_mean}% mean, ${p.cpu_percent_max_interval}% peak interval; RSS ${Math.round(
					p.rss_kb_median / 1024
				)} MiB median, ${Math.round(p.rss_kb_max / 1024)} MiB max${
					p.pss_kb_median !== null ? `; PSS ${Math.round(p.pss_kb_median / 1024)} MiB median` : ''
				}`
			: 'not sampled';
	const L = r.latency;
	return [
		'# IDEA Keystage Host B performance',
		'',
		`Run ${r.when} on ${r.machine.cpus} CPUs (${r.machine.cpu_model}), Linux ${r.machine.kernel}, ${r.browser.engine} ${r.browser.version} headless (${r.browser.channel}), demo burst at ${r.demo.rate_per_second} keys per second for ${r.demo.duration_seconds} s, /overlay/keys at 1920x1080.`,
		'',
		'## Latency (ms, median / p95 / max)',
		'',
		`| Measure | ms |`,
		`| --- | --- |`,
		`| Server send to WebSocket delivery | ${ms(L.transport_ms)} |`,
		`| Server send to DOM updated | ${ms(L.dom_ms)} |`,
		`| Server send to first animation frame after the DOM update | ${ms(L.frame_ms)} |`,
		`| Server send to the next animation frame (paint approximation) | ${ms(L.next_frame_ms)} |`,
		`| Page only: delivery to first animation frame | ${ms(L.page_to_frame_ms)} |`,
		'',
		`Callouts: ${L.counts.sent} sent, ${L.counts.rendered} rendered, ${L.counts.dropped} dropped (${L.counts.unpaired_callouts} without a perf stamp, ${L.counts.still_pending} never seen in the DOM). Long tasks: ${
			L.long_tasks
				? `${L.long_tasks.count} (${Math.round(L.long_tasks.totalMs)} ms total)`
				: 'not observable'
		}.`,
		'',
		'The "paint approximation" is the second requestAnimationFrame after the DOM change: the frame holding the callout has been produced by then. Headless Chromium has no display, so this is not the time pixels reach a screen, and OBS adds its own frame pacing on top. Chromium draws at 60 frames per second here, so a DOM change waits up to 16.7 ms for its frame; the burst period (one key every 50 ms at 20 per second) is a whole number of frames, so every press lands at a similar point in the frame.',
		'',
		`## Resources (percent of one core; /proc, ${r.idle.seconds} s idle, ${r.burst.seconds_sampled} s burst)`,
		'',
		'| Process | Idle | Burst |',
		'| --- | --- | --- |',
		`| Demo server | ${proc(r.idle.processes.demo_server)} | ${proc(r.burst.processes.demo_server)} |`,
		`| Chromium renderer (the page) | ${proc(r.idle.processes.renderer)} | ${proc(r.burst.processes.renderer)} |`,
		`| Chromium GPU process | ${proc(r.idle.processes.gpu_process)} | ${proc(r.burst.processes.gpu_process)} |`,
		`| Chromium browser process | ${proc(r.idle.processes.browser_main)} | ${proc(r.burst.processes.browser_main)} |`,
		'',
		'The demo server is the keystage-server crate with the real keystage-core Processor fed by a script; the app adds the Windows hooks and the native overlay, which this Linux run cannot measure.',
		'',
	].join('\n');
}
