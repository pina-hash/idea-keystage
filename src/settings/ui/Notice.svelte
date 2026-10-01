<!--
	A message set into the surface: an icon and a lead word say what kind it
	is (Note, Warning, Problem), so the tone color is never the only signal.
	Problems are announced (role="alert"); the rest are status.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';
	import Icon from './Icon.svelte';
	import type { IconName } from './icons';

	interface Props {
		tone?: 'info' | 'warn' | 'danger' | 'ok';
		/** The lead word; defaults to the tone's word. */
		lead?: string;
		children: Snippet;
		actions?: Snippet;
		/** Announce it when it appears. */
		announce?: boolean;
	}

	let { tone = 'info', lead, children, actions, announce = false }: Props = $props();

	const WORDS = { info: 'Note', warn: 'Warning', danger: 'Problem', ok: 'Done' } as const;
	const ICON: Record<string, IconName> = {
		info: 'info',
		warn: 'warning',
		danger: 'error',
		ok: 'check',
	};
</script>

<div
	class="st-notice is-{tone}"
	role={announce ? (tone === 'danger' ? 'alert' : 'status') : undefined}
>
	<span class="st-notice-icon"><Icon name={ICON[tone] ?? 'info'} size={20} /></span>
	<div class="st-notice-body">
		<p class="st-notice-text">
			<strong class="st-notice-lead">{lead ?? WORDS[tone]}:</strong>
			{@render children()}
		</p>
		{#if actions}<div class="st-notice-actions">{@render actions()}</div>{/if}
	</div>
</div>

<style>
	.st-notice {
		display: flex;
		gap: 0.7rem;
		padding: 0.75rem 0.9rem;
		border-radius: var(--plate-r-control);
		background-color: var(--plate-chip-ground);
		background-image: linear-gradient(to bottom, var(--plate-chip-top), var(--plate-chip-bot));
		box-shadow:
			inset 0 0 0 1px var(--plate-chip-edge),
			inset 0 2px 3px -1px var(--plate-chip-shade),
			inset 0 -1px 0 0 var(--plate-chip-lip),
			0 1px 0 0 var(--plate-chip-lip);
		color: var(--plate-ink);
	}
	.st-notice::before {
		content: '';
		flex: none;
		width: 3px;
		margin: -0.2rem 0;
		border-radius: 2px;
		background: var(--st-tone);
	}
	.is-info {
		--st-tone: var(--plate-info-ink);
	}
	.is-warn {
		--st-tone: var(--plate-warn-ink);
	}
	.is-danger {
		--st-tone: var(--plate-danger-ink);
	}
	.is-ok {
		--st-tone: var(--plate-ok-ink);
	}
	.st-notice-icon {
		flex: none;
		color: var(--st-tone);
		padding-top: 2px;
	}
	.st-notice-body {
		display: flex;
		flex-direction: column;
		gap: 0.6rem;
		min-width: 0;
	}
	.st-notice-text {
		margin: 0;
		font-size: 0.98rem;
		line-height: 1.45;
	}
	.st-notice-lead {
		color: var(--st-tone);
		font-weight: 700;
	}
	.st-notice-actions {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
	}
</style>
