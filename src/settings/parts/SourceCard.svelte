<!--
	One OBS Browser Source: what it shows, the exact URL with a Copy key, and
	the exact width and height to type into OBS.
-->
<script lang="ts">
	import type { SourceCard } from '../lib/obs';
	import CopyField from '../ui/CopyField.svelte';

	interface Props {
		card: SourceCard;
		copy: (text: string) => Promise<void>;
	}

	let { card, copy }: Props = $props();
</script>

<article class="plate-panel st-source" aria-label="{card.info.title} source">
	<header class="st-source-head">
		<div>
			<h3 class="st-source-title">{card.info.title}</h3>
			<p class="st-source-desc">{card.info.description}</p>
		</div>
		<div class="st-source-size" aria-label="Width {card.size.width}, height {card.size.height}">
			<span class="st-size-cell">
				<span class="plate-label">Width</span>
				<span class="st-size-num">{card.size.width}</span>
			</span>
			<span class="st-size-x" aria-hidden="true">x</span>
			<span class="st-size-cell">
				<span class="plate-label">Height</span>
				<span class="st-size-num">{card.size.height}</span>
			</span>
		</div>
	</header>
	<CopyField value={card.url} label="{card.info.title} source URL" {copy} />
</article>

<style>
	.st-source {
		display: flex;
		flex-direction: column;
		gap: 0.8rem;
		padding: 1rem 1.1rem 1.05rem;
	}
	.st-source-head {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 1rem;
	}
	.st-source-title {
		margin: 0;
		font-size: 1.15rem;
		font-weight: 700;
	}
	.st-source-desc {
		margin: 0.1rem 0 0;
		color: var(--plate-ink-2);
		font-size: 0.95rem;
		line-height: 1.35;
	}
	.st-source-size {
		flex: none;
		display: flex;
		align-items: flex-end;
		gap: 0.4rem;
	}
	.st-size-cell {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.2rem;
	}
	.st-size-num {
		min-width: 4.6rem;
		padding: 0.35rem 0.6rem;
		text-align: center;
		border-radius: var(--plate-r-small);
		font-family: var(--font-mono);
		font-size: 1.05rem;
		font-variant-numeric: tabular-nums;
		color: var(--plate-screen-ink);
		text-shadow: 0 0 6px var(--plate-screen-glow);
		background: linear-gradient(to bottom, var(--plate-screen-top), var(--plate-screen-bot));
		box-shadow:
			inset 0 0 0 1px var(--plate-screen-rim),
			inset 0 4px 8px -4px var(--plate-screen-shade);
	}
	.st-size-x {
		padding-bottom: 0.45rem;
		font-family: var(--font-mono);
		color: var(--plate-ink-2);
	}
</style>
