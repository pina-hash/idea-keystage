<!--
	Display: which monitor the overlay covers, the native overlay window, and
	the overall scale.
-->
<script lang="ts">
	import { LIMITS } from '../../lib/protocol';
	import { useController } from '../state.svelte';
	import { chosenMonitor, displayLabel, isMissing, logicalSize } from '../lib/monitors';
	import { formatScale } from '../lib/format';
	import PageTitle from '../ui/PageTitle.svelte';
	import Card from '../ui/Card.svelte';
	import Row from '../ui/Row.svelte';
	import Switch from '../ui/Switch.svelte';
	import Slider from '../ui/Slider.svelte';
	import Key from '../ui/Key.svelte';
	import Notice from '../ui/Notice.svelte';
	import MonitorMap from '../parts/MonitorMap.svelte';

	const ctl = useController();
	const s = $derived(ctl.settings!);
	const chosen = $derived(chosenMonitor(ctl.monitors, s.host.monitor));
	const missing = $derived(isMissing(ctl.monitors, s.host.monitor));
	const captureSupported = $derived(ctl.platform?.exclude_from_capture_supported ?? true);
	let refreshing = $state(false);

	async function refresh() {
		refreshing = true;
		await ctl.refreshMonitors();
		refreshing = false;
	}
</script>

<PageTitle
	title="Display"
	lead="Where the native overlay appears and how large everything is drawn."
/>

<div class="st-stack">
	<Card title="Monitor">
		{#snippet aside()}
			<Key icon="refresh" busy={refreshing} onclick={refresh}>Look again</Key>
		{/snippet}
		{#if ctl.monitors.length === 0}
			<div class="st-empty">
				<svg viewBox="0 0 64 44" width="96" height="66" aria-hidden="true">
					<rect
						x="4"
						y="3"
						width="56"
						height="32"
						rx="3"
						fill="none"
						stroke="currentColor"
						stroke-width="2"
					/>
					<path
						d="M24 41h16M32 35v6"
						stroke="currentColor"
						stroke-width="2"
						stroke-linecap="round"
					/>
					<path
						d="M24 14l16 10M40 14 24 24"
						stroke="currentColor"
						stroke-width="2"
						stroke-linecap="round"
					/>
				</svg>
				<p class="st-empty-title">No monitors reported</p>
				<p class="st-empty-text">
					Windows did not list any monitors just now. The overlay uses the primary monitor, and the
					OBS sizes assume 1920 x 1080 until a monitor is found. Plug in or wake the monitor, then
					look again.
				</p>
			</div>
		{:else}
			<MonitorMap
				monitors={ctl.monitors}
				value={s.host.monitor}
				onchoose={(id) => (s.host.monitor = id)}
			/>
			{#if missing}
				<Notice tone="warn" lead="Saved monitor not connected">
					The overlay was set to a monitor that is not connected, so it uses
					{chosen ? displayLabel(chosen, ctl.monitors.indexOf(chosen)) : 'the primary monitor'} for now.
				</Notice>
			{/if}
			{#if chosen}
				{@const logical = logicalSize(chosen)}
				<p class="st-mon-note">
					The overlay covers {displayLabel(chosen, ctl.monitors.indexOf(chosen))} ({chosen.width} x {chosen.height}).
					At {Math.round(chosen.scale * 100)}% Windows scaling the page is {logical.width} x {logical.height}.
				</p>
			{/if}
		{/if}
	</Card>

	<Card title="Native overlay">
		<Row
			label="Native overlay"
			help="A transparent, click-through window on the chosen monitor that draws the overlay over every app."
		>
			<Switch bind:checked={s.host.native_overlay} label="Native overlay" />
		</Row>
		<Row
			label="Step aside for OBS"
			help="While an OBS Browser Source is connected, the native overlay draws nothing, so you never record it twice."
			dim={!s.host.native_overlay}
		>
			<Switch
				bind:checked={s.host.hide_native_when_obs}
				label="Hide the native overlay while OBS sources are connected"
				disabled={!s.host.native_overlay}
			/>
		</Row>
		<Row
			label="Keep out of screen capture"
			help={captureSupported
				? 'Screen recorders and screen sharing skip the native overlay, so only you see it. Use this when OBS sources draw the overlay.'
				: 'Needs Windows 10 version 2004 or later. On this version Windows would record a black box instead.'}
			dim={!s.host.native_overlay || !captureSupported}
		>
			<Switch
				bind:checked={s.host.exclude_from_capture}
				label="Keep the native overlay out of screen capture"
				disabled={!s.host.native_overlay || !captureSupported}
			/>
		</Row>
	</Card>

	<Card title="Scale">
		<Row
			label="Overall scale"
			help="Everything grows or shrinks together. 1x is sized for a 1080p recording; use 2x for 4K."
		>
			<Slider
				bind:value={s.overlay.scale}
				min={LIMITS.scale.min}
				max={LIMITS.scale.max}
				step={0.05}
				label="Overall scale"
				format={formatScale}
			/>
		</Row>
	</Card>
</div>

<style>
	.st-stack {
		display: flex;
		flex-direction: column;
		gap: 1.6rem;
	}
	.st-empty {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.4rem;
		padding: 1.6rem 1rem 1.4rem;
		text-align: center;
		color: var(--plate-ink-2);
	}
	.st-empty-title {
		margin: 0.4rem 0 0;
		font-weight: 700;
		font-size: 1.15rem;
		color: var(--plate-ink);
	}
	.st-empty-text {
		margin: 0;
		max-width: 34rem;
		line-height: 1.45;
	}
	.st-mon-note {
		margin: 0 0 0.9rem;
		color: var(--plate-ink-2);
		font-size: 0.95rem;
	}
</style>
