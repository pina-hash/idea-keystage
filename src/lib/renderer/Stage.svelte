<!--
	The renderer's composition: every piece a route shows, placed at its
	anchor inside the stage, in the active theme. The overlay page mounts it
	full window; the settings preview mounts it inside a box (the stage fills
	its nearest positioned ancestor). The callouts move inward, clear of the
	keyboard and the mouse diagram, when those cover the row the stack grows
	from (layout.ts calloutsOffsetY).

	The theme's tokens and the size unit are set here, so the keyboard visual,
	the mouse diagram and the cursor effects read the same `--ks-*` values.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';
	import '../styles/fonts.css';
	import '../themes/index.css';
	import './callouts.css';
	import type { ClientHost } from '../protocol';
	import type { RendererStore } from './store.svelte';
	import { resolveTheme } from '../themes/themes';
	import { CALLOUT_UNIT_PX, KEYBOARD_UNIT_PX, MOUSE_DIAGRAM } from './metrics';
	import { FONT_STACKS, indicatorsFor, visiblePieces } from './pieces';
	import { anchorStyle, calloutsOffsetY, type Size } from './layout';
	import { keyboardDrawingSize, mouseDrawingSize } from './sizes';
	import Callouts from './Callouts.svelte';
	import KeyboardVisual from './keyboard/KeyboardVisual.svelte';
	import MouseDiagram from './MouseDiagram.svelte';
	import CursorFx from './CursorFx.svelte';

	interface Props {
		store: RendererStore;
		/** What this page is (Host A draws nothing while native_visible is false). */
		host: ClientHost;
		/** Multiplies config.scale; the settings preview shrinks the stage with it. */
		zoom?: number;
		/** Width over height of the monitor the cursor positions refer to. */
		aspect?: number;
		/** Drawn last, inside the stage (the overlay page's connection card). */
		children?: Snippet;
	}

	let { store, host, zoom = 1, aspect, children }: Props = $props();

	const config = $derived(store.config);
	const scale = $derived(Math.max(0.05, config.scale * (zoom > 0 ? zoom : 1)));
	const theme = $derived(resolveTheme(config));
	const pieces = $derived(visiblePieces(store.route, config, store.appState, host));
	const indicators = $derived(pieces.indicators ? indicatorsFor(config, store.appState) : []);
	const unit = $derived(CALLOUT_UNIT_PX[config.callouts.size] * scale);
	const kbUnit = $derived(KEYBOARD_UNIT_PX[config.keyboard.size] * scale);
	const stageStyle = $derived(
		[
			theme.style,
			`--ks-scale: ${scale}`,
			`--ks-u: ${unit}px`,
			`--ks-fade-ms: ${Math.round(config.callouts.fade_ms)}ms`,
			`--ks-font-key: ${FONT_STACKS[config.font]}`,
		]
			.filter((s) => s.length > 0)
			.join('; ')
	);

	let width = $state(1920);
	let height = $state(1080);
	const monitorAspect = $derived(aspect ?? (height > 0 ? width / height : 16 / 9));

	// The drawn size of the keyboard and the mouse diagram, measured; until the
	// first measurement, the box their drawing is specified to fit (metrics.ts).
	let kbWidth = $state(0);
	let kbHeight = $state(0);
	let mouseWidth = $state(0);
	let mouseHeight = $state(0);
	function measured(w: number, h: number, planned: Size): Size {
		return w > 0 && h > 0 ? { width: w, height: h } : planned;
	}
	// The callouts move clear of the other pieces (layout.ts calloutsOffsetY).
	const calloutsY = $derived(
		calloutsOffsetY(
			config,
			{
				keyboard: pieces.keyboard
					? measured(kbWidth, kbHeight, keyboardDrawingSize(config.keyboard, kbUnit))
					: null,
				mouse: pieces.mouse ? measured(mouseWidth, mouseHeight, mouseDrawingSize(scale)) : null,
			},
			{ width, height },
			scale
		)
	);
</script>

<div
	class="ks-stage"
	data-theme={theme.id}
	data-route={store.route}
	style={stageStyle}
	bind:clientWidth={width}
	bind:clientHeight={height}
>
	{#if pieces.cursor}
		<CursorFx cursor={config.cursor} mouse={store.mouse} aspect={monitorAspect} {scale} />
	{/if}
	{#if pieces.keyboard}
		<div
			class="ks-region is-keyboard"
			style={anchorStyle(
				config.keyboard.anchor,
				config.keyboard.offset_x,
				config.keyboard.offset_y,
				scale
			)}
			bind:clientWidth={kbWidth}
			bind:clientHeight={kbHeight}
		>
			<KeyboardVisual
				layout={config.keyboard.layout}
				keys={store.keys}
				legends={store.legends}
				size={config.keyboard.size}
				unitPx={kbUnit}
				decayMs={config.keyboard.decay_ms}
				showMouse={config.keyboard.mouse}
				buttons={store.mouse.buttons}
				wheel={store.mouse.wheel}
			/>
		</div>
	{/if}
	{#if pieces.mouse}
		<div
			class="ks-region is-mouse"
			style={anchorStyle(config.mouse.anchor, config.mouse.offset_x, config.mouse.offset_y, scale)}
			bind:clientWidth={mouseWidth}
			bind:clientHeight={mouseHeight}
		>
			<MouseDiagram
				buttons={store.mouse.buttons}
				wheel={store.mouse.wheel}
				size={MOUSE_DIAGRAM.heightPx * scale}
			/>
		</div>
	{/if}
	{#if pieces.callouts || indicators.length > 0}
		<div
			class="ks-region is-callouts"
			style={anchorStyle(config.callouts.anchor, config.callouts.offset_x, calloutsY, scale)}
		>
			<Callouts
				items={pieces.callouts ? store.callouts : []}
				config={config.callouts}
				{indicators}
			/>
		</div>
	{/if}
	{@render children?.()}
</div>
