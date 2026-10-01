<!--
	One chip of a callout: a raised keycap for a key, a modifier keycap (its
	own face, an indicator lamp and a mono legend, so it differs by shape and
	type as well as color), or a mouse chip with a small mouse glyph.
	Styles: src/lib/renderer/callouts.css.
-->
<script lang="ts">
	import type { Chip } from '../protocol';
	import { GLYPHS, glyphFor, isShortLabel, mouseChipInfo } from './glyphs';
	import MouseGlyph from './MouseGlyph.svelte';

	interface Props {
		chip: Chip;
		/** Draw arrows, Enter, Tab and Backspace as glyphs. */
		symbols: boolean;
	}

	let { chip, symbols }: Props = $props();

	const glyph = $derived(symbols && chip.kind !== 'mouse' ? glyphFor(chip.key) : null);
	const mouse = $derived(chip.kind === 'mouse' ? mouseChipInfo(chip) : null);
	const short = $derived(chip.kind === 'key' && glyph === null && isShortLabel(chip.label));
	const wide = $derived(chip.kind === 'key' && chip.key === 'Space');
</script>

<span
	class="ks-cap"
	class:is-mod={chip.kind === 'modifier'}
	class:is-mouse={chip.kind === 'mouse'}
	class:is-short={short}
	class:is-wide={wide}
	class:is-glyph={glyph !== null}
>
	{#if chip.kind === 'modifier'}
		<span class="ks-cap-lamp" aria-hidden="true"></span>
	{/if}
	{#if mouse}
		<MouseGlyph part={mouse.part} hint={mouse.hint} />
	{/if}
	{#if glyph}
		{#if glyph.prefix}<span class="ks-cap-prefix" aria-hidden="true">{glyph.prefix}</span>{/if}
		<svg class="ks-cap-glyph" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
			<path d={GLYPHS[glyph.id].d} />
		</svg>
		<span class="ks-sr">{glyph.words}</span>
	{:else}
		<span class="ks-cap-label">{chip.label}</span>
	{/if}
</span>
