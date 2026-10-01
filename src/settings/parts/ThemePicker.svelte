<!--
	The overlay theme picker: one raised card per theme, each showing a real
	callout (the renderer's Callout component) drawn in that theme over a
	model-view backdrop. The chosen card is lit: accent ring, a tick with the
	word "In use", and aria-checked.
-->
<script lang="ts">
	import '../../lib/styles/fonts.css';
	import '../../lib/themes/index.css';
	import '../../lib/renderer/callouts.css';
	import type { Callout as CalloutData, OverlayConfig } from '../../lib/protocol';
	import { THEMES, resolveTheme, DEFAULT_CUSTOM } from '../../lib/themes/themes';
	import { FONT_STACKS } from '../../lib/renderer/pieces';
	import type { CalloutItem } from '../../lib/renderer/logic/callouts';
	import Callout from '../../lib/renderer/Callout.svelte';
	import Icon from '../ui/Icon.svelte';

	interface Props {
		config: OverlayConfig;
		/** Caption for the sample from the profile data, or null. */
		caption: string | null;
		onchoose: (theme: string) => void;
	}

	let { config, caption, onchoose }: Props = $props();

	let cards: HTMLButtonElement[] = $state([]);

	const sample = $derived<CalloutItem>({
		id: 1,
		callout: {
			id: 1,
			kind: 'combo',
			steps: [
				[
					{ key: 'Ctrl', label: 'Ctrl', kind: 'modifier' },
					{ key: 'S', label: 'S', kind: 'key' },
				],
			],
			text: null,
			count: 1,
			caption,
			context: null,
			profile: caption ? 'solidworks' : null,
			t_ms: 0,
			updated_ms: 0,
		} satisfies CalloutData,
		phase: 'live',
		bornAt: 0,
		updatedAt: 0,
		fadeAt: Number.MAX_SAFE_INTEGER,
		removeAt: Number.MAX_SAFE_INTEGER,
		rev: 0,
	});

	function styleFor(id: string): string {
		const resolved = resolveTheme({ theme: id, custom: config.custom ?? DEFAULT_CUSTOM });
		return [
			resolved.style,
			'--ks-u: 30px',
			'--ks-scale: 0.65',
			`--ks-font-key: ${FONT_STACKS[config.font]}`,
		]
			.filter(Boolean)
			.join('; ');
	}

	function onkeydown(e: KeyboardEvent, index: number) {
		const n = THEMES.length;
		let next = -1;
		if (e.key === 'ArrowRight' || e.key === 'ArrowDown') next = (index + 1) % n;
		else if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') next = (index - 1 + n) % n;
		if (next < 0) return;
		e.preventDefault();
		const t = THEMES[next];
		if (!t) return;
		onchoose(t.id);
		cards[next]?.focus();
	}
</script>

<div class="st-themes" role="radiogroup" aria-label="Overlay theme">
	{#each THEMES as t, i (t.id)}
		{@const on = config.theme === t.id}
		<button
			bind:this={cards[i]}
			type="button"
			role="radio"
			aria-checked={on}
			tabindex={on ? 0 : -1}
			class="plate-key st-card-pick"
			onclick={() => onchoose(t.id)}
			onkeydown={(e) => onkeydown(e, i)}
		>
			<span class="st-theme-view" aria-hidden="true">
				<span class="ks-stage st-theme-stage" data-theme={t.id} style={styleFor(t.id)}>
					<span class="st-theme-center">
						<Callout item={sample} symbols={config.callouts.symbols} captions={!!caption} />
					</span>
				</span>
			</span>
			<span class="st-theme-words">
				<span class="st-theme-name">
					{t.name}
					{#if on}<span class="st-theme-on"><Icon name="check" size={16} />In use</span>{/if}
				</span>
				<span class="st-theme-desc">{t.description}</span>
			</span>
		</button>
	{/each}
</div>

<style>
	.st-themes {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(232px, 1fr));
		gap: 0.85rem;
		padding: 0.9rem 0;
	}
	:global(.ks-plate) .st-card-pick {
		display: flex;
		flex-direction: column;
		align-items: stretch;
		justify-content: flex-start;
		gap: 0;
		padding: 7px;
		min-height: 0;
		text-align: left;
		font-weight: 500;
		letter-spacing: 0;
	}
	.st-theme-view {
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
	.st-theme-stage {
		display: block;
	}
	.st-theme-center {
		position: absolute;
		inset: 0;
		display: flex;
		align-items: center;
		justify-content: center;
	}
	.st-theme-words {
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
		padding: 0.6rem 0.35rem 0.3rem;
	}
	.st-theme-name {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.5rem;
		font-weight: 700;
		font-size: 1.06rem;
		color: var(--plate-ink);
	}
	.st-theme-on {
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
	.st-theme-desc {
		font-size: 0.9rem;
		line-height: 1.3;
		color: var(--plate-ink-2);
		white-space: normal;
	}
</style>
