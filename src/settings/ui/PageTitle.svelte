<!--
	A page's title bar (spaced mono caps between two hazard blocks, see
	settings.css .st-title) and one sentence about the page under it.
-->
<script lang="ts">
	interface Props {
		title: string;
		lead?: string;
	}

	let { title, lead }: Props = $props();
</script>

<header class="st-page-head">
	<h1 class="st-title" tabindex="-1">{title}</h1>
	{#if lead}<p class="st-page-lead">{lead}</p>{/if}
</header>

<style>
	.st-page-head {
		display: flex;
		flex-direction: column;
		gap: 0.7rem;
		margin-bottom: 1.5rem;
	}
	/* The title takes focus when a page opens (so a screen reader starts
	   there); it is not a control, so it draws no focus ring. */
	.st-page-head h1:focus,
	.st-page-head h1:focus-visible {
		outline: none;
	}
	.st-page-lead {
		margin: 0;
		text-align: center;
		color: var(--plate-ink-2);
		font-size: 1.02rem;
		line-height: 1.45;
		text-wrap: balance;
	}
</style>
