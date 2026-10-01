<!--
	One status fact on Home: set into the surface (status, not a control),
	with a label above, a lamp and a word, and a line of detail.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { Tone } from '../lib/nav';
	import Icon from './Icon.svelte';
	import type { IconName } from './icons';

	interface Props {
		label: string;
		word: string;
		detail?: string;
		tone: Tone;
		icon: IconName;
		children?: Snippet;
	}

	let { label, word, detail, tone, icon, children }: Props = $props();
</script>

<div class="st-tile">
	<div class="st-tile-head">
		<span class="st-tile-icon"><Icon name={icon} size={18} /></span>
		<span class="plate-label">{label}</span>
	</div>
	<div class="st-tile-word">
		<span
			class="plate-lamp"
			class:is-off={tone === 'off'}
			class:is-amber={tone === 'warn'}
			class:is-red={tone === 'danger'}
			aria-hidden="true"
		></span>
		<span>{word}</span>
	</div>
	{#if detail}<p class="st-tile-detail">{detail}</p>{/if}
	{#if children}<div class="st-tile-extra">{@render children()}</div>{/if}
</div>

<style>
	.st-tile {
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
		min-width: 0;
		padding: 0.85rem 1rem 0.9rem;
		border-radius: var(--plate-r-control);
		background-color: var(--plate-chip-ground);
		background-image: linear-gradient(to bottom, var(--plate-chip-top), var(--plate-chip-bot));
		box-shadow:
			inset 0 0 0 1px var(--plate-chip-edge),
			inset 0 3px 4px -2px var(--plate-chip-shade),
			inset 0 -1px 0 0 var(--plate-chip-lip),
			0 1px 0 0 var(--plate-chip-lip);
	}
	.st-tile-head {
		display: flex;
		align-items: center;
		gap: 0.45rem;
		color: var(--plate-caption);
	}
	.st-tile-word {
		display: flex;
		align-items: center;
		gap: 0.55rem;
		font-weight: 700;
		font-size: 1.2rem;
		color: var(--plate-ink);
	}
	.st-tile-detail {
		margin: 0;
		font-size: 0.95rem;
		line-height: 1.35;
		color: var(--plate-ink-2);
		overflow-wrap: anywhere;
	}
	.st-tile-extra {
		margin-top: 0.25rem;
	}
	:global(.ks-plate) .st-tile .plate-lamp.is-red {
		--plate-led-color: var(--plate-led-red);
	}
</style>
