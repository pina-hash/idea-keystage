<!--
	A section: its label sits above it (IDEA Classroom's rule), the content
	sits in a raised panel. `aside` puts a status chip or a key at the right
	end of the label line.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		title: string;
		id?: string;
		/** One sentence under the label. */
		lead?: string;
		aside?: Snippet;
		children: Snippet;
		/** No inner padding (lists and tables that reach the edges). */
		flush?: boolean;
	}

	let { title, id, lead, aside, children, flush = false }: Props = $props();
	const uid = $props.id();
</script>

<section class="st-card" aria-labelledby="{uid}-t" {id}>
	<header class="st-card-head">
		<h2 class="plate-label st-card-title" id="{uid}-t">{title}</h2>
		{#if aside}<div class="st-card-aside">{@render aside()}</div>{/if}
	</header>
	{#if lead}<p class="st-card-lead">{lead}</p>{/if}
	<div class="plate-panel st-card-body" class:is-flush={flush}>
		{@render children()}
	</div>
</section>

<style>
	.st-card {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}
	.st-card-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		min-height: 24px;
		padding: 0 0.25rem;
	}
	.st-card-title {
		margin: 0;
		font-weight: 400;
	}
	.st-card-aside {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}
	.st-card-lead {
		margin: 0 0.25rem;
		color: var(--plate-ink-2);
		font-size: 0.98rem;
		line-height: 1.45;
	}
	.st-card-body {
		padding: 0.4rem 1.25rem;
	}
	.st-card-body.is-flush {
		padding: 0;
	}
</style>
