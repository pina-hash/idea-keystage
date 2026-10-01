<!--
	A shortcut shown as text: each key a small recessed legend (status, not a
	button), joined by "+". The whole combo is also given as one string for
	assistive technology.
-->
<script lang="ts">
	interface Props {
		parts: readonly string[];
		/** The separator between keys ("+" for a combo, "," for a sequence). */
		joiner?: string;
		size?: 's' | 'm';
	}

	let { parts, joiner = '+', size = 'm' }: Props = $props();
	const text = $derived(parts.join(` ${joiner} `));
</script>

<span class="st-combo is-{size}" aria-label={text} role="img">
	{#each parts as p, i (i)}
		{#if i > 0}<span class="st-combo-join" aria-hidden="true">{joiner}</span>{/if}
		<kbd class="st-kbd" aria-hidden="true">{p}</kbd>
	{/each}
</span>

<style>
	.st-combo {
		display: inline-flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 4px;
	}
	.st-kbd {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		min-width: 26px;
		height: 26px;
		padding: 0 7px;
		border-radius: var(--plate-r-small);
		font-family: var(--font-mono);
		font-size: 0.8rem;
		letter-spacing: 0.02em;
		line-height: 1;
		white-space: nowrap;
		color: var(--plate-ink);
		background-color: var(--plate-chip-ground);
		background-image: linear-gradient(to bottom, var(--plate-chip-top), var(--plate-chip-bot));
		box-shadow:
			inset 0 0 0 1px var(--plate-chip-edge),
			inset 0 2px 2px -1px var(--plate-chip-shade),
			inset 0 -2px 0 0 var(--plate-chip-lip),
			0 1px 0 0 var(--plate-chip-lip);
	}
	.is-s .st-kbd {
		min-width: 22px;
		height: 22px;
		padding: 0 5px;
		font-size: 0.72rem;
	}
	.st-combo-join {
		font-family: var(--font-mono);
		font-size: 0.8rem;
		color: var(--plate-ink-2);
	}
</style>
