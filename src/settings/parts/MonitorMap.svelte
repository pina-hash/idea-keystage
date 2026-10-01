<!--
	The monitors drawn the way Windows arranges them, in proportion, each a
	raised key showing its number, name, resolution and scale. The chosen one
	is lit (accent ring, "Overlay here" and aria-checked). The primary
	monitor carries a "Primary" tag.
-->
<script lang="ts">
	import type { Monitor } from '../types';
	import { displayLabel, monitorMap, scaleLabel } from '../lib/monitors';
	import Icon from '../ui/Icon.svelte';

	interface Props {
		monitors: readonly Monitor[];
		value: string | null;
		onchoose: (id: string) => void;
		height?: number;
	}

	let { monitors, value, onchoose, height = 230 }: Props = $props();

	const map = $derived(monitorMap(monitors, 1.4));
	let keys: HTMLButtonElement[] = $state([]);
	let outerWidth = $state(600);
	const innerWidth = $derived(Math.max(120, Math.min(outerWidth - 16, height * map.aspect)));
	const innerHeight = $derived(innerWidth / map.aspect);

	function chosenId(): string | null {
		if (value && monitors.some((m) => m.id === value)) return value;
		return monitors.find((m) => m.primary)?.id ?? monitors[0]?.id ?? null;
	}

	function onkeydown(e: KeyboardEvent, index: number) {
		let next = -1;
		if (e.key === 'ArrowRight' || e.key === 'ArrowDown') next = (index + 1) % monitors.length;
		else if (e.key === 'ArrowLeft' || e.key === 'ArrowUp')
			next = (index - 1 + monitors.length) % monitors.length;
		if (next < 0) return;
		e.preventDefault();
		const m = monitors[next];
		if (!m) return;
		onchoose(m.id);
		keys[next]?.focus();
	}
</script>

<div
	class="st-monmap"
	role="radiogroup"
	aria-label="Monitor for the overlay"
	bind:clientWidth={outerWidth}
>
	<div class="st-monmap-inner" style:width="{innerWidth}px" style:height="{innerHeight}px">
		{#each monitors as m, i (m.id)}
			{@const box = map.boxes[i]}
			{@const on = chosenId() === m.id}
			{#if box}
				<button
					bind:this={keys[i]}
					type="button"
					role="radio"
					aria-checked={on}
					tabindex={on ? 0 : -1}
					class="plate-key st-monitor"
					style:left="{box.left}%"
					style:top="{box.top}%"
					style:width="{box.width}%"
					style:height="{box.height}%"
					onclick={() => onchoose(m.id)}
					onkeydown={(e) => onkeydown(e, i)}
				>
					<span class="st-mon-num" aria-hidden="true">{i + 1}</span>
					<span class="st-mon-words">
						<span class="st-mon-name">{displayLabel(m, i)}</span>
						<span class="st-mon-detail">{m.name}</span>
						<span class="st-mon-detail">{m.width} x {m.height}, {scaleLabel(m.scale)}</span>
						<span class="st-mon-tags">
							{#if m.primary}<span class="plate-chip">Primary</span>{/if}
							{#if on}<span class="st-mon-on"><Icon name="check" size={15} />Overlay here</span
								>{/if}
						</span>
					</span>
				</button>
			{/if}
		{/each}
	</div>
</div>

<style>
	.st-monmap {
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 1rem 0.5rem;
	}
	.st-monmap-inner {
		position: relative;
	}
	:global(.ks-plate) .st-monitor {
		position: absolute;
		display: flex;
		align-items: flex-start;
		justify-content: flex-start;
		gap: 0.6rem;
		padding: 0.6rem 0.7rem;
		min-width: 0;
		min-height: 0;
		overflow: hidden;
		text-align: left;
		font-weight: 500;
		letter-spacing: 0;
		white-space: normal;
	}
	.st-mon-num {
		flex: none;
		font-family: var(--font-title);
		font-weight: 700;
		font-size: 1.6rem;
		line-height: 1;
		color: var(--plate-ink-2);
	}
	.st-monitor[aria-checked='true'] .st-mon-num {
		color: var(--plate-accent);
	}
	.st-mon-words {
		display: flex;
		flex-direction: column;
		gap: 0.1rem;
		min-width: 0;
	}
	.st-mon-name {
		font-weight: 700;
		font-size: 1.05rem;
		color: var(--plate-ink);
	}
	.st-mon-detail {
		font-size: 0.88rem;
		color: var(--plate-ink-2);
		line-height: 1.25;
	}
	.st-mon-tags {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.4rem;
		margin-top: 0.35rem;
	}
	.st-mon-on {
		display: inline-flex;
		align-items: center;
		gap: 0.2rem;
		font-family: var(--font-mono);
		font-size: 0.7rem;
		letter-spacing: 0.1em;
		text-transform: uppercase;
		color: var(--plate-mark-ink);
	}
</style>
