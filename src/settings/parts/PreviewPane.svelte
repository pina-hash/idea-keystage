<!--
	The live preview: the real renderer (Stage, RendererStore) showing the
	settings being edited at once (the store's pinned config). A close-up
	draws the screen at actual size around the piece being set up, and a
	thumbnail shows the whole monitor. Input comes from the built-in demo by
	default; "Live input" switches to the app's own overlay events. Below, the
	status strip keeps Pause and the session log at hand.
-->
<script lang="ts">
	import { untrack } from 'svelte';
	import Stage from '../../lib/renderer/Stage.svelte';
	import { RendererStore } from '../../lib/renderer/store.svelte';
	import { DemoTransport } from '../../lib/renderer/transport';
	import type { DemoScriptId } from '../../lib/renderer/options';
	import type { Route } from '../../lib/protocol';
	import { useController } from '../state.svelte';
	import { PAGE_INFO } from '../lib/nav';
	import { liveTransport } from '../lib/live';
	import { closeUp } from '../lib/preview';
	import { ANCHOR_LABELS } from '../lib/anchors';
	import { chosenMonitor, displayLabel, FALLBACK_MONITOR, logicalSize } from '../lib/monitors';
	import Segmented from '../ui/Segmented.svelte';
	import Switch from '../ui/Switch.svelte';
	import StatusChip from '../ui/StatusChip.svelte';
	import StatusStrip from './StatusStrip.svelte';

	const ctl = useController();

	let live = $state(false);
	let routeChoice = $state<'auto' | Route>('auto');
	let script = $state<DemoScriptId>('tour');
	let backdrop = $state<'light' | 'dark'>('light');
	let frameWidth = $state(640);
	let thumbWidth = $state(300);

	const route = $derived<Route>(
		routeChoice === 'auto' ? PAGE_INFO[ctl.page].previewRoute : routeChoice
	);
	const monitor = $derived(
		chosenMonitor(ctl.monitors, ctl.settings?.host.monitor ?? null) ?? FALLBACK_MONITOR
	);
	const monitorIndex = $derived(
		Math.max(
			0,
			ctl.monitors.findIndex((m) => m.id === monitor.id)
		)
	);
	const aspect = $derived(monitor.width / Math.max(1, monitor.height));
	const screen = $derived(logicalSize(monitor));
	const frame = $derived({
		width: Math.max(200, frameWidth),
		height: Math.round(Math.max(200, frameWidth) * 0.5625),
	});
	const close = $derived(ctl.settings ? closeUp(route, ctl.settings.overlay, screen, frame) : null);
	const thumbHeight = $derived(Math.round(thumbWidth / aspect));
	const thumbZoom = $derived(thumbWidth / Math.max(1, screen.width));

	let store = $state.raw<RendererStore | null>(null);

	$effect(() => {
		const r = route;
		const isLive = live;
		const sc = script;
		const s = new RendererStore({ route: r });
		const overlay = untrack(() => (ctl.settings ? $state.snapshot(ctl.settings.overlay) : null));
		if (overlay) s.pinConfig(overlay);
		s.connect(isLive ? liveTransport(ctl.ipc) : new DemoTransport({ script: sc, route: r }));
		store = s;
		return () => s.destroy();
	});

	$effect(() => {
		const overlay = ctl.settings ? $state.snapshot(ctl.settings.overlay) : null;
		if (overlay && store) store.pinConfig(overlay);
	});

	const connection = $derived(store?.connection ?? 'idle');
	const liveChip = $derived.by(() => {
		if (!live) return null;
		if (connection === 'live')
			return { tone: 'ok' as const, text: ctl.ipc.kind === 'mock' ? 'Simulated' : 'Connected' };
		if (connection === 'failed') return { tone: 'danger' as const, text: 'Not available' };
		return { tone: 'warn' as const, text: 'Waiting for the app' };
	});

	const PIECE_WORDS: Record<Route, string> = {
		keys: 'the keystrokes',
		keyboard: 'the keyboard',
		mouse: 'the mouse diagram',
		cursor: 'the cursor',
		all: 'the keystrokes',
	};

	const ROUTE_OPTIONS: { value: 'auto' | Route; label: string; hint: string }[] = [
		{ value: 'auto', label: 'Auto', hint: 'Follow the page you are on' },
		{ value: 'all', label: 'All', hint: 'Everything that is turned on' },
		{ value: 'keys', label: 'Keys', hint: 'The keystrokes source' },
		{ value: 'keyboard', label: 'Keyboard', hint: 'The keyboard source' },
		{ value: 'mouse', label: 'Mouse', hint: 'The mouse diagram source' },
		{ value: 'cursor', label: 'Cursor', hint: 'The cursor source' },
	];
	const SCRIPT_OPTIONS: { value: DemoScriptId; label: string; hint: string }[] = [
		{ value: 'tour', label: 'Tour', hint: 'SolidWorks, then general typing' },
		{ value: 'solidworks', label: 'SolidWorks', hint: 'Captions from the SolidWorks data' },
		{ value: 'general', label: 'General', hint: 'Shortcuts only, typing hidden' },
		{ value: 'states', label: 'Status', hint: 'The Paused and Hidden pills' },
	];
</script>

<section class="st-preview st-recessed" aria-labelledby="st-preview-title">
	<header class="st-preview-head">
		<h2 class="plate-label" id="st-preview-title">Live preview</h2>
		<div class="st-preview-head-right">
			{#if liveChip}<StatusChip tone={liveChip.tone} text={liveChip.text} live />{/if}
			<span class="st-live-word">Live input</span>
			<Switch bind:checked={live} label="Live input" describedby="st-live-help" />
		</div>
	</header>
	<p class="st-preview-help" id="st-live-help">
		{#if live}
			{ctl.ipc.kind === 'mock'
				? 'Outside the app, live input is simulated with the demo.'
				: 'Your own keys and mouse, as the overlay gets them. Nothing shows while paused or in a password field.'}
		{:else}
			A demo plays through your settings. Turn on live input to see your own keys.
		{/if}
	</p>

	<div class="st-control">
		<span class="plate-label">Show</span>
		<Segmented bind:value={routeChoice} options={ROUTE_OPTIONS} label="What the preview shows" />
	</div>

	<div class="st-frame" bind:clientWidth={frameWidth}>
		<div
			class="st-box"
			class:is-dark={backdrop === 'dark'}
			style:height="{frame.height}px"
			role="img"
			aria-label="Close-up of {PIECE_WORDS[route]}, {close?.label ?? ''}"
		>
			<div class="st-shade" aria-hidden="true"></div>
			{#if store && close}
				<div
					class="st-zoomed"
					style:left="{close.left}px"
					style:top="{close.top}px"
					style:width="{close.stageWidth}px"
					style:height="{close.stageHeight}px"
				>
					<Stage {store} host="preview" zoom={close.zoom} {aspect} />
				</div>
			{/if}
		</div>
	</div>
	<p class="st-caption">
		<span>Close-up</span>
		<span class="st-dot" aria-hidden="true"></span>
		<span>{close?.label}</span>
		<span class="st-dot" aria-hidden="true"></span>
		<span>{close ? ANCHOR_LABELS[close.anchor] : ''}</span>
	</p>

	<div class="st-lower">
		<div class="st-thumb-col">
			<div class="st-frame is-thumb" bind:clientWidth={thumbWidth}>
				<div
					class="st-box"
					class:is-dark={backdrop === 'dark'}
					style:height="{thumbHeight}px"
					role="img"
					aria-label="The whole of {displayLabel(monitor, monitorIndex)}"
				>
					<div class="st-shade" aria-hidden="true"></div>
					{#if store}<Stage {store} host="preview" zoom={thumbZoom} {aspect} />{/if}
				</div>
			</div>
			<p class="st-caption">
				<span>{ctl.monitors.length ? displayLabel(monitor, monitorIndex) : 'Primary monitor'}</span>
				<span class="st-dot" aria-hidden="true"></span>
				<span>{monitor.width} x {monitor.height}</span>
			</p>
		</div>
		<div class="st-lower-controls">
			{#if !live}
				<div class="st-control is-stack">
					<span class="plate-label">Demo</span>
					<Segmented bind:value={script} options={SCRIPT_OPTIONS} label="Demo" columns={2} />
				</div>
			{/if}
			<div class="st-control is-stack">
				<span class="plate-label">Backdrop</span>
				<Segmented
					bind:value={backdrop}
					options={[
						{ value: 'light', label: 'Light', icon: 'sun', hint: 'A light model view' },
						{ value: 'dark', label: 'Dark', icon: 'moon', hint: 'A dark model view' },
					]}
					label="Preview backdrop"
					columns={2}
				/>
			</div>
		</div>
	</div>

	<StatusStrip />
</section>

<style>
	.st-preview {
		display: flex;
		flex-direction: column;
		gap: 0.7rem;
		min-height: 0;
		height: 100%;
		overflow-y: auto;
		padding: 1rem 1.25rem 1.1rem;
	}
	.st-preview-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.75rem;
		min-height: 44px;
	}
	.st-preview-head h2 {
		margin: 0;
		font-weight: 400;
	}
	.st-preview-head-right {
		display: flex;
		align-items: center;
		gap: 0.6rem;
	}
	.st-live-word {
		font-weight: 600;
		color: var(--plate-ink);
	}
	.st-preview-help {
		margin: -0.35rem 0 0;
		color: var(--plate-ink-2);
		font-size: 0.95rem;
	}
	.st-control {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		flex-wrap: wrap;
	}
	.st-control > .plate-label {
		width: 3.6rem;
		flex: none;
	}
	.st-control.is-stack {
		flex-direction: column;
		align-items: flex-start;
		gap: 0.35rem;
	}
	.st-control.is-stack > .plate-label {
		width: auto;
	}
	.st-frame {
		padding: 9px;
		border-radius: 14px;
		border: 1px solid var(--plate-hair);
		background: linear-gradient(to bottom, var(--plate-panel-top), var(--plate-panel-bot));
		box-shadow:
			inset 0 1px 0 0 var(--plate-panel-hi),
			inset 0 0 0 3px var(--plate-frame),
			inset 0 -4px 4px -2px var(--plate-frame-lo),
			0 0 0 1px var(--plate-rim-out),
			0 10px 24px -8px var(--plate-drop);
	}
	.st-frame.is-thumb {
		padding: 6px;
		border-radius: 10px;
	}
	.st-box {
		position: relative;
		overflow: hidden;
		border-radius: 6px;
		background: linear-gradient(to bottom, #c9d0d8 0%, #e4e8ec 55%, #f1f3f5 100%);
		box-shadow:
			0 0 0 1px rgba(0, 0, 0, 0.55),
			inset 0 0 0 1px rgba(255, 255, 255, 0.08);
	}
	.st-box.is-dark {
		background: linear-gradient(to bottom, #39414a 0%, #23282d 60%, #1a1d21 100%);
	}
	.st-shade {
		position: absolute;
		inset: 0;
		pointer-events: none;
		background: radial-gradient(ellipse at 50% 42%, rgba(255, 255, 255, 0.35), transparent 60%);
	}
	.is-dark .st-shade {
		background: radial-gradient(ellipse at 50% 40%, rgba(255, 255, 255, 0.06), transparent 60%);
	}
	.st-zoomed {
		position: absolute;
	}
	.st-caption {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.5rem;
		margin: -0.3rem 0 0;
		font-family: var(--font-mono);
		font-size: 0.72rem;
		letter-spacing: 0.08em;
		text-transform: uppercase;
		color: var(--plate-caption);
	}
	.st-dot {
		width: 4px;
		height: 4px;
		border-radius: 50%;
		background: var(--plate-caption);
	}
	.st-lower {
		display: grid;
		grid-template-columns: minmax(0, 0.95fr) minmax(0, 1.05fr);
		gap: 1rem;
		align-items: start;
	}
	.st-thumb-col {
		display: flex;
		flex-direction: column;
		gap: 0.6rem;
	}
	.st-lower-controls {
		display: flex;
		flex-direction: column;
		gap: 0.7rem;
	}
	.st-lower-controls :global(.st-tray) {
		width: 100%;
	}
</style>
