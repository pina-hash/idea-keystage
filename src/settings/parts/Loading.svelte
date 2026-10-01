<!--
	Shown while the settings load, or when the app could not be reached.
-->
<script lang="ts">
	import Mark from './Mark.svelte';
	import Key from '../ui/Key.svelte';
	import Notice from '../ui/Notice.svelte';

	interface Props {
		error: string | null;
		retry: () => void;
	}

	let { error, retry }: Props = $props();
</script>

<div class="st-loading">
	<div class="plate-housing st-loading-card">
		<Mark />
		{#if error}
			<Notice tone="danger" lead="Could not load the settings" announce>
				{error}
			</Notice>
			<Key icon="refresh" variant="primary" onclick={retry}>Try again</Key>
		{:else}
			<p class="st-loading-text" role="status">Loading your settings</p>
		{/if}
	</div>
</div>

<style>
	.st-loading {
		grid-column: 1 / -1;
		display: grid;
		place-items: center;
		min-height: 100vh;
		padding: 2rem;
	}
	.st-loading-card {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 1.1rem;
		width: min(520px, 100%);
		padding: 1.75rem;
	}
	.st-loading-text {
		margin: 0;
		font-family: var(--font-mono);
		font-size: 0.8rem;
		letter-spacing: 0.12em;
		text-transform: uppercase;
		color: var(--plate-caption);
	}
</style>
