<!--
	A small mouse drawn in SVG for mouse chips. The part the chip is about is
	filled with the accent; a wheel chip also gets a chevron for its direction.
	Decorative: the chip's label carries the words.
-->
<script lang="ts">
	import type { MouseHint, MousePart } from './glyphs';

	interface Props {
		part: MousePart | null;
		hint: MouseHint;
	}

	let { part, hint }: Props = $props();
</script>

<svg class="ks-mouse" viewBox="0 0 24 32" aria-hidden="true" focusable="false">
	<path
		class="ks-mouse-body"
		d="M12 2C6.6 2 4 5.6 4 10.6v10.8C4 26.8 7.4 30 12 30s8-3.2 8-8.6V10.6C20 5.6 17.4 2 12 2z"
	/>
	{#if part === 'left'}
		<path class="ks-mouse-hit" d="M12 2C6.6 2 4 5.6 4 10.6V13h8z" />
	{:else if part === 'right'}
		<path class="ks-mouse-hit" d="M12 2c5.4 0 8 3.6 8 8.6V13h-8z" />
	{:else if part === 'x1'}
		<rect class="ks-mouse-hit" x="1.6" y="19" width="2.4" height="3.6" rx="0.8" />
	{:else if part === 'x2'}
		<rect class="ks-mouse-hit" x="1.6" y="14.4" width="2.4" height="3.6" rx="0.8" />
	{/if}
	<path class="ks-mouse-line" d="M4 13h16M12 2v3.2M12 10.4V13" />
	<rect
		class="ks-mouse-wheel"
		class:is-hit={part === 'wheel' || part === 'middle'}
		x="10.4"
		y="5"
		width="3.2"
		height="5.4"
		rx="1.6"
	/>
	{#if part === 'wheel' && hint === 'up'}
		<path class="ks-mouse-chev" d="M9.6 4.2 12 1.8l2.4 2.4" />
	{:else if part === 'wheel' && hint === 'down'}
		<path class="ks-mouse-chev" d="M9.6 12.4 12 14.8l2.4-2.4" />
	{/if}
</svg>
