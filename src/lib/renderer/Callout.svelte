<!--
	One keystroke callout: a raised plate holding the keycaps ("+" between
	keys of a combo, "," between chord steps), the repeat badge, the typed-text
	readout or the masked Typing tag, and the caption line (command name and
	an optional small context label). Styles: src/lib/renderer/callouts.css.
-->
<script lang="ts">
	import type { CalloutItem } from './logic/callouts';
	import Keycap from './Keycap.svelte';
	import { calloutText, repeatText, textRunDisplay } from './glyphs';

	interface Props {
		item: CalloutItem;
		symbols: boolean;
		captions: boolean;
	}

	let { item, symbols, captions }: Props = $props();

	const c = $derived(item.callout);
	const steps = $derived(c.steps.filter((s) => s.length > 0));
	const badge = $derived(repeatText(c.count));
	const caption = $derived(captions && c.caption ? c.caption : null);
	const context = $derived(caption && c.context ? c.context : null);
	const label = $derived(
		[calloutText(c), caption, context].filter((s): s is string => !!s).join('. ')
	);
</script>

<div
	class="ks-callout"
	class:is-fading={item.phase === 'fading'}
	class:is-leaving={item.phase === 'leaving'}
	class:is-text={c.kind === 'text'}
	class:is-masked={c.kind === 'masked'}
	role="img"
	aria-label={label}
>
	<div class="ks-keys">
		{#if c.kind === 'masked'}
			<span class="ks-typing">
				<svg class="ks-typing-glyph" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
					<path
						d="M3.5 7.5h17a1 1 0 0 1 1 1v8a1 1 0 0 1-1 1h-17a1 1 0 0 1-1-1v-8a1 1 0 0 1 1-1zM6 11h1M9 11h1M12 11h1M15 11h1M18 11h1M8 14.5h8"
					/>
				</svg>
				<span class="ks-typing-word">Typing</span>
				<span class="ks-typing-dots" aria-hidden="true"><i></i><i></i><i></i></span>
			</span>
		{/if}
		{#if c.kind !== 'masked' || steps.length > 0}
			{#if c.kind === 'text' && c.text}
				<span class="ks-screen"
					><span class="ks-screen-face"
						><span class="ks-screen-text">{textRunDisplay(c.text)}</span></span
					></span
				>
			{/if}
			{#key c.kind === 'text' || c.kind === 'masked' ? 0 : c.count}
				<span class="ks-steps">
					{#each steps as step, si (si)}
						{#if si > 0}<span class="ks-join is-comma" aria-hidden="true">,</span>{/if}
						{#each step as chip, ci (ci)}
							{#if ci > 0}<span class="ks-join" aria-hidden="true">+</span>{/if}
							<Keycap {chip} {symbols} />
						{/each}
					{/each}
				</span>
			{/key}
		{/if}
		{#if badge}
			{#key c.count}
				<span class="ks-badge">{badge}</span>
			{/key}
		{/if}
	</div>
	{#if caption}
		<div class="ks-caption">
			<span class="ks-caption-text">{caption}</span>
			{#if context}<span class="ks-caption-context">{context}</span>{/if}
		</div>
	{/if}
</div>
