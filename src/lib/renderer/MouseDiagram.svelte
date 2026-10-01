<!--
	The mouse diagram: a top view of a mouse in SVG with left, right and
	middle buttons, a wheel with ridges, and two thumb buttons (X1 back,
	lower; X2 forward, upper). A held part sinks (it shrinks into a dark well)
	and fills with the theme's hit color, so shape and color both say it is
	pressed; after the release the color fades. A wheel notch lights the
	wheel, scrolls its ridges and sends two chevrons the way it turned.

	Colors: --ks-mouse-body, --ks-mouse-line, --ks-mouse-hit, --ks-tab,
	--ks-plate-glow and --ks-plate-shade from the theme on .ks-stage.
	Geometry and timing: src/lib/renderer/keyboard/mouse-diagram.ts.
-->
<script module lang="ts">
	let instances = 0;
</script>

<script lang="ts">
	import type { MouseButton } from '../protocol';
	import type { WheelDirection } from './logic/inputs';
	import type { MouseDiagramProps } from './visual-props';
	import { MOUSE_DIAGRAM } from './metrics';
	import {
		BODY,
		BUTTON_FADE_MS,
		LEFT_AREA,
		PRESS,
		RIDGE_STEP,
		RIGHT_AREA,
		SPLIT_Y,
		TAB,
		VIEW_H,
		VIEW_W,
		WHEEL,
		WHEEL_SHOW_MS,
		WHEEL_WELL,
		X1_BOX,
		X2_BOX,
		boxPath,
		chevrons,
		ridgeSteps,
		ridgesPath,
		shouldAnimateWheel,
	} from './keyboard/mouse-diagram';
	import { lit } from './keyboard/lit';

	interface Props extends MouseDiagramProps {
		/** How long a released button takes to fade (the keyboard passes its decay). */
		fadeMs?: number;
	}

	let { buttons, wheel, size, fadeMs = BUTTON_FADE_MS }: Props = $props();

	instances += 1;
	const uid = `ksm${instances}`;

	const height = $derived(Number.isFinite(size) && size > 0 ? size : MOUSE_DIAGRAM.heightPx);
	const width = $derived(height * MOUSE_DIAGRAM.aspect);
	const fade = $derived(Number.isFinite(fadeMs) && fadeMs >= 0 ? fadeMs : BUTTON_FADE_MS);

	const ridges = ridgesPath();
	const wheelPath = boxPath(WHEEL);
	const wellPath = boxPath(WHEEL_WELL);
	const x1Path = boxPath(X1_BOX);
	const x2Path = boxPath(X2_BOX);
	const seam = `M33 3V${WHEEL_WELL.y}M33 ${WHEEL_WELL.y + WHEEL_WELL.h}V${SPLIT_Y}M5 ${SPLIT_Y}H61`;

	/* The wheel: each new pulse lights it, scrolls the ridges and runs the chevrons. */
	let lastSeq = 0;
	let wheelDir = $state<WheelDirection>('up');
	let wheelSeq = $state(0);
	let wheelLight = $state<{ down: boolean; pressedAt: number; releasedAt: number } | null>(null);
	let wheelSteps = $state(1);
	const chev = $derived(chevrons(wheelDir));

	$effect(() => {
		const p = wheel;
		if (!p) return;
		const now = performance.now();
		if (!shouldAnimateWheel(p, lastSeq, now)) {
			if (p.seq > lastSeq) lastSeq = p.seq;
			return;
		}
		lastSeq = p.seq;
		wheelDir = p.direction;
		wheelSteps = ridgeSteps(p);
		wheelLight = { down: false, pressedAt: p.at, releasedAt: p.at };
		wheelSeq = p.seq;
	});

	interface PulseParams {
		seq: number;
		dx: number;
		dy: number;
		delay: number;
	}

	/** Runs the chevron animation once per new pulse. */
	function chevronRun(node: SVGGElement, params: PulseParams) {
		let seq = 0;
		let anim: Animation | null = null;
		function run(p: PulseParams) {
			if (p.seq === seq || p.seq === 0) return;
			seq = p.seq;
			anim?.cancel();
			if (typeof node.animate !== 'function') return;
			// With reduced motion the chevrons only blink in place.
			const still =
				typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
			const dx = still ? 0 : p.dx;
			const dy = still ? 0 : p.dy;
			anim = node.animate(
				[
					{ opacity: 0, transform: 'translate(0px, 0px)' },
					{ opacity: 1, offset: 0.22 },
					{ opacity: 0, transform: `translate(${dx}px, ${dy}px)` },
				],
				{ duration: WHEEL_SHOW_MS, delay: p.delay, easing: 'cubic-bezier(0.2, 0.8, 0.2, 1)' }
			);
		}
		run(params);
		return {
			update: run,
			destroy() {
				anim?.cancel();
			},
		};
	}

	/** Scrolls the ridges one step per notch. */
	function ridgeRun(
		node: SVGPathElement,
		params: { seq: number; dir: WheelDirection; steps: number }
	) {
		let seq = 0;
		let anim: Animation | null = null;
		function run(p: { seq: number; dir: WheelDirection; steps: number }) {
			if (p.seq === seq || p.seq === 0) return;
			seq = p.seq;
			anim?.cancel();
			if (typeof node.animate !== 'function' || (p.dir !== 'up' && p.dir !== 'down')) return;
			if (
				typeof matchMedia === 'function' &&
				matchMedia('(prefers-reduced-motion: reduce)').matches
			) {
				return;
			}
			const dy = p.dir === 'up' ? -RIDGE_STEP : RIDGE_STEP;
			anim = node.animate(
				[{ transform: 'translate(0px, 0px)' }, { transform: `translate(0px, ${dy}px)` }],
				{ duration: 120, iterations: p.steps, easing: 'ease-out' }
			);
		}
		run(params);
		return {
			update: run,
			destroy() {
				anim?.cancel();
			},
		};
	}

	function lightOf(b: MouseButton) {
		const s = buttons[b];
		if (!s || (!s.down && s.releasedAt === null)) return null;
		return { down: s.down, pressedAt: s.pressedAt ?? 0, releasedAt: s.releasedAt };
	}

	const parts = $derived({
		left: lightOf('left'),
		right: lightOf('right'),
		middle: lightOf('middle'),
		x1: lightOf('x1'),
		x2: lightOf('x2'),
	});

	function origin(b: MouseButton): string {
		const p = PRESS[b];
		return `transform-origin: ${p.cx}px ${p.cy}px; --kms-press: ${p.scale}`;
	}
</script>

<svg
	class="kms"
	viewBox="0 0 {VIEW_W} {VIEW_H}"
	{width}
	{height}
	style="--kms-h: {height}px"
	aria-hidden="true"
	focusable="false"
>
	<defs>
		<clipPath id="{uid}-body"><path d={BODY} /></clipPath>
		<clipPath id="{uid}-wheel"><path d={wheelPath} /></clipPath>
		<linearGradient id="{uid}-shade" x1="0" y1="0" x2="0" y2="1">
			<stop offset="0" stop-color="#ffffff" stop-opacity="0.16" />
			<stop offset="0.42" stop-color="#ffffff" stop-opacity="0" />
			<stop offset="0.7" stop-color="#000000" stop-opacity="0" />
			<stop offset="1" stop-color="#000000" stop-opacity="0.22" />
		</linearGradient>
	</defs>

	<path class="kms-body" d={BODY} />

	<g clip-path="url(#{uid}-body)">
		{#each [{ id: 'left', box: LEFT_AREA }, { id: 'right', box: RIGHT_AREA }] as main (main.id)}
			{@const b = main.id as MouseButton}
			{@const light = parts[b as 'left' | 'right']}
			<g class="kms-part" class:is-down={light?.down}>
				<rect
					class="kms-well"
					x={main.box.x}
					y={main.box.y}
					width={main.box.w}
					height={main.box.h}
				/>
				<g class="kms-face" style={origin(b)}>
					<g clip-path="url(#{uid}-body)">
						<rect
							class="kms-fill"
							x={main.box.x}
							y={main.box.y}
							width={main.box.w}
							height={main.box.h}
						/>
						{#if light}
							<rect
								class="kms-hit"
								x={main.box.x}
								y={main.box.y}
								width={main.box.w}
								height={main.box.h}
								use:lit={{ light, decayMs: fade }}
							/>
						{/if}
					</g>
				</g>
			</g>
		{/each}
		<path class="kms-sheen" d={BODY} fill="url(#{uid}-shade)" />
		<path class="kms-seam" d={seam} />
	</g>

	<path class="kms-outline" d={BODY} />

	<!-- Thumb buttons on the left side, over the body's edge. -->
	{#each [{ id: 'x2', d: x2Path }, { id: 'x1', d: x1Path }] as side (side.id)}
		{@const b = side.id as MouseButton}
		{@const light = parts[b as 'x1' | 'x2']}
		<g class="kms-part" class:is-down={light?.down}>
			<path class="kms-well" d={side.d} />
			<g class="kms-face" style={origin(b)}>
				<path class="kms-fill" d={side.d} />
				{#if light}
					<path class="kms-hit" d={side.d} use:lit={{ light, decayMs: fade }} />
				{/if}
				<path class="kms-line" d={side.d} />
			</g>
		</g>
	{/each}

	<!-- The wheel in its well. -->
	<path class="kms-wellbox" d={wellPath} />
	<g class="kms-part" class:is-down={parts.middle?.down}>
		<g class="kms-face" style={origin('middle')}>
			<path class="kms-fill" d={wheelPath} />
			{#if wheelLight}
				<path
					class="kms-hit"
					d={wheelPath}
					use:lit={{ light: wheelLight, decayMs: WHEEL_SHOW_MS }}
				/>
			{/if}
			{#if parts.middle}
				<path class="kms-hit" d={wheelPath} use:lit={{ light: parts.middle, decayMs: fade }} />
			{/if}
			<g clip-path="url(#{uid}-wheel)">
				<path
					class="kms-ridges"
					d={ridges}
					use:ridgeRun={{ seq: wheelSeq, dir: wheelDir, steps: wheelSteps }}
				/>
			</g>
			<path class="kms-line" d={wheelPath} />
		</g>
	</g>

	{#each chev.paths as d, i (i)}
		<g class="kms-chev" use:chevronRun={{ seq: wheelSeq, dx: chev.dx, dy: chev.dy, delay: i * 70 }}>
			<path class="kms-chev-rim" {d} />
			<path class="kms-chev-line" {d} />
		</g>
	{/each}

	<path class="kms-tab" d={TAB} />
</svg>

<style>
	.kms {
		display: block;
		overflow: visible;
		filter: drop-shadow(0 0 calc(var(--kms-h) * 0.05) var(--ks-plate-glow))
			drop-shadow(0 calc(var(--kms-h) * 0.02) calc(var(--kms-h) * 0.03) var(--ks-plate-shade));
	}
	.kms-body {
		fill: var(--ks-mouse-body);
	}
	.kms-outline {
		fill: none;
		stroke: var(--ks-mouse-line);
		stroke-width: 1.5;
		stroke-linejoin: round;
	}
	.kms-sheen {
		pointer-events: none;
	}
	.kms-seam {
		fill: none;
		stroke: var(--ks-mouse-line);
		stroke-width: 1.1;
		stroke-linecap: round;
	}
	/* A held part shrinks into a dark well. */
	.kms-well {
		fill: rgba(0, 0, 0, 0.42);
		opacity: 0;
	}
	.kms-part.is-down .kms-well {
		opacity: 1;
	}
	.kms-wellbox {
		fill: rgba(0, 0, 0, 0.4);
		stroke: var(--ks-mouse-line);
		stroke-width: 0.9;
	}
	.kms-face {
		transform: scale(1);
		transition: transform 140ms cubic-bezier(0.2, 0.8, 0.2, 1);
	}
	.kms-part.is-down .kms-face {
		transform: scale(var(--kms-press));
		transition-duration: 60ms;
	}
	.kms-fill {
		fill: var(--ks-mouse-body);
	}
	.kms-hit {
		fill: var(--ks-mouse-hit);
		opacity: 0;
	}
	.kms-line {
		fill: none;
		stroke: var(--ks-mouse-line);
		stroke-width: 1.1;
	}
	.kms-part.is-down .kms-line {
		stroke-width: 1.8;
	}
	.kms-ridges {
		fill: none;
		stroke: var(--ks-mouse-line);
		stroke-width: 0.8;
		stroke-linecap: round;
		opacity: 0.6;
	}
	.kms-chev {
		opacity: 0;
	}
	.kms-chev-rim {
		fill: none;
		stroke: rgba(0, 0, 0, 0.45);
		stroke-width: 3.2;
		stroke-linecap: round;
		stroke-linejoin: round;
	}
	.kms-chev-line {
		fill: none;
		stroke: var(--ks-mouse-hit);
		stroke-width: 1.8;
		stroke-linecap: round;
		stroke-linejoin: round;
	}
	@media (prefers-reduced-motion: reduce) {
		.kms-face,
		.kms-part.is-down .kms-face {
			transition: none;
		}
	}
	.kms-tab {
		fill: var(--ks-tab);
		opacity: 0.9;
	}
</style>
