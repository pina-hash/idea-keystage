<!--
	The keyboard visual: the chosen layout drawn in SVG on a raised plate,
	lighting the PHYSICAL keys the key messages name. A held key sinks into
	its socket: a thick lit outline, a dark gap, then a smaller face in the
	press color, so its shape changes as well as its color. A modifier lights
	in its own hue (Ctrl, Shift, Alt and Win differ) and always carries a
	small corner marker. After the release the light fades over the decay time instead of
	snapping off. Legends follow the active Windows layout when a legends
	message arrived, US legends otherwise. The mouse diagram can sit beside it.

	The drawing fits KEYBOARD_FOOTPRINTS[layout] (+ KEYBOARD_MOUSE_W with the
	mouse) times unitPx (metrics.ts), the box obsSourceSize is computed from.
	Colors are the theme's --ks-* tokens on the enclosing .ks-stage.
-->
<script module lang="ts">
	let instances = 0;
</script>

<script lang="ts">
	import { onMount } from 'svelte';
	import type { Modifier } from '../../protocol';
	import { MODIFIERS } from '../../protocol';
	import type { KeyboardVisualProps } from '../visual-props';
	import { KEYBOARD_MOUSE_W, MOUSE_DIAGRAM } from '../metrics';
	import { GLYPHS } from '../glyphs';
	import MouseDiagram from '../MouseDiagram.svelte';
	import { getLayout, usLegendOf } from './layouts';
	import { drawKeyboard, legendDraw, subLegendDraw, type KeyDraw } from './draw';
	import {
		MODIFIER_TOKENS,
		indexLayout,
		inkOn,
		legendFor,
		lightsFor,
		normalizePhys,
		subLegendFor,
		type KeyLight,
	} from './model';
	import { lit } from './lit';

	interface Props extends KeyboardVisualProps {
		/**
		 * ISO geometry (Enter over two rows, the 102nd key). 'auto' switches to
		 * it the first time the 102nd key is pressed, since only an ISO
		 * keyboard has that key.
		 */
		iso?: boolean | 'auto';
	}

	let {
		layout,
		keys,
		legends,
		size,
		unitPx,
		decayMs,
		showMouse,
		buttons,
		wheel,
		iso = 'auto',
	}: Props = $props();

	instances += 1;
	const uid = `kbv${instances}`;

	/* Learned from the key stream: an ISO keyboard, a layout with AltGr. */
	let isoSeen = $state(false);
	let altGrSeen = $state(false);
	$effect(() => {
		if (isoSeen && altGrSeen) return;
		for (const s of Object.values(keys)) {
			if (!isoSeen && normalizePhys(s.phys) === 'sc:56') isoSeen = true;
			if (!altGrSeen && s.key === 'AltGr') altGrSeen = true;
		}
	});

	/* Theme values the drawing needs as numbers or for contrast. */
	let edgeW = $state(1);
	let inks = $state<Partial<Record<Modifier, string>>>({});
	let root = $state<HTMLDivElement | null>(null);

	function readTheme(el: Element): void {
		const cs = getComputedStyle(el);
		const e = parseFloat(cs.getPropertyValue('--ks-edge-w'));
		edgeW = Number.isFinite(e) && e > 0 ? e : 1;
		const next: Partial<Record<Modifier, string>> = {};
		for (const m of MODIFIERS) {
			const ink = inkOn(cs.getPropertyValue(MODIFIER_TOKENS[m]));
			if (ink) next[m] = ink;
		}
		inks = next;
	}

	onMount(() => {
		const el = root;
		if (!el) return;
		readTheme(el);
		const stage = el.closest('.ks-stage');
		if (!stage || typeof MutationObserver === 'undefined') return;
		const mo = new MutationObserver(() => readTheme(el));
		mo.observe(stage, { attributes: true, attributeFilter: ['style', 'data-theme', 'class'] });
		return () => mo.disconnect();
	});

	const unit = $derived(Number.isFinite(unitPx) && unitPx > 0 ? unitPx : 38);
	const useIso = $derived(iso === 'auto' ? isoSeen : iso);
	const def = $derived(getLayout(layout, useIso));
	const index = $derived(indexLayout(def));
	const drawing = $derived(drawKeyboard(def, unit, edgeW));
	const lights = $derived(lightsFor(index, keys, legends));

	const mouseColW = $derived(showMouse ? KEYBOARD_MOUSE_W * unit : 0);
	const boxW = $derived(2 * Math.ceil((drawing.width + mouseColW) / 2));
	const boxH = $derived(2 * Math.ceil(drawing.height / 2));
	/** The mouse fills its column with a margin, at the diagram's proportions. */
	const mouseH = $derived(
		Math.min(drawing.height - 1.6 * unit, ((KEYBOARD_MOUSE_W - 0.9) * unit) / MOUSE_DIAGRAM.aspect)
	);
	const mouseLeft = $derived(drawing.width + (mouseColW - mouseH * MOUSE_DIAGRAM.aspect) / 2);
	const mouseTop = $derived((drawing.height - mouseH) / 2);

	function legendOf(k: KeyDraw) {
		if (k.def.mod === 'alt' && k.def.id === 'sc:E038' && altGrSeen) {
			return legendDraw(k, { text: 'AltGr', kind: 'word' }, unit);
		}
		return legendDraw(k, legendFor(k.def, legends), unit);
	}

	function subOf(k: KeyDraw) {
		const text = subLegendFor(k.def, legends, usLegendOf);
		return text ? subLegendDraw(k, text, unit) : null;
	}

	function hueOf(light: KeyLight | undefined, k: KeyDraw): Modifier | null {
		return light?.modifier ?? k.def.mod;
	}

	function litFill(m: Modifier | null): string {
		return m ? `var(${MODIFIER_TOKENS[m]})` : `url(#${uid}-press)`;
	}

	function litInk(m: Modifier | null): string {
		if (!m) return 'var(--ks-press-ink)';
		return inks[m] ?? 'var(--ks-press-ink)';
	}

	function haloColor(m: Modifier | null): string {
		return m ? `var(${MODIFIER_TOKENS[m]})` : 'var(--ks-press-glow)';
	}
</script>

<div
	class="kbv"
	bind:this={root}
	data-layout={layout}
	data-size={size}
	data-iso={useIso ? 'true' : 'false'}
	style="width: {boxW}px; height: {boxH}px; --kbv-u: {unit}px"
	aria-hidden="true"
>
	<svg
		class="kbv-plate"
		width={drawing.width}
		height={drawing.height}
		viewBox="0 0 {drawing.width} {drawing.height}"
		focusable="false"
	>
		<defs>
			<linearGradient id="{uid}-plate" x1="0" y1="0" x2="0" y2="1">
				<stop offset="0" style="stop-color: var(--ks-plate-top)" />
				<stop offset="1" style="stop-color: var(--ks-plate-bot)" />
			</linearGradient>
			<linearGradient id="{uid}-plate-edge" x1="0" y1="0" x2="0" y2="1">
				<stop offset="0" style="stop-color: var(--ks-plate-edge-top)" />
				<stop offset="0.5" style="stop-color: var(--ks-plate-edge)" />
				<stop offset="1" style="stop-color: var(--ks-plate-edge)" />
			</linearGradient>
			<linearGradient id="{uid}-plate-hi" x1="0" y1="0" x2="0" y2="1">
				<stop offset="0" style="stop-color: var(--ks-plate-hi)" />
				<stop offset="0.08" style="stop-color: var(--ks-plate-hi); stop-opacity: 0" />
			</linearGradient>
		</defs>
		<path class="kbv-housing" d={drawing.housing} fill="url(#{uid}-plate)" />
		<path class="kbv-housing-hi" d={drawing.housingHi} stroke="url(#{uid}-plate-hi)" />
		<path
			class="kbv-housing-edge"
			d={drawing.housing}
			stroke="url(#{uid}-plate-edge)"
			style="stroke-width: var(--ks-edge-w)"
		/>
		<path class="kbv-tab" d={drawing.tab} />
		<path class="kbv-dots" d={drawing.dots} />
	</svg>

	<svg
		class="kbv-keys"
		width={drawing.width}
		height={drawing.height}
		viewBox="0 0 {drawing.width} {drawing.height}"
		focusable="false"
	>
		<defs>
			<linearGradient id="{uid}-cap" x1="0" y1="0" x2="0" y2="1">
				<stop offset="0" style="stop-color: var(--ks-cap-top)" />
				<stop offset="1" style="stop-color: var(--ks-cap-bot)" />
			</linearGradient>
			<linearGradient id="{uid}-press" x1="0" y1="0" x2="0" y2="1">
				<stop offset="0" style="stop-color: var(--ks-press-top)" />
				<stop offset="1" style="stop-color: var(--ks-press-bot)" />
			</linearGradient>
			<linearGradient id="{uid}-sheen" x1="0" y1="0" x2="0" y2="1">
				<stop offset="0" stop-color="#ffffff" stop-opacity="0.3" />
				<stop offset="0.55" stop-color="#ffffff" stop-opacity="0" />
				<stop offset="1" stop-color="#000000" stop-opacity="0.12" />
			</linearGradient>
		</defs>

		{#each drawing.keys as k (k.def.id)}
			{@const light = lights.get(k.def.id)}
			{@const down = light?.down === true}
			{@const hue = hueOf(light, k)}
			{@const legend = legendOf(k)}
			{@const sub = subOf(k)}
			<g class="kbv-key" class:is-down={down} class:is-mod={k.def.mod !== null}>
				<path class="kbv-shadow" d={k.outer} />
				<path class="kbv-wall" d={k.outer} fill="url(#{uid}-cap)" />
				<path class="kbv-wall-shade" d={k.outer} />
				{#if down}
					<path class="kbv-socket" d={k.outer} />
				{:else}
					<path class="kbv-lip" d={k.lip} />
				{/if}
				<path class="kbv-face" d={down ? k.faceDown : k.face} fill="url(#{uid}-cap)" />
				{#if !down}
					<path class="kbv-hi" d={k.hi} />
				{/if}
				{#if k.def.mod}
					<path
						class="kbv-marker"
						d={k.marker}
						style="fill: var({MODIFIER_TOKENS[k.def.mod]})"
						transform={down ? `translate(${k.downDx} ${k.downDy})` : undefined}
					/>
				{/if}
				<g
					class="kbv-legend-pos"
					transform={down ? `translate(${k.downDx} ${k.downDy})` : undefined}
				>
					{#if legend}
						{#if legend.kind === 'glyph' && legend.glyph}
							<svg
								class="kbv-glyph"
								x={legend.x - legend.size / 2}
								y={legend.y - legend.size / 2}
								width={legend.size}
								height={legend.size}
								viewBox="0 0 24 24"
							>
								<path d={GLYPHS[legend.glyph].d} />
							</svg>
						{:else}
							<text
								class="kbv-legend"
								class:is-mono={legend.mono}
								class:is-fn={legend.kind === 'fn'}
								x={legend.x}
								y={legend.y}
								font-size={legend.size}>{legend.text}</text
							>
						{/if}
					{/if}
					{#if sub}
						<text class="kbv-legend kbv-sub" x={sub.x} y={sub.y} font-size={sub.size}
							>{sub.text}</text
						>
					{/if}
				</g>
				{#if light}
					<g class="kbv-lit" use:lit={{ light, decayMs }}>
						<path
							class="kbv-halo"
							d={k.outer}
							style="stroke: {haloColor(hue)}"
							stroke-width={k.haloWidth}
						/>
						<path
							class="kbv-lit-face"
							d={down ? k.faceDown : k.face}
							style="fill: {litFill(hue)}"
						/>
						{#if hue}
							<path class="kbv-lit-sheen" d={down ? k.faceDown : k.face} fill="url(#{uid}-sheen)" />
						{/if}
						<g transform={down ? `translate(${k.downDx} ${k.downDy})` : undefined}>
							{#if k.def.mod}
								<path class="kbv-lit-marker" d={k.marker} style="fill: {litInk(hue)}" />
							{/if}
							{#if legend}
								{#if legend.kind === 'glyph' && legend.glyph}
									<svg
										class="kbv-glyph"
										x={legend.x - legend.size / 2}
										y={legend.y - legend.size / 2}
										width={legend.size}
										height={legend.size}
										viewBox="0 0 24 24"
										style="stroke: {litInk(hue)}"
									>
										<path d={GLYPHS[legend.glyph].d} />
									</svg>
								{:else}
									<text
										class="kbv-legend"
										class:is-mono={legend.mono}
										class:is-fn={legend.kind === 'fn'}
										x={legend.x}
										y={legend.y}
										font-size={legend.size}
										style="fill: {litInk(hue)}">{legend.text}</text
									>
								{/if}
							{/if}
							{#if sub}
								<text
									class="kbv-legend kbv-sub"
									x={sub.x}
									y={sub.y}
									font-size={sub.size}
									style="fill: {litInk(hue)}">{sub.text}</text
								>
							{/if}
						</g>
					</g>
				{/if}
				<path class="kbv-edge" d={k.edge} style="stroke-width: var(--ks-edge-w)" />
				{#if light}
					<path
						class="kbv-ring"
						d={k.ring}
						stroke-width={k.ringWidth}
						use:lit={{ light, decayMs }}
					/>
				{/if}
			</g>
		{/each}
	</svg>

	{#if showMouse}
		<div
			class="kbv-mouse"
			style="left: {mouseLeft}px; top: {mouseTop}px; width: {mouseH *
				MOUSE_DIAGRAM.aspect}px; height: {mouseH}px"
		>
			<MouseDiagram {buttons} {wheel} size={mouseH} fadeMs={decayMs} />
		</div>
	{/if}
</div>

<style>
	.kbv {
		position: relative;
		display: block;
		pointer-events: none;
	}
	.kbv svg {
		overflow: visible;
	}
	.kbv-plate,
	.kbv-keys {
		position: absolute;
		left: 0;
		top: 0;
		display: block;
	}
	.kbv-plate {
		filter: drop-shadow(0 0 calc(var(--kbv-u) * 0.3) var(--ks-plate-glow))
			drop-shadow(0 calc(var(--kbv-u) * 0.06) calc(var(--kbv-u) * 0.1) var(--ks-plate-shade));
	}
	.kbv-housing-hi {
		fill: none;
		stroke-width: 2;
	}
	.kbv-housing-edge {
		fill: none;
	}
	.kbv-tab,
	.kbv-dots {
		fill: var(--ks-tab);
	}
	.kbv-dots {
		opacity: 0.8;
	}

	/* A raised key: the callout keycap in SVG. */
	.kbv-shadow {
		fill: var(--ks-cap-shade);
		transform: translateY(calc(var(--kbv-u) * 0.04));
	}
	.kbv-wall-shade {
		fill: var(--ks-cap-skirt);
	}
	.kbv-lip {
		fill: var(--ks-cap-hi);
	}
	/* A held key's socket: the plate shows between its outline and its face. */
	.kbv-socket {
		fill: var(--ks-plate-bot);
	}
	.kbv-hi {
		fill: none;
		stroke: var(--ks-cap-hi);
		stroke-width: 1;
	}
	.kbv-edge {
		fill: none;
		stroke: var(--ks-cap-edge);
	}
	.kbv-marker {
		opacity: 0.95;
	}

	.kbv-legend {
		fill: var(--ks-cap-ink);
		font-family: var(--ks-font-key);
		font-weight: 700;
		text-anchor: middle;
		dominant-baseline: central;
		letter-spacing: 0.01em;
	}
	.kbv-legend.is-fn {
		font-weight: 600;
	}
	.kbv-legend.is-mono {
		font-family: var(--ks-font-mono);
		font-weight: 400;
		letter-spacing: 0.06em;
		text-transform: uppercase;
	}
	.kbv-sub {
		text-anchor: end;
		font-weight: 600;
		opacity: 0.85;
	}
	.kbv-glyph {
		fill: none;
		stroke: var(--ks-cap-ink);
		stroke-width: 2.4;
		stroke-linecap: round;
		stroke-linejoin: round;
		overflow: visible;
	}

	/* Lit: the press color (or the modifier's hue), a halo and a thick outline. */
	.kbv-lit,
	.kbv-ring {
		opacity: 0;
	}
	.kbv-halo {
		fill: none;
		stroke-opacity: 0.5;
	}
	.kbv-ring {
		fill: none;
		stroke: var(--ks-press-edge);
	}

	.kbv-mouse {
		position: absolute;
	}
</style>
