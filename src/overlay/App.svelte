<!--
	The overlay page. Draws the stage for the page's route. A WebSocket page
	that is not live draws no pieces (no stale or default keyboard), and
	either nothing more (inside OBS, so a status banner is never recorded) or
	the "Waiting for IDEA Keystage" card (in a browser, or with status=1).
	Host A and the demo always draw.
-->
<script lang="ts">
	import Stage from '../lib/renderer/Stage.svelte';
	import type { RendererStore } from '../lib/renderer/store.svelte';
	import type { PageOptions } from '../lib/renderer/options';
	import { drawsPieces, showWaitingCard } from './status';
	import WaitingCard from './WaitingCard.svelte';

	interface Props {
		store: RendererStore;
		options: PageOptions;
		inObs: boolean;
	}

	let { store, options, inObs }: Props = $props();

	const card = $derived(
		showWaitingCard({
			connection: store.connection,
			transport: options.transport,
			inObs,
			status: options.status,
		})
	);
	const live = $derived(
		drawsPieces({ connection: store.connection, transport: options.transport })
	);
	const port = $derived(window.location.port || '47820');
</script>

<Stage {store} host={options.host} {live}>
	{#if card}
		<WaitingCard connection={store.connection} {port} host={window.location.hostname} />
	{/if}
</Stage>
