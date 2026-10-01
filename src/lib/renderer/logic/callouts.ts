/**
 * The callout stack, as pure logic with an injected clock.
 *
 * keystage-core decides WHAT a callout says; this decides how long it stays
 * on screen. Rules (ARCHITECTURE.md sections 4 and 5, plus the renderer task):
 *   - `callout` messages upsert by id. An update replaces the content, keeps
 *     the item's place in the stack and restarts its linger.
 *   - A callout lingers `linger_ms` after its last update, then fades for
 *     `fade_ms`, then is removed. An update while fading brings it back.
 *   - At most `max_stack` items are shown; when a new one arrives the least
 *     recently updated items over the limit leave quickly (`LEAVE_MS`).
 *   - Items are kept in arrival order (oldest first). The view decides which
 *     end is the anchor edge from `direction`.
 *   - A stale update (older `updated_ms` than the one shown) is ignored.
 */
import type { Callout } from '../../protocol';

export type CalloutPhase = 'live' | 'fading' | 'leaving';

export interface CalloutItem {
	/** Callout id from the wire. */
	id: number;
	callout: Callout;
	phase: CalloutPhase;
	/** Local clock (ms) when this id first arrived. */
	bornAt: number;
	/** Local clock (ms) of the last update. */
	updatedAt: number;
	/** When the linger ends and the fade starts. */
	fadeAt: number;
	/** When the item leaves the stack. */
	removeAt: number;
	/** Increments on every update, so the view can replay an update pulse. */
	rev: number;
}

export interface StackTiming {
	maxStack: number;
	lingerMs: number;
	fadeMs: number;
}

/** How long an item pushed out by `max_stack` takes to leave. */
export const LEAVE_MS = 160;

export class CalloutStack {
	private list: CalloutItem[] = [];
	private timing: StackTiming;

	constructor(timing: StackTiming) {
		this.timing = sanitize(timing);
	}

	get items(): readonly CalloutItem[] {
		return this.list;
	}

	/** Items still counting toward max_stack (live or fading). */
	get visibleCount(): number {
		let n = 0;
		for (const it of this.list) if (it.phase !== 'leaving') n++;
		return n;
	}

	setTiming(timing: StackTiming, now: number): void {
		const next = sanitize(timing);
		const changed = next.lingerMs !== this.timing.lingerMs || next.fadeMs !== this.timing.fadeMs;
		this.timing = next;
		if (changed) {
			for (const it of this.list) {
				if (it.phase === 'live') {
					it.fadeAt = it.updatedAt + next.lingerMs;
					it.removeAt = it.fadeAt + next.fadeMs;
				} else if (it.phase === 'fading') {
					it.removeAt = Math.min(it.removeAt, now + next.fadeMs);
				}
			}
		}
		this.trim(now);
		this.tick(now);
	}

	/** Returns true when the stack changed. */
	upsert(callout: Callout, now: number): boolean {
		const { lingerMs, fadeMs } = this.timing;
		const existing = this.list.find((it) => it.id === callout.id && it.phase !== 'leaving');
		if (existing) {
			if (callout.updated_ms < existing.callout.updated_ms) return false;
			existing.callout = callout;
			existing.phase = 'live';
			existing.updatedAt = now;
			existing.fadeAt = now + lingerMs;
			existing.removeAt = existing.fadeAt + fadeMs;
			existing.rev += 1;
			this.trim(now);
			return true;
		}
		// A leaving item with the same id is replaced by the new arrival.
		this.list = this.list.filter((it) => it.id !== callout.id);
		this.list.push({
			id: callout.id,
			callout,
			phase: 'live',
			bornAt: now,
			updatedAt: now,
			fadeAt: now + lingerMs,
			removeAt: now + lingerMs + fadeMs,
			rev: 0,
		});
		this.trim(now);
		return true;
	}

	/** Removes everything at once (the `clear` message). */
	clear(): boolean {
		if (this.list.length === 0) return false;
		this.list = [];
		return true;
	}

	/**
	 * Advances phases to `now`. Returns true when anything changed. Call it
	 * at (or after) `nextDeadline()`.
	 */
	tick(now: number): boolean {
		let changed = false;
		const kept: CalloutItem[] = [];
		for (const it of this.list) {
			if (now >= it.removeAt) {
				changed = true;
				continue;
			}
			if (it.phase === 'live' && now >= it.fadeAt) {
				it.phase = 'fading';
				changed = true;
			}
			kept.push(it);
		}
		if (changed) this.list = kept;
		return changed;
	}

	/** The next time `tick` will change something, or null when idle. */
	nextDeadline(): number | null {
		let next: number | null = null;
		for (const it of this.list) {
			const t = it.phase === 'live' ? it.fadeAt : it.removeAt;
			if (next === null || t < next) next = t;
		}
		return next;
	}

	private trim(now: number): void {
		const max = this.timing.maxStack;
		let excess = this.visibleCount - max;
		if (excess <= 0) return;
		// Fading items go first, then the least recently updated live ones.
		const candidates = this.list
			.filter((it) => it.phase !== 'leaving')
			.sort((a, b) => {
				const fa = a.phase === 'fading' ? 0 : 1;
				const fb = b.phase === 'fading' ? 0 : 1;
				if (fa !== fb) return fa - fb;
				if (a.updatedAt !== b.updatedAt) return a.updatedAt - b.updatedAt;
				return a.bornAt - b.bornAt;
			});
		for (const it of candidates) {
			if (excess <= 0) break;
			it.phase = 'leaving';
			it.removeAt = Math.min(it.removeAt, now + LEAVE_MS);
			excess--;
		}
	}
}

function sanitize(t: StackTiming): StackTiming {
	const finite = (n: number, fallback: number) => (Number.isFinite(n) ? n : fallback);
	return {
		maxStack: Math.max(1, Math.round(finite(t.maxStack, 4))),
		lingerMs: Math.max(0, finite(t.lingerMs, 2200)),
		fadeMs: Math.max(0, finite(t.fadeMs, 450)),
	};
}
