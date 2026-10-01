<!--
	One setting: its name and a short explanation on the left, the control on
	the right (or under the words when `stack` is set). Rows in a card are
	divided by etched lines.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		label: string;
		help?: string;
		/** The id of the control, so the name labels it. */
		for?: string;
		/** Put the control under the words (wide controls). */
		stack?: boolean;
		dim?: boolean;
		children: Snippet;
		/** Extra content under the help text (a notice, a list). */
		below?: Snippet;
		helpId?: string;
	}

	let {
		label,
		help,
		for: forId,
		stack = false,
		dim = false,
		children,
		below,
		helpId,
	}: Props = $props();
</script>

<div class="st-row" class:is-stack={stack} class:is-dim={dim}>
	<div class="st-row-words">
		{#if forId}
			<label class="st-row-label" for={forId}>{label}</label>
		{:else}
			<span class="st-row-label">{label}</span>
		{/if}
		{#if help}<p class="st-row-help" id={helpId}>{help}</p>{/if}
	</div>
	<div class="st-row-control">
		{@render children()}
	</div>
	{#if below}<div class="st-row-below">{@render below()}</div>{/if}
</div>

<style>
	.st-row {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto;
		align-items: center;
		gap: 0.6rem 1.5rem;
		padding: 0.95rem 0;
	}
	.st-row.is-stack {
		grid-template-columns: minmax(0, 1fr);
		align-items: start;
	}
	.st-row-words {
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
		min-width: 0;
	}
	.st-row-label {
		font-family: var(--font-display);
		font-weight: 600;
		font-size: 1.08rem;
		color: var(--plate-ink);
		letter-spacing: 0.01em;
	}
	.st-row-help {
		margin: 0;
		color: var(--plate-ink-2);
		font-size: 0.95rem;
		line-height: 1.4;
		max-width: 46rem;
	}
	.is-dim .st-row-label,
	.is-dim .st-row-help {
		color: var(--plate-disabled);
	}
	.st-row-control {
		display: flex;
		align-items: center;
		justify-content: flex-end;
		flex-wrap: wrap;
		gap: 0.5rem;
		min-width: 0;
	}
	.is-stack .st-row-control {
		justify-content: flex-start;
	}
	.st-row-below {
		grid-column: 1 / -1;
	}
</style>
