import { describe, expect, it } from 'vitest';
import type { KeyMsg } from '../../protocol';
import {
	RECENT_LIMIT,
	RECENT_MS,
	applyButton,
	applyKey,
	applyMove,
	applyWheel,
	emptyMouse,
	keyIntensity,
	nextKeyDeadline,
	nextMouseDeadline,
	pruneKeys,
	pruneMouse,
	releaseAllButtons,
	releaseAllKeys,
	wheelDirection,
	type KeyStates,
} from './inputs';

const key = (
	phys: string,
	k: string,
	down: boolean,
	modifier: KeyMsg['modifier'] = null
): KeyMsg => ({
	t: 'key',
	phys,
	key: k,
	down,
	modifier,
	t_ms: 0,
});

describe('key states', () => {
	it('tracks down, then released_at for decay', () => {
		let s: KeyStates = {};
		s = applyKey(s, key('sc:1D', 'Ctrl', true, 'ctrl'), 100);
		expect(s['sc:1D']).toMatchObject({
			down: true,
			pressedAt: 100,
			releasedAt: null,
			modifier: 'ctrl',
		});
		s = applyKey(s, key('sc:1D', 'Ctrl', false, 'ctrl'), 250);
		expect(s['sc:1D']).toMatchObject({ down: false, pressedAt: 100, releasedAt: 250 });
	});

	it('treats a repeat key-down as no change', () => {
		const s1 = applyKey({}, key('sc:2C', 'Z', true), 0);
		expect(applyKey(s1, key('sc:2C', 'Z', true), 30)).toBe(s1);
	});

	it('lights a key-up with no key-down briefly (Print Screen)', () => {
		const s = applyKey({}, key('sc:E037', 'PrintScreen', false), 500);
		expect(s['sc:E037']).toMatchObject({ down: false, pressedAt: 500, releasedAt: 500 });
	});

	it('decays: intensity falls from 1 to 0 over decay_ms, then the key is pruned', () => {
		let s = applyKey({}, key('sc:1E', 'A', true), 0);
		expect(keyIntensity(s['sc:1E'], 50, 400)).toBe(1);
		s = applyKey(s, key('sc:1E', 'A', false), 100);
		expect(keyIntensity(s['sc:1E'], 300, 400)).toBeCloseTo(0.5);
		expect(keyIntensity(s['sc:1E'], 500, 400)).toBe(0);
		expect(nextKeyDeadline(s, 400)).toBe(500);
		expect(pruneKeys(s, 499, 400)).toBe(s);
		expect(pruneKeys(s, 500, 400)).toEqual({});
		expect(keyIntensity(undefined, 0, 400)).toBe(0);
		expect(keyIntensity(s['sc:1E'], 100, 0)).toBe(0);
	});

	it('release_all lets every held key decay from now', () => {
		let s = applyKey({}, key('sc:1D', 'Ctrl', true, 'ctrl'), 0);
		s = applyKey(s, key('sc:1F', 'S', true), 10);
		const r = releaseAllKeys(s, 90);
		expect(Object.values(r).every((k) => !k.down && k.releasedAt === 90)).toBe(true);
		expect(releaseAllKeys(r, 100)).toBe(r);
	});
});

describe('mouse state', () => {
	it('tracks buttons, records presses for ripples and moves the pointer', () => {
		let m = emptyMouse();
		expect(m.pointer.known).toBe(false);
		m = applyButton(m, { t: 'button', button: 'middle', down: true, x: 0.4, y: 0.6, t_ms: 0 }, 10);
		expect(m.buttons.middle).toEqual({ down: true, pressedAt: 10, releasedAt: null });
		expect(m.pointer).toMatchObject({ x: 0.4, y: 0.6, known: true });
		expect(m.clicks).toHaveLength(1);
		const seq = m.clicks[0]?.seq ?? 0;
		m = applyButton(m, { t: 'button', button: 'middle', down: false, x: 0.5, y: 0.6, t_ms: 0 }, 80);
		expect(m.buttons.middle).toEqual({ down: false, pressedAt: 10, releasedAt: 80 });
		expect(m.clicks).toHaveLength(1);
		m = applyButton(m, { t: 'button', button: 'left', down: true, x: 0.5, y: 0.6, t_ms: 0 }, 90);
		expect(m.clicks[1]?.seq).toBeGreaterThan(seq);
	});

	it('moves only when the position changes', () => {
		const m = applyMove(emptyMouse(), { t: 'move', x: 0.1, y: 0.2, t_ms: 0 }, 5);
		expect(applyMove(m, { t: 'move', x: 0.1, y: 0.2, t_ms: 0 }, 6)).toBe(m);
		expect(applyMove(m, { t: 'move', x: 0.2, y: 0.2, t_ms: 0 }, 7).pointer.x).toBe(0.2);
	});

	it('reads wheel direction from WHEEL_DELTA units (+120 is up or right)', () => {
		expect(wheelDirection(0, 120)).toBe('up');
		expect(wheelDirection(0, -120)).toBe('down');
		expect(wheelDirection(120, 0)).toBe('right');
		expect(wheelDirection(-240, 30)).toBe('left');
		expect(wheelDirection(0, 0)).toBeNull();
		const m = applyWheel(emptyMouse(), { t: 'wheel', dx: 0, dy: -240, x: 0.5, y: 0.5, t_ms: 0 }, 1);
		expect(m.wheel).toMatchObject({ direction: 'down', notches: 2 });
		const still = emptyMouse();
		expect(applyWheel(still, { t: 'wheel', dx: 0, dy: 0, x: 0, y: 0, t_ms: 0 }, 1)).toBe(still);
	});

	it('keeps recent pulses bounded in count and age', () => {
		let m = emptyMouse();
		for (let i = 0; i < RECENT_LIMIT + 10; i++) {
			m = applyWheel(m, { t: 'wheel', dx: 0, dy: 120, x: 0.5, y: 0.5, t_ms: 0 }, i);
		}
		expect(m.wheels).toHaveLength(RECENT_LIMIT);
		expect(nextMouseDeadline(m)).toBe(10 + RECENT_MS);
		const later = pruneMouse(m, 100 + RECENT_MS);
		expect(later.wheels).toHaveLength(0);
		expect(pruneMouse(later, 0)).toBe(later);
	});

	it('release_all lets every held button go', () => {
		let m = applyButton(
			emptyMouse(),
			{ t: 'button', button: 'right', down: true, x: 0, y: 0, t_ms: 0 },
			0
		);
		m = releaseAllButtons(m, 50);
		expect(m.buttons.right).toEqual({ down: false, pressedAt: 0, releasedAt: 50 });
		expect(releaseAllButtons(m, 60)).toBe(m);
	});
});
