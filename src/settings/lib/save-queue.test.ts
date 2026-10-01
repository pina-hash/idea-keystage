import { describe, expect, it } from 'vitest';
import { SaveQueue, type SaveState, type SaveTimers } from './save-queue';

class FakeTimers implements SaveTimers {
	private next = 1;
	private jobs = new Map<number, { at: number; fn: () => void }>();
	now = 0;
	setTimeout(fn: () => void, ms: number): unknown {
		const id = this.next++;
		this.jobs.set(id, { at: this.now + ms, fn });
		return id;
	}
	clearTimeout(h: unknown): void {
		this.jobs.delete(h as number);
	}
	async advance(ms: number): Promise<void> {
		this.now += ms;
		for (const [id, job] of [...this.jobs].sort((a, b) => a[1].at - b[1].at)) {
			if (job.at <= this.now) {
				this.jobs.delete(id);
				job.fn();
			}
		}
		await flushPromises();
	}
}

async function flushPromises(): Promise<void> {
	for (let i = 0; i < 10; i++) await Promise.resolve();
}

interface Value {
	n: number;
}

function setup(opts: { fail?: () => boolean; clamp?: (v: Value) => Value; slow?: boolean } = {}) {
	const timers = new FakeTimers();
	const sent: Value[] = [];
	const saved: [Value, Value][] = [];
	const states: [SaveState, string | null][] = [];
	let release: (() => void) | null = null;
	const q = new SaveQueue<Value>({
		delayMs: 300,
		timers,
		equal: (a, b) => a.n === b.n,
		describeError: (e) => String(e),
		save: async (v) => {
			sent.push(v);
			if (opts.slow) await new Promise<void>((r) => (release = r));
			if (opts.fail?.()) throw 'disk full';
			return opts.clamp ? opts.clamp(v) : v;
		},
		onSaved: (a, b) => saved.push([a, b]),
		onState: (s, e) => states.push([s, e]),
	});
	q.setBaseline({ n: 0 });
	return { q, timers, sent, saved, states, release: () => release?.() };
}

describe('SaveQueue', () => {
	it('waits for a pause in edits, then saves the latest value once', async () => {
		const { q, timers, sent } = setup();
		q.push({ n: 1 });
		await timers.advance(100);
		q.push({ n: 2 });
		await timers.advance(100);
		q.push({ n: 3 });
		await timers.advance(299);
		expect(sent).toEqual([]);
		await timers.advance(1);
		expect(sent).toEqual([{ n: 3 }]);
		expect(q.current).toBe('saved');
	});

	it('does not save a value equal to what the app confirmed', async () => {
		const { q, timers, sent } = setup();
		q.push({ n: 0 });
		await timers.advance(1000);
		expect(sent).toEqual([]);
		q.push({ n: 4 });
		q.push({ n: 0 });
		await timers.advance(1000);
		expect(sent).toEqual([]);
	});

	it('reports what the app stored so the owner can adopt corrections', async () => {
		const { q, timers, saved } = setup({ clamp: (v) => ({ n: Math.min(v.n, 10) }) });
		q.push({ n: 99 });
		await timers.advance(300);
		expect(saved).toEqual([[{ n: 99 }, { n: 10 }]]);
		// The owner adopts {n: 10}; pushing it back is not another save.
		q.push({ n: 10 });
		await timers.advance(300);
		expect(saved).toHaveLength(1);
	});

	it('keeps edits made during a save and sends them after it', async () => {
		const { q, timers, sent, release } = setup({ slow: true });
		q.push({ n: 1 });
		await timers.advance(300);
		expect(sent).toEqual([{ n: 1 }]);
		q.push({ n: 2 });
		expect(q.current).toBe('saving');
		await timers.advance(300);
		expect(sent).toEqual([{ n: 1 }]);
		release();
		await flushPromises();
		expect(sent).toEqual([{ n: 1 }, { n: 2 }]);
		release();
		await flushPromises();
		expect(q.current).toBe('saved');
		expect(q.dirty).toBe(false);
	});

	it('keeps a failed edit, reports the error and sends it again on retry', async () => {
		let failing = true;
		const { q, timers, sent, states } = setup({ fail: () => failing });
		q.push({ n: 5 });
		await timers.advance(300);
		expect(q.current).toBe('error');
		expect(states.at(-1)).toEqual(['error', 'disk full']);
		expect(q.dirty).toBe(true);
		failing = false;
		await q.retry();
		expect(sent).toEqual([{ n: 5 }, { n: 5 }]);
		expect(q.current).toBe('saved');
	});

	it('flush sends a waiting edit at once', async () => {
		const { q, sent } = setup();
		q.push({ n: 7 });
		await q.flush();
		expect(sent).toEqual([{ n: 7 }]);
		await q.flush();
		expect(sent).toHaveLength(1);
	});

	it('cancel and a new baseline drop a waiting edit', async () => {
		const { q, timers, sent } = setup();
		q.push({ n: 3 });
		q.cancel();
		await timers.advance(1000);
		q.push({ n: 4 });
		q.setBaseline({ n: 4 });
		await timers.advance(1000);
		expect(sent).toEqual([]);
	});
});
