<!--
	A status tag set into the surface (plate.css .plate-chip) with a lamp. The
	word carries the meaning; the lamp's color and the lamp being lit or dark
	only repeat it.
-->
<script lang="ts">
	import type { Tone } from '../lib/nav';

	interface Props {
		tone: Tone;
		text: string;
		/** Read the change aloud (for live status). */
		live?: boolean;
		title?: string;
	}

	let { tone, text, live = false, title }: Props = $props();
</script>

<span class="plate-chip st-chip" {title} role={live ? 'status' : undefined}>
	<span
		class="plate-lamp"
		class:is-off={tone === 'off'}
		class:is-amber={tone === 'warn'}
		class:is-red={tone === 'danger'}
		class:is-info={tone === 'info'}
		aria-hidden="true"
	></span>
	<span class="st-chip-word">{text}</span>
</span>

<style>
	:global(.ks-plate) .st-chip {
		min-height: 26px;
		padding: 0 8px 0 7px;
		gap: 7px;
		white-space: nowrap;
	}
	.st-chip-word {
		padding-top: 1px;
	}
	:global(.ks-plate) .plate-lamp.is-info {
		--plate-led-color: var(--plate-focus);
	}
</style>
