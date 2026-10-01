<!--
	A status pill (Paused, Hidden): an inset status plate with a lamp, a glyph
	and the word, so color is never the only signal.
-->
<script lang="ts">
	import type { IndicatorView } from './pieces';

	interface Props {
		view: IndicatorView;
	}

	let { view }: Props = $props();
</script>

<div
	class="ks-indicator"
	class:is-paused={view.kind === 'paused'}
	class:is-hidden={view.kind === 'hidden'}
>
	<span class="ks-indicator-lamp" aria-hidden="true"></span>
	<svg class="ks-indicator-glyph" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
		{#if view.kind === 'paused'}
			<path d="M9 6v12M15 6v12" />
		{:else}
			<path
				d="M3 12s3.5-6 9-6 9 6 9 6-3.5 6-9 6-9-6-9-6zM12 9.2a2.8 2.8 0 1 0 0 5.6 2.8 2.8 0 0 0 0-5.6zM4 20 20 4"
			/>
		{/if}
	</svg>
	<span class="ks-indicator-word">{view.kind === 'paused' ? 'Paused' : 'Hidden'}</span>
	{#if view.detail}
		<span class="ks-indicator-detail">{view.detail}</span>
	{/if}
</div>
