<!--
	The navigation rail: the app's mark and live status at the top, one pad
	per page (the current one lit, with aria-current), and the save state and
	the window's appearance at the foot.
-->
<script lang="ts">
	import { useController } from '../state.svelte';
	import { PAGES, PAGE_INFO, activityWord, recordingText } from '../lib/nav';
	import Icon from '../ui/Icon.svelte';
	import StatusChip from '../ui/StatusChip.svelte';
	import Key from '../ui/Key.svelte';
	import Mark from './Mark.svelte';

	const ctl = useController();
	const appearance = $derived(ctl.appearance);
	const activity = $derived(ctl.status ? activityWord(ctl.status) : null);
	const rec = $derived(ctl.status ? recordingText(ctl.status) : null);
	const notices = $derived(ctl.status?.notices.length ?? 0);

	function toggleAppearance() {
		ctl.setAppearance(appearance === 'dark' ? 'light' : 'dark');
	}

	const saveText = $derived.by(() => {
		switch (ctl.saveState) {
			case 'pending':
			case 'saving':
				return 'Saving';
			case 'error':
				return 'Not saved';
			case 'saved':
				return 'All changes saved';
			default:
				return 'Changes save as you go';
		}
	});
</script>

<aside class="st-rail">
	<div class="st-rail-top">
		<Mark />
		<div class="st-rail-status">
			{#if activity}
				<StatusChip tone={activity.tone} text={activity.word} live title={activity.detail} />
			{/if}
			{#if rec}
				<StatusChip
					tone="danger"
					text={rec.startsWith('Recording paused') ? 'Rec paused' : 'Rec'}
					title={rec}
				/>
			{/if}
			{#if notices > 0}
				<StatusChip
					tone="warn"
					text={notices === 1 ? '1 warning' : `${notices} warnings`}
					title="Read on the Home page."
				/>
			{/if}
		</div>
	</div>

	<nav class="st-rail-nav" aria-label="Settings pages">
		<ul>
			{#each PAGES as id (id)}
				{@const info = PAGE_INFO[id]}
				<li>
					<button
						type="button"
						class="st-pad"
						aria-current={ctl.page === id ? 'page' : undefined}
						onclick={() => ctl.go(id)}
					>
						<Icon name={info.icon} size={19} />
						<span>{info.title}</span>
					</button>
				</li>
			{/each}
		</ul>
	</nav>

	<div class="st-rail-foot">
		<div class="st-save" class:is-error={ctl.saveState === 'error'} role="status">
			<Icon
				name={ctl.saveState === 'error'
					? 'warning'
					: ctl.saveState === 'saved'
						? 'check'
						: 'refresh'}
				size={16}
			/>
			<span>{saveText}</span>
		</div>
		{#if ctl.saveState === 'error'}
			<p class="st-save-error">{ctl.saveError}</p>
			<Key icon="refresh" onclick={() => void ctl.retrySave()}>Try again</Key>
		{/if}
		<div class="st-rail-row">
			<Key
				icon={appearance === 'dark' ? 'sun' : 'moon'}
				onclick={toggleAppearance}
				title="Switch this window to {appearance === 'dark' ? 'light' : 'dark'} plates"
			>
				{appearance === 'dark' ? 'Light' : 'Dark'}
			</Key>
			{#if ctl.status?.version}
				<span class="st-version">v{ctl.status.version}</span>
			{/if}
		</div>
	</div>
</aside>

<style>
	.st-rail {
		position: relative;
		display: flex;
		flex-direction: column;
		gap: 1rem;
		min-height: 0;
		padding: 1.1rem 0.9rem 0.9rem;
		background: linear-gradient(to bottom, var(--plate-plate-top), var(--plate-plate-bot));
		box-shadow:
			inset -1px 0 0 0 var(--plate-groove-dk),
			inset -2px 0 0 0 var(--plate-groove-lt),
			inset 1px 0 0 0 var(--plate-plate-hi);
	}
	.st-rail-top {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		padding: 0 0.2rem;
	}
	.st-rail-status {
		display: flex;
		flex-wrap: wrap;
		gap: 0.4rem;
	}
	.st-rail-nav {
		flex: 1 1 auto;
		min-height: 0;
		overflow-y: auto;
		margin: 0 -0.5rem;
		padding: 0.35rem 0.5rem;
	}
	ul {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 9px;
	}
	.st-rail-foot {
		display: flex;
		flex-direction: column;
		gap: 0.6rem;
		padding: 0.75rem 0.2rem 0;
		box-shadow:
			inset 0 1px 0 0 var(--plate-groove-dk),
			inset 0 2px 0 0 var(--plate-groove-lt);
	}
	.st-save {
		display: flex;
		align-items: center;
		gap: 0.45rem;
		font-family: var(--font-mono);
		font-size: 0.72rem;
		letter-spacing: 0.08em;
		text-transform: uppercase;
		color: var(--plate-caption);
	}
	.st-save.is-error {
		color: var(--plate-danger-ink);
	}
	.st-save-error {
		margin: 0;
		font-size: 0.9rem;
		color: var(--plate-ink-2);
	}
	.st-rail-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.5rem;
	}
	.st-version {
		font-family: var(--font-mono);
		font-size: 0.72rem;
		letter-spacing: 0.08em;
		color: var(--plate-caption);
	}
</style>
