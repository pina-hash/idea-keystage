<!--
	Short messages after an action (saved, copied, a command that failed),
	stacked at the bottom of the page column. Each has an icon and words, and
	can be closed; problems stay twice as long.
-->
<script lang="ts">
	import { useController } from '../state.svelte';
	import Icon from '../ui/Icon.svelte';

	const ctl = useController();
	const ICONS = { ok: 'check', info: 'info', danger: 'error' } as const;
</script>

<div class="st-toasts" aria-live="polite">
	{#each ctl.toasts as t (t.id)}
		<div class="plate-housing st-toast is-{t.tone}" role={t.tone === 'danger' ? 'alert' : 'status'}>
			<span class="st-toast-icon"><Icon name={ICONS[t.tone]} size={20} /></span>
			<span class="st-toast-text">{t.text}</span>
			<button
				type="button"
				class="plate-key st-toast-close"
				aria-label="Close message"
				onclick={() => ctl.dismiss(t.id)}
			>
				<Icon name="close" size={16} />
			</button>
		</div>
	{/each}
</div>

<style>
	.st-toasts {
		position: fixed;
		left: calc(var(--st-rail-w) + 1.5rem);
		bottom: 1.25rem;
		z-index: 50;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		max-width: min(560px, calc(100vw - var(--st-rail-w) - 3rem));
		pointer-events: none;
	}
	.st-toast {
		display: flex;
		align-items: center;
		gap: 0.7rem;
		padding: 0.55rem 0.55rem 0.55rem 1rem;
		pointer-events: auto;
	}
	.st-toast-icon {
		flex: none;
	}
	.is-ok .st-toast-icon {
		color: var(--plate-ok-ink);
	}
	.is-info .st-toast-icon {
		color: var(--plate-info-ink);
	}
	.is-danger .st-toast-icon {
		color: var(--plate-danger-ink);
	}
	.st-toast-text {
		flex: 1 1 auto;
		font-size: 0.98rem;
		line-height: 1.35;
	}
	:global(.ks-plate) .st-toast-close {
		flex: none;
		width: 44px;
		padding: 0;
	}
</style>
