<!--
	The display: dark tinted glass with a glowing mono readout, its lower
	corners chamfered (plate.css .plate-screen). The one dark island on a
	Space White page. Used for live numbers: REC timecode, port, counts.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		children: Snippet;
		/** Read changes aloud. */
		live?: boolean;
		label?: string;
		/** The readout's color: green (the default), amber for a warning, red for a problem. The words say the same. */
		tone?: 'ok' | 'warn' | 'danger';
	}

	let { children, live = false, label, tone = 'ok' }: Props = $props();
</script>

<div
	class="plate-screen st-readout-screen is-{tone}"
	role={live ? 'status' : undefined}
	aria-label={label}
>
	{@render children()}
</div>

<style>
	:global(.ks-plate) .st-readout-screen {
		padding: 0.85rem 1.1rem 1.25rem;
		font-variant-numeric: tabular-nums;
		letter-spacing: 0.06em;
		background-image:
			linear-gradient(165deg, var(--plate-glass) 0 38%, transparent 62%),
			linear-gradient(to bottom, var(--plate-screen-top), var(--plate-screen-bot));
		box-shadow:
			inset 0 0 0 1px var(--plate-screen-rim),
			inset 5px 0 4px -3px var(--plate-screen-wall),
			inset -5px 0 4px -3px var(--plate-screen-wall),
			inset 0 6px 12px -6px var(--plate-screen-shade);
	}
	/* The screen is dark glass in both registers, so these inks are measured
	   against it in style.test.ts (screen-top #16201a and #383c44): 5.2 to 1
	   or better. */
	:global(.ks-plate) .st-readout-screen.is-warn {
		color: #f0b36a;
		text-shadow: 0 0 8px rgba(240, 179, 106, 0.55);
	}
	:global(.ks-plate) .st-readout-screen.is-danger {
		color: #f39a9a;
		text-shadow: 0 0 8px rgba(243, 154, 154, 0.55);
	}
</style>
