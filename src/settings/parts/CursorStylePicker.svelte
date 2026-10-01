<!--
	The cursor highlight styles as raised cards, each with a vector drawing of
	the style around a pointer in the chosen color and opacity. The chosen
	card is lit (accent ring, "In use", aria-checked).
-->
<script lang="ts">
	import type { CursorStyle } from '../../lib/protocol';
	import Icon from '../ui/Icon.svelte';

	interface Props {
		value: CursorStyle;
		color: string;
		opacity: number;
		onchoose: (s: CursorStyle) => void;
	}

	let { value, color, opacity, onchoose }: Props = $props();

	const STYLES: { id: CursorStyle; name: string; description: string }[] = [
		{ id: 'ring', name: 'Ring', description: 'A ring around the pointer.' },
		{ id: 'spotlight', name: 'Spotlight', description: 'Dims the screen around the pointer.' },
		{ id: 'dot', name: 'Dot', description: 'A soft dot under the pointer.' },
	];

	let cards: HTMLButtonElement[] = $state([]);
	const POINTER = 'M60 33v27l6.5-6.2 4.4 10 4-1.8-4.4-9.8h9z';

	function onkeydown(e: KeyboardEvent, index: number) {
		let next = -1;
		if (e.key === 'ArrowRight' || e.key === 'ArrowDown') next = (index + 1) % STYLES.length;
		else if (e.key === 'ArrowLeft' || e.key === 'ArrowUp')
			next = (index - 1 + STYLES.length) % STYLES.length;
		if (next < 0) return;
		e.preventDefault();
		const st = STYLES[next];
		if (!st) return;
		onchoose(st.id);
		cards[next]?.focus();
	}
</script>

<div class="st-cursor-styles" role="radiogroup" aria-label="Cursor highlight style">
	{#each STYLES as st, i (st.id)}
		{@const on = st.id === value}
		<button
			bind:this={cards[i]}
			type="button"
			role="radio"
			aria-checked={on}
			tabindex={on ? 0 : -1}
			class="plate-key st-card-pick"
			onclick={() => onchoose(st.id)}
			onkeydown={(e) => onkeydown(e, i)}
		>
			<svg class="st-cursor-view" viewBox="0 0 120 84" aria-hidden="true">
				<defs>
					<linearGradient id="st-cv-bg-{st.id}" x1="0" y1="0" x2="0" y2="1">
						<stop offset="0" stop-color="#c9d0d8" />
						<stop offset="1" stop-color="#f1f3f5" />
					</linearGradient>
					<radialGradient
						id="st-cv-spot-{st.id}"
						cx="60"
						cy="42"
						r="34"
						gradientUnits="userSpaceOnUse"
					>
						<stop offset="0.62" stop-color="#000" stop-opacity="0" />
						<stop offset="0.82" stop-color="#000" stop-opacity={Math.min(0.85, opacity * 0.62)} />
						<stop offset="1" stop-color="#000" stop-opacity={Math.min(0.85, opacity * 0.62)} />
					</radialGradient>
				</defs>
				<rect width="120" height="84" fill="url(#st-cv-bg-{st.id})" />
				<path d="M18 62 44 22h34l24 40z" fill="#aab3bd" opacity="0.55" />
				{#if st.id === 'ring'}
					<circle cx="60" cy="42" r="15" fill="none" stroke={color} stroke-width="3.5" {opacity} />
				{:else if st.id === 'spotlight'}
					<rect width="120" height="84" fill="url(#st-cv-spot-{st.id})" />
				{:else}
					<circle cx="60" cy="42" r="11" fill={color} opacity={opacity * 0.75} />
				{/if}
				<path
					d={POINTER}
					transform="translate(0 9)"
					fill="#fff"
					stroke="#111"
					stroke-width="1.4"
					stroke-linejoin="round"
				/>
			</svg>
			<span class="st-cs-words">
				<span class="st-cs-name">
					{st.name}
					{#if on}<span class="st-cs-on"><Icon name="check" size={16} />In use</span>{/if}
				</span>
				<span class="st-cs-desc">{st.description}</span>
			</span>
		</button>
	{/each}
</div>

<style>
	.st-cursor-styles {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: 0.85rem;
		padding: 0.9rem 0;
	}
	:global(.ks-plate) .st-cursor-styles .st-card-pick {
		display: flex;
		flex-direction: column;
		align-items: stretch;
		padding: 7px;
		min-height: 0;
		text-align: left;
		font-weight: 500;
		letter-spacing: 0;
	}
	.st-cursor-view {
		display: block;
		width: 100%;
		height: auto;
		border-radius: 6px;
		box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.45);
	}
	.st-cs-words {
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
		padding: 0.6rem 0.35rem 0.3rem;
	}
	.st-cs-name {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.5rem;
		font-weight: 700;
		font-size: 1.06rem;
		color: var(--plate-ink);
	}
	.st-cs-on {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		font-family: var(--font-mono);
		font-size: 0.7rem;
		letter-spacing: 0.1em;
		text-transform: uppercase;
		color: var(--plate-mark-ink);
		font-weight: 400;
	}
	.st-cs-desc {
		font-size: 0.9rem;
		line-height: 1.3;
		color: var(--plate-ink-2);
		white-space: normal;
	}
</style>
