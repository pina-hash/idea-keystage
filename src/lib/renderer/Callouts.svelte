<!--
	The keystroke callout stack. Items arrive oldest first; with direction
	"up" the newest sits at the bottom (the anchor edge of a bottom anchor),
	with "down" at the top. Neighbors slide with a FLIP animation when an item
	arrives or leaves; with reduced motion they move at once.
-->
<script lang="ts">
	import { flip } from 'svelte/animate';
	import type { CalloutsConfig } from '../protocol';
	import type { CalloutItem } from './logic/callouts';
	import type { IndicatorView } from './pieces';
	import { flexAlign } from './layout';
	import Callout from './Callout.svelte';
	import Indicator from './Indicator.svelte';
	import { MediaQuery } from 'svelte/reactivity';

	interface Props {
		items: readonly CalloutItem[];
		config: CalloutsConfig;
		indicators: readonly IndicatorView[];
	}

	let { items, config, indicators }: Props = $props();

	const reduced = new MediaQuery('prefers-reduced-motion: reduce');
	const ordered = $derived(config.direction === 'up' ? items : [...items].reverse());
	const flipMs = $derived(reduced.current ? 0 : 180);
</script>

<div
	class="ks-stack"
	class:is-down={config.direction === 'down'}
	style:align-items={flexAlign(config.anchor)}
>
	{#if config.direction === 'down'}
		{#each indicators as view (view.kind)}
			<Indicator {view} />
		{/each}
	{/if}
	{#each ordered as item (item.id)}
		<div class="ks-slot" animate:flip={{ duration: flipMs }}>
			<Callout {item} symbols={config.symbols} captions={config.captions} />
		</div>
	{/each}
	{#if config.direction === 'up'}
		{#each indicators as view (view.kind)}
			<Indicator {view} />
		{/each}
	{/if}
</div>
