/**
 * The `lit` action: drives the opacity of a key's (or a mouse button's) lit
 * layer from its held and released times. Fully lit while held; after the
 * release it fades linearly to nothing over the decay time, on the
 * compositor (Web Animations API), so a decaying key costs no script work
 * per frame and an idle keyboard runs no animation at all.
 *
 * Without Element.animate (not expected in Chromium 103 or later, kept for
 * safety) it falls back to a CSS opacity transition.
 */
import { decayPlan, decaySignature, type KeyLight } from './model';

export interface LitParams {
	light: Pick<KeyLight, 'down' | 'releasedAt' | 'pressedAt'> | null | undefined;
	decayMs: number;
	/** Clock of the release times (performance.now by default). */
	now?: () => number;
}

interface Animated {
	animate?: (keyframes: Keyframe[], options: KeyframeAnimationOptions) => Animation;
}

function defaultNow(): number {
	return typeof performance !== 'undefined' ? performance.now() : Date.now();
}

export function lit(node: Element & ElementCSSInlineStyle, params: LitParams) {
	let signature = '';
	let running: Animation | null = null;

	function stop(): void {
		if (running) {
			running.cancel();
			running = null;
		}
	}

	function apply(p: LitParams): void {
		const sig = decaySignature(p.light, p.decayMs);
		if (sig === signature) return;
		signature = sig;
		stop();
		const plan = decayPlan(p.light, (p.now ?? defaultNow)(), p.decayMs);
		const style = node.style;
		if (plan.kind !== 'fade') {
			style.transition = 'none';
			style.opacity = plan.kind === 'hold' ? '1' : '0';
			return;
		}
		const anim = (node as Animated).animate;
		if (typeof anim === 'function') {
			style.transition = 'none';
			style.opacity = '0';
			running = anim.call(node, [{ opacity: plan.from }, { opacity: 0 }], {
				duration: plan.duration,
				easing: 'linear',
				fill: 'none',
			});
			const mine = running;
			mine.onfinish = () => {
				if (running === mine) running = null;
			};
			return;
		}
		// Fallback: jump to the current level, then transition to 0.
		style.transition = 'none';
		style.opacity = String(plan.from);
		void (node as unknown as HTMLElement).getBoundingClientRect?.();
		style.transition = `opacity ${Math.round(plan.duration)}ms linear`;
		style.opacity = '0';
	}

	apply(params);
	return {
		update(p: LitParams) {
			apply(p);
		},
		destroy() {
			stop();
		},
	};
}
