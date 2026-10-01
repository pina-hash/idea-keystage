<!--
	Where a piece sits on the screen: nine keys laid out inside a small screen
	frame. The chosen key is lit (accent ring, a solid marker, aria-checked)
	and its name is written under the frame. Arrow keys move in two
	dimensions.
-->
<script lang="ts">
	import { ANCHORS, type Anchor } from '../../lib/protocol';
	import { ANCHOR_LABELS } from '../lib/anchors';

	interface Props {
		value: Anchor;
		label: string;
		id?: string;
		disabled?: boolean;
		onchange?: (v: Anchor) => void;
	}

	let { value = $bindable(), label, id, disabled = false, onchange }: Props = $props();

	let buttons: HTMLButtonElement[] = $state([]);

	function choose(a: Anchor) {
		if (disabled || a === value) return;
		value = a;
		onchange?.(a);
	}

	function onkeydown(e: KeyboardEvent, index: number) {
		const row = Math.floor(index / 3);
		const col = index % 3;
		let r = row;
		let c = col;
		if (e.key === 'ArrowLeft') c = Math.max(0, col - 1);
		else if (e.key === 'ArrowRight') c = Math.min(2, col + 1);
		else if (e.key === 'ArrowUp') r = Math.max(0, row - 1);
		else if (e.key === 'ArrowDown') r = Math.min(2, row + 1);
		else return;
		e.preventDefault();
		const next = r * 3 + c;
		const a = ANCHORS[next];
		if (!a) return;
		choose(a);
		buttons[next]?.focus();
	}
</script>

<div class="st-anchor">
	<div class="st-anchor-frame" role="radiogroup" aria-label={label} {id}>
		{#each ANCHORS as a, i (a)}
			{@const on = a === value}
			<button
				bind:this={buttons[i]}
				type="button"
				role="radio"
				class="plate-key st-anchor-key"
				aria-checked={on}
				aria-label={ANCHOR_LABELS[a]}
				title={ANCHOR_LABELS[a]}
				tabindex={on ? 0 : -1}
				{disabled}
				onclick={() => choose(a)}
				onkeydown={(e) => onkeydown(e, i)}
			>
				<span class="st-anchor-mark" class:is-on={on} aria-hidden="true"></span>
			</button>
		{/each}
	</div>
	<span class="st-anchor-name">{ANCHOR_LABELS[value]}</span>
</div>

<style>
	.st-anchor {
		display: inline-flex;
		flex-direction: column;
		align-items: center;
		gap: 0.45rem;
	}
	.st-anchor-frame {
		display: grid;
		grid-template-columns: repeat(3, 44px);
		grid-template-rows: repeat(3, 44px);
		justify-content: space-between;
		align-content: space-between;
		width: 236px;
		height: 160px;
		padding: 10px;
		border: 1px solid var(--plate-hair);
		border-radius: var(--plate-r-control);
		background-color: var(--plate-well-ground);
		background-image: linear-gradient(to bottom, var(--plate-well-top), var(--plate-well-face) 30%);
		box-shadow:
			inset 0 1px 0 0 var(--plate-well-shade),
			inset 0 2px 3px -1px var(--plate-well-shade),
			0 1px 0 0 var(--plate-well-lip);
	}
	:global(.ks-plate) .st-anchor-key {
		width: 44px;
		height: 44px;
		min-width: 44px;
		padding: 0;
	}
	.st-anchor-mark {
		display: block;
		width: 12px;
		height: 12px;
		border-radius: 3px;
		border: 2px solid var(--plate-ink-2);
	}
	.st-anchor-mark.is-on {
		width: 16px;
		height: 16px;
		border-color: var(--plate-accent);
		background: var(--plate-accent);
		box-shadow: 0 0 6px 1px var(--plate-accent-tint);
	}
	.st-anchor-name {
		font-family: var(--font-mono);
		font-size: var(--plate-label-size);
		letter-spacing: var(--plate-label-track);
		text-transform: uppercase;
		color: var(--plate-caption);
	}
</style>
