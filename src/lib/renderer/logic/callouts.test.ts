import { describe, expect, it } from 'vitest';
import type { Callout } from '../../protocol';
import { CalloutStack, LEAVE_MS } from './callouts';

function callout(id: number, changes: Partial<Callout> = {}): Callout {
	return {
		id,
		kind: 'combo',
		steps: [
			[
				{ key: 'Ctrl', label: 'Ctrl', kind: 'modifier' },
				{ key: 'Z', label: 'Z', kind: 'key' },
			],
		],
		text: null,
		count: 1,
		caption: null,
		context: null,
		profile: null,
		t_ms: id * 10,
		updated_ms: id * 10,
		...changes,
	};
}

const timing = { maxStack: 3, lingerMs: 1000, fadeMs: 200 };

describe('CalloutStack', () => {
	it('upserts by id: an update replaces content in place and restarts the linger', () => {
		const s = new CalloutStack(timing);
		s.upsert(callout(1), 0);
		s.upsert(callout(2), 100);
		s.upsert(callout(1, { count: 2, updated_ms: 50 }), 600);
		expect(s.items.map((i) => i.id)).toEqual([1, 2]);
		const first = s.items[0];
		expect(first?.callout.count).toBe(2);
		expect(first?.rev).toBe(1);
		expect(first?.fadeAt).toBe(1600);
		expect(first?.removeAt).toBe(1800);
	});

	it('counts repeats exactly as the app sends them', () => {
		const s = new CalloutStack(timing);
		for (let n = 1; n <= 5; n++) s.upsert(callout(9, { count: n, updated_ms: 100 + n }), n * 100);
		expect(s.items).toHaveLength(1);
		expect(s.items[0]?.callout.count).toBe(5);
		expect(s.items[0]?.rev).toBe(4);
	});

	it('ignores a stale update', () => {
		const s = new CalloutStack(timing);
		s.upsert(callout(1, { count: 3, updated_ms: 500 }), 0);
		expect(s.upsert(callout(1, { count: 2, updated_ms: 400 }), 10)).toBe(false);
		expect(s.items[0]?.callout.count).toBe(3);
	});

	it('lingers, then fades, then leaves, on the configured timings', () => {
		const s = new CalloutStack(timing);
		s.upsert(callout(1), 0);
		expect(s.nextDeadline()).toBe(1000);
		expect(s.tick(999)).toBe(false);
		expect(s.items[0]?.phase).toBe('live');
		expect(s.tick(1000)).toBe(true);
		expect(s.items[0]?.phase).toBe('fading');
		expect(s.nextDeadline()).toBe(1200);
		expect(s.tick(1199)).toBe(false);
		expect(s.tick(1200)).toBe(true);
		expect(s.items).toHaveLength(0);
		expect(s.nextDeadline()).toBeNull();
	});

	it('brings a fading callout back when it is updated', () => {
		const s = new CalloutStack(timing);
		s.upsert(callout(1), 0);
		s.tick(1100);
		expect(s.items[0]?.phase).toBe('fading');
		s.upsert(callout(1, { count: 2, updated_ms: 20 }), 1100);
		expect(s.items[0]?.phase).toBe('live');
		expect(s.items[0]?.fadeAt).toBe(2100);
	});

	it('keeps at most max_stack: the least recently updated leave quickly', () => {
		const s = new CalloutStack(timing);
		s.upsert(callout(1), 0);
		s.upsert(callout(2), 10);
		s.upsert(callout(3), 20);
		s.upsert(callout(1, { count: 2, updated_ms: 99 }), 30);
		s.upsert(callout(4), 40);
		const phases = Object.fromEntries(s.items.map((i) => [i.id, i.phase]));
		expect(phases).toEqual({ 1: 'live', 2: 'leaving', 3: 'live', 4: 'live' });
		expect(s.visibleCount).toBe(3);
		const leaving = s.items.find((i) => i.id === 2);
		expect(leaving?.removeAt).toBe(40 + LEAVE_MS);
		s.tick(40 + LEAVE_MS);
		expect(s.items.map((i) => i.id)).toEqual([1, 3, 4]);
	});

	it('trims fading items before live ones', () => {
		const s = new CalloutStack({ maxStack: 2, lingerMs: 100, fadeMs: 1000 });
		s.upsert(callout(1), 0);
		s.upsert(callout(2), 50);
		s.tick(120); // 1 is fading, 2 is live
		s.upsert(callout(3), 130);
		expect(s.items.find((i) => i.id === 1)?.phase).toBe('leaving');
		expect(s.items.find((i) => i.id === 2)?.phase).toBe('live');
	});

	it('shrinks to a smaller max_stack when the config changes', () => {
		const s = new CalloutStack(timing);
		[1, 2, 3].forEach((id, i) => s.upsert(callout(id), i));
		s.setTiming({ ...timing, maxStack: 1 }, 10);
		expect(s.items.filter((i) => i.phase !== 'leaving').map((i) => i.id)).toEqual([3]);
	});

	it('re-times live items when linger changes', () => {
		const s = new CalloutStack(timing);
		s.upsert(callout(1), 0);
		s.setTiming({ ...timing, lingerMs: 3000 }, 5);
		expect(s.items[0]?.fadeAt).toBe(3000);
	});

	it('clears everything at once', () => {
		const s = new CalloutStack(timing);
		s.upsert(callout(1), 0);
		expect(s.clear()).toBe(true);
		expect(s.items).toHaveLength(0);
		expect(s.clear()).toBe(false);
	});

	it('a new arrival with the id of a leaving item takes its place', () => {
		const s = new CalloutStack({ ...timing, maxStack: 1 });
		s.upsert(callout(1), 0);
		s.upsert(callout(2), 10);
		expect(s.items.find((i) => i.id === 1)?.phase).toBe('leaving');
		s.upsert(callout(1, { count: 2, updated_ms: 50 }), 20);
		expect(s.items.filter((i) => i.id === 1)).toHaveLength(1);
		expect(s.items.find((i) => i.id === 1)?.phase).toBe('live');
	});

	it('sanitizes nonsense timings', () => {
		const s = new CalloutStack({ maxStack: 0, lingerMs: Number.NaN, fadeMs: -1 });
		s.upsert(callout(1), 0);
		s.upsert(callout(2), 1);
		expect(s.visibleCount).toBe(1);
		expect(s.items.find((i) => i.id === 2)?.fadeAt).toBe(2201);
	});

	it('loses no callout in a burst of 200 messages at 20 per second', () => {
		const s = new CalloutStack({ maxStack: 4, lingerMs: 2200, fadeMs: 450 });
		const seen = new Set<number>();
		let now = 0;
		let id = 0;
		let count = 1;
		for (let n = 0; n < 200; n++) {
			now = n * 50;
			s.tick(now);
			// Every third message repeats the previous callout (count + 1).
			if (n % 3 === 2 && id > 0) {
				count += 1;
				s.upsert(callout(id, { count, updated_ms: now }), now);
				const shown = s.items.find((i) => i.id === id);
				expect(shown?.callout.count).toBe(count);
				expect(shown?.phase).toBe('live');
			} else {
				id += 1;
				count = 1;
				s.upsert(callout(id, { updated_ms: now }), now);
				expect(s.items.find((i) => i.id === id)?.phase, `id ${id} at ${now}`).toBe('live');
			}
			seen.add(id);
			expect(s.visibleCount).toBeLessThanOrEqual(4);
		}
		expect(seen.size).toBe(134);
		const live = s.items.filter((i) => i.phase !== 'leaving').map((i) => i.id);
		expect(live).toEqual([id - 3, id - 2, id - 1, id]);
		// After the burst everything fades and leaves on time.
		s.tick(now + 2200 + 450);
		expect(s.items).toHaveLength(0);
	});
});
