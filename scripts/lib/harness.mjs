// Shared helpers for scripts/screenshots.mjs and scripts/perf.mjs.
//
// Pure functions (statistics, /proc parsing, demo output parsing, callout
// labels and capture moments) are unit tested in tests/unit/harness.test.ts.
// The process helpers start the front-end build, the demo server and vite
// preview, and read /proc for CPU and memory. Linux only for /proc.
//
// Privacy: the demo feeds synthetic input only. These helpers never print
// key names or callout text; logs carry counts and timings.

import { execFileSync, spawn } from 'node:child_process';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { createServer } from 'node:net';
import { dirname, join, resolve } from 'node:path';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';

export const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

/* ------------------------------------------------------------------------ */
/* Pure helpers                                                              */
/* ------------------------------------------------------------------------ */

/** Nearest-rank percentile of already sorted numbers (p in 0..100). */
export function percentileSorted(sorted, p) {
	if (sorted.length === 0) return null;
	const rank = Math.ceil((p / 100) * sorted.length);
	return sorted[Math.min(sorted.length, Math.max(1, rank)) - 1];
}

/** count, min, median, p95, max and mean of finite numbers (nulls skipped). */
export function summarize(values) {
	const v = values.filter((x) => typeof x === 'number' && Number.isFinite(x));
	const sorted = [...v].sort((a, b) => a - b);
	const round = (x) => (x === null ? null : Math.round(x * 1000) / 1000);
	return {
		count: sorted.length,
		min: round(sorted.length ? sorted[0] : null),
		median: round(percentileSorted(sorted, 50)),
		p95: round(percentileSorted(sorted, 95)),
		max: round(sorted.length ? sorted[sorted.length - 1] : null),
		mean: round(sorted.length ? sorted.reduce((a, b) => a + b, 0) / sorted.length : null),
	};
}

/**
 * One line of `/proc/<pid>/stat`. The command name may hold spaces and
 * parentheses, so fields are counted after the last ')'. Field numbers are
 * those of proc(5): 3 state, 4 ppid, 14 utime, 15 stime, 22 starttime.
 */
export function parseProcStat(text) {
	const close = text.lastIndexOf(')');
	const open = text.indexOf('(');
	if (open < 0 || close < open) throw new Error('not a /proc/<pid>/stat line');
	const rest = text
		.slice(close + 1)
		.trim()
		.split(/\s+/);
	const field = (n) => Number(rest[n - 3]);
	return {
		pid: Number(text.slice(0, open).trim()),
		comm: text.slice(open + 1, close),
		state: rest[0],
		ppid: field(4),
		utime: field(14),
		stime: field(15),
		starttime: field(22),
	};
}

/** `VmRSS` and `VmHWM` (kB) from `/proc/<pid>/status`. */
export function parseProcStatus(text) {
	const kb = (name) => {
		const m = new RegExp(`^${name}:\\s+(\\d+)\\s+kB`, 'm').exec(text);
		return m ? Number(m[1]) : null;
	};
	return { rssKb: kb('VmRSS'), hwmKb: kb('VmHWM') };
}

/** `Pss` (kB) from `/proc/<pid>/smaps_rollup`, or null. */
export function parseSmapsRollup(text) {
	const m = /^Pss:\s+(\d+)\s+kB/m.exec(text);
	return m ? Number(m[1]) : null;
}

/** Percent of one core used between two stat samples taken `wallMs` apart. */
export function cpuPercent(before, after, clkTck, wallMs) {
	if (!before || !after || wallMs <= 0) return null;
	const ticks = after.utime + after.stime - (before.utime + before.stime);
	return (ticks / clkTck / (wallMs / 1000)) * 100;
}

/**
 * The Chromium process type from `/proc/<pid>/cmdline`. Arguments are
 * NUL-separated, except in processes forked from the zygote, which rewrite
 * their command line into one space-separated string.
 */
export function chromiumProcessType(cmdline) {
	const m = /(?:^|[\s\0])--type=([^\s\0]+)/.exec(cmdline);
	return m ? m[1] : 'browser';
}

/**
 * What a demo server output line says. The demo prints (examples/demo.rs):
 * `listening http://127.0.0.1:<port>`, `overlay connected`,
 * `script <name> started`, `burst: sent ...`, `script <name> done: ...`
 * and one machine line `RESULT {json}`.
 */
export function parseDemoLine(line) {
	const listening = /^listening http:\/\/127\.0\.0\.1:(\d+)\s*$/.exec(line);
	if (listening) return { kind: 'listening', port: Number(listening[1]) };
	if (line === 'overlay connected') return { kind: 'connected' };
	const started = /^script (\w+) started$/.exec(line);
	if (started) return { kind: 'started', script: started[1] };
	if (/^burst: sent /.test(line) || /^script \w+ done: /.test(line)) return { kind: 'done' };
	if (line.startsWith('RESULT ')) {
		try {
			return { kind: 'result', result: JSON.parse(line.slice('RESULT '.length)) };
		} catch {
			return { kind: 'other' };
		}
	}
	return { kind: 'other' };
}

/**
 * The renderer's text form of a callout (src/lib/renderer/glyphs.ts
 * `calloutText`, which starts each callout's aria-label). Kept in step with
 * it by tests/unit/harness.test.ts.
 */
export function calloutLabel(c) {
	const steps = c.steps.map((s) => s.map((chip) => chip.label).join(' + ')).filter((s) => s);
	let out;
	if (c.kind === 'text' || c.kind === 'masked') {
		const base = c.kind === 'text' ? (c.text ?? '') : 'Typing';
		out = [base, ...steps].filter((s) => s.length > 0).join(' ');
	} else {
		out = steps.join(', ');
	}
	if (c.count > 1) out += ` x${c.count}`;
	return out;
}

const chipsOf = (c) => c.steps.flat();

/** A mouse drag with a modifier held (`Ctrl + Middle drag`). */
export function isModifierDrag(c) {
	const chips = chipsOf(c);
	return (
		c.kind === 'mouse' &&
		chips.some((chip) => chip.kind === 'modifier') &&
		chips.some((chip) => chip.kind === 'mouse' && /Drag$/.test(chip.key))
	);
}

/** A typed value closed with Enter (`25.4 Enter`). */
export function isTypedValueWithEnter(c) {
	return c.kind === 'text' && chipsOf(c).some((chip) => chip.key === 'Enter');
}

/** A combo of three chips (`Ctrl + Shift + N`). */
export function isThreeKeyCombo(c) {
	return c.kind === 'combo' && c.steps.length === 1 && (c.steps[0] ?? []).length === 3;
}

/**
 * Capture moments for the scripted states, as predicates over the callout
 * messages received so far (oldest first). Each returns true once the
 * moment has been reached; the caller then waits a fixed settle delay.
 */
export const MOMENTS = {
	/** Mid-drag: the first modifier drag has started (cursor, keyboard and mouse busy). */
	drag: (callouts) => callouts.some(isModifierDrag),
	/** A typed value closed with Enter, then two more new callouts (a full stack). */
	typed: (callouts) => {
		const at = callouts.findIndex(isTypedValueWithEnter);
		if (at < 0) return false;
		const before = new Set(callouts.slice(0, at + 1).map((c) => c.id));
		const after = new Set(callouts.slice(at + 1).map((c) => c.id));
		return [...after].filter((id) => !before.has(id)).length >= 2;
	},
	/** Shortcuts-only: masked typing, a repeated undo, then a three-key combo. */
	combo3: (callouts) => callouts.some(isThreeKeyCombo),
};

/** Runs `worker(item, index)` over `items` with at most `limit` running at once. */
export async function pool(items, limit, worker) {
	const results = new Array(items.length);
	let next = 0;
	const run = async () => {
		while (next < items.length) {
			const i = next++;
			results[i] = await worker(items[i], i);
		}
	};
	await Promise.all(Array.from({ length: Math.max(1, Math.min(limit, items.length)) }, run));
	return results;
}

/** The values of `export const NAME = [ 'a', 'b' ] as const;` in a TypeScript file. */
export function readConstList(source, name) {
	const m = new RegExp(`export const ${name}\\b[^=]*=\\s*\\[([\\s\\S]*?)\\]`).exec(source);
	if (!m) throw new Error(`export const ${name} not found`);
	return [...m[1].matchAll(/'([^']*)'|"([^"]*)"/g)].map((x) => x[1] ?? x[2]);
}

export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/* ------------------------------------------------------------------------ */
/* Processes                                                                 */
/* ------------------------------------------------------------------------ */

function run(cmd, args, opts = {}) {
	execFileSync(cmd, args, { cwd: ROOT, stdio: 'inherit', ...opts });
}

/** Builds dist/ when it is missing (or when `force`). */
export function ensureFrontEnd({ force = false } = {}) {
	const ok =
		existsSync(join(ROOT, 'dist/overlay.html')) && existsSync(join(ROOT, 'dist/settings.html'));
	if (ok && !force) return false;
	console.log(force ? 'building the front end (npm run build)' : 'dist/ is missing: npm run build');
	run('npm', ['run', 'build']);
	return true;
}

/** Builds the demo example in release mode and returns the binary path. */
export function buildDemo() {
	console.log('building the demo server (cargo build --release -p keystage-server --example demo)');
	run('cargo', ['build', '--release', '--locked', '-p', 'keystage-server', '--example', 'demo']);
	const meta = JSON.parse(
		execFileSync('cargo', ['metadata', '--format-version', '1', '--no-deps'], {
			cwd: ROOT,
			encoding: 'utf8',
			maxBuffer: 64 * 1024 * 1024,
		})
	);
	const exe = join(meta.target_directory, 'release/examples/demo');
	if (!existsSync(exe)) throw new Error(`the demo binary is not at ${exe}`);
	return exe;
}

/** A free TCP port on 127.0.0.1 (released before it is returned). */
export function freePort() {
	return new Promise((res, rej) => {
		const srv = createServer();
		srv.once('error', rej);
		srv.listen(0, '127.0.0.1', () => {
			const { port } = srv.address();
			srv.close(() => res(port));
		});
	});
}

/** True when nothing accepts connections on 127.0.0.1:port. */
export function portIsFree(port) {
	return new Promise((res) => {
		const srv = createServer();
		srv.once('error', () => res(false));
		srv.listen(port, '127.0.0.1', () => srv.close(() => res(true)));
	});
}

/**
 * Starts the demo server and resolves once it listens. The handle exposes
 * the port, the pid, `waitFor(kind)` for demo output events, `stop()`
 * (SIGSTOP: freezes the input stream for a capture), `kill()`, and `exited`.
 */
export function startDemo(exe, args, { dist = join(ROOT, 'dist'), port = 0 } = {}) {
	const child = spawn(exe, ['--port', String(port), '--dist', dist, ...args], {
		cwd: ROOT,
		stdio: ['ignore', 'pipe', 'pipe'],
	});
	const events = [];
	const waiters = [];
	let stderr = '';
	child.stderr.on('data', (d) => {
		stderr += d.toString();
	});
	const exited = new Promise((res) => child.once('exit', (code, signal) => res({ code, signal })));
	const lines = createInterface({ input: child.stdout });
	lines.on('line', (line) => {
		const ev = { ...parseDemoLine(line), at: performance.now() };
		events.push(ev);
		for (const w of [...waiters]) {
			if (w.kind === ev.kind) {
				waiters.splice(waiters.indexOf(w), 1);
				w.resolve(ev);
			}
		}
	});
	const waitFor = (kind, timeoutMs = 30_000) => {
		const seen = events.find((e) => e.kind === kind);
		if (seen) return Promise.resolve(seen);
		return new Promise((res, rej) => {
			const w = { kind, resolve: res };
			const timer = setTimeout(() => {
				const i = waiters.indexOf(w);
				if (i >= 0) waiters.splice(i, 1);
				rej(new Error(`demo: no "${kind}" within ${timeoutMs} ms. ${stderr.trim()}`));
			}, timeoutMs);
			w.resolve = (ev) => {
				clearTimeout(timer);
				res(ev);
			};
			waiters.push(w);
		});
	};
	const handle = {
		child,
		pid: child.pid,
		port: 0,
		events,
		waitFor,
		exited,
		stderr: () => stderr,
		stop: () => {
			try {
				process.kill(child.pid, 'SIGSTOP');
			} catch {
				// Already gone.
			}
		},
		kill: async () => {
			if (child.exitCode !== null || child.signalCode !== null) return;
			child.kill('SIGKILL');
			await exited;
		},
	};
	const ready = Promise.race([
		waitFor('listening', 20_000),
		exited.then(({ code }) => {
			throw new Error(`demo exited (${code}) before listening: ${stderr.trim()}`);
		}),
	]).then((ev) => {
		handle.port = ev.port;
		return handle;
	});
	return ready;
}

/**
 * Watches a demo server's WebSocket without saying hello, so it never
 * starts the demo script (the page's hello does). Collects callout messages.
 */
export async function observeDemo(port) {
	const callouts = [];
	const ws = new WebSocket(`ws://127.0.0.1:${port}/ws`);
	await new Promise((res, rej) => {
		ws.onopen = res;
		ws.onerror = () => rej(new Error('observer could not connect'));
	});
	ws.onmessage = (ev) => {
		try {
			const msg = JSON.parse(String(ev.data));
			if (msg.t === 'callout' && msg.callout) callouts.push(msg.callout);
		} catch {
			// Not JSON: ignored, like the renderer does.
		}
	};
	const waitUntil = async (predicate, timeoutMs) => {
		const end = performance.now() + timeoutMs;
		while (!predicate(callouts)) {
			if (performance.now() > end) return false;
			await sleep(5);
		}
		return true;
	};
	return { callouts, waitUntil, close: () => ws.close() };
}

/** Starts `vite preview` for dist/ on a free port and resolves with its base URL. */
export async function startPreview() {
	const port = await freePort();
	const vite = join(ROOT, 'node_modules/vite/bin/vite.js');
	const child = spawn(
		process.execPath,
		[vite, 'preview', '--host', '127.0.0.1', '--port', String(port), '--strictPort'],
		{ cwd: ROOT, stdio: ['ignore', 'pipe', 'pipe'] }
	);
	let out = '';
	child.stdout.on('data', (d) => (out += d.toString()));
	child.stderr.on('data', (d) => (out += d.toString()));
	const base = `http://127.0.0.1:${port}`;
	const end = performance.now() + 20_000;
	for (;;) {
		if (child.exitCode !== null) throw new Error(`vite preview exited: ${out.trim()}`);
		try {
			const r = await fetch(`${base}/settings.html`);
			if (r.ok) break;
		} catch {
			// Not listening yet.
		}
		if (performance.now() > end) throw new Error(`vite preview did not start: ${out.trim()}`);
		await sleep(100);
	}
	return {
		base,
		stop: async () => {
			if (child.exitCode !== null) return;
			child.kill('SIGTERM');
			await new Promise((r) => child.once('exit', r));
		},
	};
}

/* ------------------------------------------------------------------------ */
/* /proc                                                                     */
/* ------------------------------------------------------------------------ */

export function clockTicks() {
	try {
		return Number(execFileSync('getconf', ['CLK_TCK'], { encoding: 'utf8' }).trim()) || 100;
	} catch {
		return 100;
	}
}

function readText(path) {
	try {
		return readFileSync(path, 'utf8');
	} catch {
		return null;
	}
}

/** One sample of a process: stat, RSS, PSS, or null when it is gone. */
export function sampleProcess(pid) {
	const stat = readText(`/proc/${pid}/stat`);
	const status = readText(`/proc/${pid}/status`);
	if (!stat || !status) return null;
	const smaps = readText(`/proc/${pid}/smaps_rollup`);
	return {
		at: performance.now(),
		...parseProcStat(stat),
		...parseProcStatus(status),
		pssKb: smaps ? parseSmapsRollup(smaps) : null,
	};
}

/** Every live process with its parent and Chromium type. */
export function listProcesses() {
	const out = [];
	for (const name of readdirSync('/proc')) {
		if (!/^\d+$/.test(name)) continue;
		const stat = readText(`/proc/${name}/stat`);
		if (!stat) continue;
		try {
			const s = parseProcStat(stat);
			const cmd = readText(`/proc/${name}/cmdline`) ?? '';
			out.push({ pid: s.pid, ppid: s.ppid, cpuTicks: s.utime + s.stime, cmdline: cmd });
		} catch {
			// Raced with an exiting process.
		}
	}
	return out;
}

/** The Chromium processes under `rootPid`, with their types. */
export function chromiumTree(rootPid) {
	const all = listProcesses();
	const kids = new Map();
	for (const p of all) {
		if (!kids.has(p.ppid)) kids.set(p.ppid, []);
		kids.get(p.ppid).push(p);
	}
	const out = [];
	const stack = [rootPid];
	while (stack.length) {
		const pid = stack.pop();
		for (const k of kids.get(pid) ?? []) {
			out.push({ ...k, type: chromiumProcessType(k.cmdline) });
			stack.push(k.pid);
		}
	}
	const root = all.find((p) => p.pid === rootPid);
	if (root) out.unshift({ ...root, type: chromiumProcessType(root.cmdline) });
	return out;
}

/**
 * Samples processes every `intervalMs` until `stop()` is called, then
 * returns per-process summaries: CPU percent of one core over the whole
 * window (from the first and last samples), the highest one-interval CPU,
 * and RSS and PSS median and maximum.
 */
export function startSampler(pids, { intervalMs = 1000, clkTck = clockTicks() } = {}) {
	const series = new Map(pids.map((p) => [p, []]));
	const take = () => {
		for (const [pid, list] of series) {
			const s = sampleProcess(pid);
			if (s) list.push(s);
		}
	};
	take();
	const timer = setInterval(take, intervalMs);
	return {
		stop() {
			clearInterval(timer);
			take();
			const out = {};
			for (const [pid, list] of series) out[pid] = summarizeSamples(list, clkTck);
			return out;
		},
	};
}

/** Summary of one process's samples (see startSampler). */
export function summarizeSamples(list, clkTck) {
	if (list.length < 2) return { samples: list.length, error: 'process not sampled twice' };
	const first = list[0];
	const last = list[list.length - 1];
	const intervals = [];
	for (let i = 1; i < list.length; i++) {
		intervals.push(cpuPercent(list[i - 1], list[i], clkTck, list[i].at - list[i - 1].at));
	}
	const rss = summarize(list.map((s) => s.rssKb));
	const pss = summarize(list.map((s) => s.pssKb));
	const round1 = (x) => (x === null ? null : Math.round(x * 10) / 10);
	return {
		samples: list.length,
		seconds: Math.round(last.at - first.at) / 1000,
		cpu_percent_mean: round1(cpuPercent(first, last, clkTck, last.at - first.at)),
		cpu_percent_max_interval: round1(Math.max(...intervals.filter((x) => x !== null))),
		cpu_seconds:
			Math.round(((last.utime + last.stime - first.utime - first.stime) / clkTck) * 1000) / 1000,
		rss_kb_median: rss.median,
		rss_kb_max: rss.max,
		pss_kb_median: pss.median,
		pss_kb_max: pss.max,
	};
}
