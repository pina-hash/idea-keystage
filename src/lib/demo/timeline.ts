/**
 * A small builder for demo scripts: it emits the same protocol messages the
 * app would (callouts, key, button, wheel and move messages) on a timeline,
 * so the demo exercises the real renderer path. Callout ids count up from 1.
 */
import type { Callout, CalloutKind, Chip, ServerMsg } from '../protocol';
import type { DemoStep } from '../renderer/transport';
import { DEMO_MODIFIERS, MODIFIER_ORDER, physOf } from './scancodes';

export interface CaptionLike {
	caption: string;
	context: string | null;
}

export interface CalloutOptions {
	caption?: CaptionLike | null;
	profile?: string | null;
}

/** Display label for a canonical key name (US words, as keystage-core uses). */
export function labelOf(key: string): string {
	switch (key) {
		case 'PageUp':
			return 'Page Up';
		case 'PageDown':
			return 'Page Down';
		case 'CapsLock':
			return 'Caps Lock';
		default:
			return key;
	}
}

export function chipOf(key: string): Chip {
	const mod = MODIFIER_ORDER.includes(key);
	return { key, label: labelOf(key), kind: mod ? 'modifier' : 'key' };
}

/** Orders modifiers first (Ctrl, Shift, Alt, AltGr, Win) then the key. */
export function orderCombo(keys: readonly string[]): string[] {
	const mods = MODIFIER_ORDER.filter((m) => keys.includes(m));
	const rest = keys.filter((k) => !MODIFIER_ORDER.includes(k));
	return [...mods, ...rest];
}

export interface TimelineOptions {
	/** Emit pointer moves (the cursor and all routes draw them). */
	pointer?: boolean;
	/** Interval between emitted moves while gliding. */
	moveMs?: number;
}

export class Timeline {
	private steps: DemoStep[] = [];
	private nextId = 1;
	private readonly pointer: boolean;
	private readonly moveMs: number;
	/** Current time, ms from the start of the script. */
	t = 0;
	/** Current pointer position (normalized). */
	x = 0.5;
	y = 0.5;

	constructor(opts: TimelineOptions = {}) {
		this.pointer = opts.pointer ?? false;
		this.moveMs = Math.max(16, opts.moveMs ?? 40);
	}

	get length(): number {
		return this.steps.length;
	}

	wait(ms: number): this {
		this.t += Math.max(0, ms);
		return this;
	}

	emit(msg: ServerMsg, at = this.t): this {
		this.steps.push({ at: Math.max(0, Math.round(at)), msg });
		return this;
	}

	/** Sorted copy of the steps (stable for equal times). */
	build(): DemoStep[] {
		return this.steps
			.map((s, i) => ({ s, i }))
			.sort((a, b) => a.s.at - b.s.at || a.i - b.i)
			.map((x) => x.s);
	}

	newCallout(
		kind: CalloutKind,
		steps: Chip[][],
		opts: CalloutOptions & { text?: string | null; count?: number } = {}
	): Callout {
		const id = this.nextId++;
		const t = Math.round(this.t);
		return {
			id,
			kind,
			steps,
			text: opts.text ?? null,
			count: opts.count ?? 1,
			caption: opts.caption?.caption ?? null,
			context: opts.caption?.context ?? null,
			profile: opts.profile ?? null,
			t_ms: t,
			updated_ms: t,
		};
	}

	callout(c: Callout): this {
		return this.emit({ t: 'callout', callout: { ...c, steps: c.steps.map((s) => [...s]) } });
	}

	/** Re-sends a callout with changes (same id), stamped now. */
	update(c: Callout, changes: Partial<Callout>): Callout {
		const next: Callout = { ...c, ...changes, updated_ms: Math.round(this.t) };
		this.callout(next);
		return next;
	}

	key(key: string, down: boolean): this {
		const phys = physOf(key);
		if (!phys) return this;
		return this.emit({
			t: 'key',
			phys,
			key,
			down,
			modifier: DEMO_MODIFIERS[key] ?? null,
			t_ms: Math.round(this.t),
		});
	}

	/**
	 * Presses a combo the way a person does: modifiers down, the key down
	 * (the callout appears), the key up, modifiers up. Returns the callout.
	 */
	combo(keys: readonly string[], opts: CalloutOptions = {}, holdMs = 140): Callout {
		const ordered = orderCombo(keys);
		const mods = ordered.filter((k) => MODIFIER_ORDER.includes(k));
		const main = ordered.filter((k) => !MODIFIER_ORDER.includes(k));
		for (const m of mods) this.key(m, true).wait(40);
		for (const k of main) this.key(k, true);
		const c = this.newCallout(mods.length > 0 ? 'combo' : 'key', [ordered.map(chipOf)], opts);
		this.callout(c);
		this.wait(holdMs);
		for (const k of main) this.key(k, false);
		this.wait(30);
		for (const m of [...mods].reverse()) this.key(m, false);
		return c;
	}

	/** Presses the same combo again: the callout's count goes up. */
	repeat(c: Callout, keys: readonly string[], holdMs = 120): Callout {
		const ordered = orderCombo(keys);
		const mods = ordered.filter((k) => MODIFIER_ORDER.includes(k));
		const main = ordered.filter((k) => !MODIFIER_ORDER.includes(k));
		for (const m of mods) this.key(m, true).wait(30);
		for (const k of main) this.key(k, true);
		const next = this.update(c, { count: c.count + 1 });
		this.wait(holdMs);
		for (const k of main) this.key(k, false);
		this.wait(30);
		for (const m of [...mods].reverse()) this.key(m, false);
		return next;
	}

	button(
		button: 'left' | 'right' | 'middle' | 'x1' | 'x2',
		down: boolean,
		x: number,
		y: number
	): this {
		return this.emit({ t: 'button', button, down, x, y, t_ms: Math.round(this.t) });
	}

	wheel(dy: number, x: number, y: number, dx = 0): this {
		return this.emit({ t: 'wheel', dx, dy, x, y, t_ms: Math.round(this.t) });
	}

	move(x: number, y: number): this {
		this.x = x;
		this.y = y;
		if (!this.pointer) return this;
		return this.emit({ t: 'move', x: round4(x), y: round4(y), t_ms: Math.round(this.t) });
	}

	/**
	 * Moves the pointer to (x, y) over `ms` along an eased path with a slight
	 * arc, emitting moves every `moveMs` when pointer moves are on. Time
	 * advances by `ms` either way.
	 */
	glide(x: number, y: number, ms: number, arc = 0.04): this {
		const x0 = this.x;
		const y0 = this.y;
		const n = Math.max(1, Math.round(ms / this.moveMs));
		const start = this.t;
		for (let i = 1; i <= n; i++) {
			const k = i / n;
			const e = k < 0.5 ? 2 * k * k : 1 - Math.pow(-2 * k + 2, 2) / 2;
			const bulge = Math.sin(Math.PI * k) * arc;
			this.t = start + (ms * i) / n;
			this.move(x0 + (x - x0) * e + bulge * (y - y0), y0 + (y - y0) * e - bulge * (x - x0));
		}
		this.t = start + ms;
		this.x = x;
		this.y = y;
		return this;
	}
}

function round4(n: number): number {
	return Math.round(n * 10000) / 10000;
}
