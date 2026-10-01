import { describe, expect, it, vi } from 'vitest';
import { lit, type LitParams } from './lit';

interface FakeAnimation {
	keyframes: Keyframe[];
	options: KeyframeAnimationOptions;
	cancelled: boolean;
	cancel(): void;
	onfinish: (() => void) | null;
}

function fakeNode(withAnimate = true) {
	const animations: FakeAnimation[] = [];
	const node = {
		style: { opacity: '', transition: '' } as CSSStyleDeclaration,
		getBoundingClientRect: () => ({}),
		animate: withAnimate
			? (keyframes: Keyframe[], options: KeyframeAnimationOptions) => {
					const a: FakeAnimation = {
						keyframes,
						options,
						cancelled: false,
						onfinish: null,
						cancel() {
							this.cancelled = true;
						},
					};
					animations.push(a);
					return a as unknown as Animation;
				}
			: undefined,
	};
	return { node: node as unknown as Element & ElementCSSInlineStyle, animations };
}

describe('the lit action', () => {
	it('is fully lit while held and dark with no light', () => {
		const { node, animations } = fakeNode();
		const a = lit(node, {
			light: { down: true, pressedAt: 0, releasedAt: null },
			decayMs: 700,
			now: () => 10,
		});
		expect(node.style.opacity).toBe('1');
		a.update({ light: null, decayMs: 700, now: () => 20 });
		expect(node.style.opacity).toBe('0');
		expect(animations).toHaveLength(0);
	});

	it('fades on the compositor from the current level after a release', () => {
		const { node, animations } = fakeNode();
		const params: LitParams = {
			light: { down: false, pressedAt: 0, releasedAt: 1000 },
			decayMs: 700,
			now: () => 1175,
		};
		lit(node, params);
		expect(animations).toHaveLength(1);
		const anim = animations[0] as FakeAnimation;
		expect(anim.keyframes[0]?.opacity).toBeCloseTo(0.75, 9);
		expect(anim.keyframes[1]?.opacity).toBe(0);
		expect(anim.options.duration).toBe(525);
		expect(anim.options.easing).toBe('linear');
		// Underneath the animation the layer is already at its end state.
		expect(node.style.opacity).toBe('0');
	});

	it('does not restart for the same press and release', () => {
		const { node, animations } = fakeNode();
		const light = { down: false, pressedAt: 0, releasedAt: 1000 };
		const a = lit(node, { light, decayMs: 700, now: () => 1000 });
		a.update({ light: { ...light }, decayMs: 700, now: () => 1100 });
		expect(animations).toHaveLength(1);
		// A new press cancels the fade and lights fully.
		a.update({
			light: { down: true, pressedAt: 1200, releasedAt: null },
			decayMs: 700,
			now: () => 1200,
		});
		expect(animations[0]?.cancelled).toBe(true);
		expect(node.style.opacity).toBe('1');
		// Its release fades again.
		a.update({
			light: { down: false, pressedAt: 1200, releasedAt: 1300 },
			decayMs: 700,
			now: () => 1300,
		});
		expect(animations).toHaveLength(2);
		a.destroy();
		expect(animations[1]?.cancelled).toBe(true);
	});

	it('snaps off when the decay is already over or zero', () => {
		const { node, animations } = fakeNode();
		lit(node, {
			light: { down: false, pressedAt: 0, releasedAt: 10 },
			decayMs: 700,
			now: () => 900,
		});
		expect(node.style.opacity).toBe('0');
		const zero = fakeNode();
		lit(zero.node, {
			light: { down: false, pressedAt: 0, releasedAt: 10 },
			decayMs: 0,
			now: () => 10,
		});
		expect(zero.node.style.opacity).toBe('0');
		expect(animations).toHaveLength(0);
		expect(zero.animations).toHaveLength(0);
	});

	it('falls back to a CSS transition without the Web Animations API', () => {
		const { node } = fakeNode(false);
		lit(node, {
			light: { down: false, pressedAt: 0, releasedAt: 1000 },
			decayMs: 400,
			now: () => 1100,
		});
		expect(node.style.transition).toBe('opacity 300ms linear');
		expect(node.style.opacity).toBe('0');
	});

	it('uses performance.now when no clock is given', () => {
		const spy = vi.spyOn(performance, 'now').mockReturnValue(1000);
		const { node, animations } = fakeNode();
		lit(node, { light: { down: false, pressedAt: 0, releasedAt: 1000 }, decayMs: 500 });
		expect(animations[0]?.options.duration).toBe(500);
		spy.mockRestore();
	});
});
