<!--
	The keyboard layout picker: one raised card per layout, each drawing the
	real keyboard visual (KeyboardVisual) at a small size in the current
	theme. The chosen card is lit (accent ring, "In use", aria-checked).
-->
<script lang="ts">
	import '../../lib/themes/index.css';
	import { KEYBOARD_LAYOUTS, type KeyboardLayout, type OverlayConfig } from '../../lib/protocol';
	import { resolveTheme } from '../../lib/themes/themes';
	import { KEYBOARD_FOOTPRINTS } from '../../lib/renderer/metrics';
	import { emptyButtons } from '../../lib/renderer/logic/inputs';
	import KeyboardVisual from '../../lib/renderer/keyboard/KeyboardVisual.svelte';
	import Icon from '../ui/Icon.svelte';
	import { LAYOUT_INFO } from '../lib/layouts';

	interface Props {
		config: OverlayConfig;
		value: KeyboardLayout;
		onchoose: (layout: KeyboardLayout) => void;
		/** Card width the drawings are fitted into. */
		drawWidth?: number;
		label?: string;
	}

	let { config, value, onchoose, drawWidth = 220, label = 'Keyboard layout' }: Props = $props();

	const theme = $derived(resolveTheme(config));
	const buttons = emptyButtons();
	let cards: HTMLButtonElement[] = $state([]);

	/** One key unit in px so the widest layout fills the card. */
	function unitFor(layout: KeyboardLayout): number {
		const widest = KEYBOARD_FOOTPRINTS.full.w;
		const fp = KEYBOARD_FOOTPRINTS[layout];
		// Every drawing uses the same key size, so the layouts compare at a glance.
		return Math.min(drawWidth / widest, 74 / fp.h);
	}

	function onkeydown(e: KeyboardEvent, index: number) {
		const n = KEYBOARD_LAYOUTS.length;
		let next = -1;
		if (e.key === 'ArrowRight' || e.key === 'ArrowDown') next = (index + 1) % n;
		else if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') next = (index - 1 + n) % n;
		if (next < 0) return;
		e.preventDefault();
		const l = KEYBOARD_LAYOUTS[next];
		if (!l) return;
		onchoose(l);
		cards[next]?.focus();
	}
</script>

<div class="st-layouts" role="radiogroup" aria-label={label}>
	{#each KEYBOARD_LAYOUTS as id, i (id)}
		{@const on = value === id}
		{@const info = LAYOUT_INFO[id]}
		<button
			bind:this={cards[i]}
			type="button"
			role="radio"
			aria-checked={on}
			tabindex={on ? 0 : -1}
			class="plate-key st-card-pick"
			onclick={() => onchoose(id)}
			onkeydown={(e) => onkeydown(e, i)}
		>
			<span class="st-layout-view" aria-hidden="true">
				<span class="ks-stage st-layout-stage" data-theme={theme.id} style={theme.style}>
					<span class="st-layout-center">
						<KeyboardVisual
							layout={id}
							keys={{}}
							legends={null}
							size="s"
							unitPx={unitFor(id)}
							decayMs={config.keyboard.decay_ms}
							showMouse={false}
							{buttons}
							wheel={null}
							iso={false}
						/>
					</span>
				</span>
			</span>
			<span class="st-layout-words">
				<span class="st-layout-name">
					{info.name}
					{#if on}<span class="st-layout-on"><Icon name="check" size={16} />In use</span>{/if}
				</span>
				<span class="st-layout-desc">{info.description}</span>
			</span>
		</button>
	{/each}
</div>

<style>
	.st-layouts {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(250px, 1fr));
		gap: 0.85rem;
		padding: 0.9rem 0;
	}
	:global(.ks-plate) .st-layouts .st-card-pick {
		display: flex;
		flex-direction: column;
		align-items: stretch;
		justify-content: flex-start;
		padding: 7px;
		min-height: 0;
		text-align: left;
		font-weight: 500;
		letter-spacing: 0;
	}
	.st-layout-view {
		position: relative;
		display: block;
		height: 104px;
		border-radius: 6px;
		overflow: hidden;
		background: linear-gradient(to bottom, #c9d0d8 0%, #e4e8ec 60%, #f1f3f5 100%);
		box-shadow:
			0 0 0 1px rgba(0, 0, 0, 0.45),
			inset 0 0 0 1px rgba(255, 255, 255, 0.1);
	}
	.st-layout-stage {
		display: block;
	}
	.st-layout-center {
		position: absolute;
		inset: 0;
		display: flex;
		align-items: center;
		justify-content: center;
	}
	.st-layout-words {
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
		padding: 0.6rem 0.35rem 0.3rem;
	}
	.st-layout-name {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.5rem;
		font-weight: 700;
		font-size: 1.06rem;
		color: var(--plate-ink);
	}
	.st-layout-on {
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
	.st-layout-desc {
		font-size: 0.9rem;
		line-height: 1.3;
		color: var(--plate-ink-2);
		white-space: normal;
	}
</style>
