<!--
	Hotkeys: the five global shortcuts, each with a capture field, a reset,
	and any conflict the window or Windows found.
-->
<script lang="ts">
	import { useController } from '../state.svelte';
	import { HOTKEY_NAMES, type HotkeyName } from '../types';
	import { DEFAULT_HOTKEYS } from '../defaults';
	import { HOTKEY_INFO, hotkeyProblems } from '../lib/hotkeys';
	import { acceleratorParts } from '../lib/accelerator';
	import PageTitle from '../ui/PageTitle.svelte';
	import Card from '../ui/Card.svelte';
	import Key from '../ui/Key.svelte';
	import Notice from '../ui/Notice.svelte';
	import Icon from '../ui/Icon.svelte';
	import HotkeyField from '../parts/HotkeyField.svelte';

	const ctl = useController();
	const s = $derived(ctl.settings!);

	function setCapture(active: boolean) {
		ctl.ipc.hotkeysCapture(active).catch(() => {
			// The shortcut can still be typed; only a shortcut equal to a current hotkey would fire it.
		});
	}

	function set(name: HotkeyName, value: string) {
		s.hotkeys[name] = value;
	}

	const allDefault = $derived(HOTKEY_NAMES.every((n) => s.hotkeys[n] === DEFAULT_HOTKEYS[n]));
</script>

<PageTitle
	title="Hotkeys"
	lead="Shortcuts that work from any app. They are never shown as callouts and never reach the app in front."
/>

<div class="st-stack">
	<Card title="Global hotkeys">
		{#snippet aside()}
			<Key
				icon="refresh"
				disabled={allDefault}
				onclick={() => {
					for (const n of HOTKEY_NAMES) s.hotkeys[n] = DEFAULT_HOTKEYS[n];
				}}
			>
				Restore all
			</Key>
		{/snippet}
		<ul class="st-hk-list">
			{#each HOTKEY_NAMES as name (name)}
				{@const info = HOTKEY_INFO[name]}
				{@const problems = hotkeyProblems(name, {
					hotkeys: s.hotkeys,
					profile: ctl.profile,
					states: ctl.hotkeyStates,
				})}
				<li class="st-hk-row">
					<div class="st-hk-words">
						<span class="st-hk-title">{info.title}</span>
						<span class="st-hk-help" id="st-hk-{name}">{info.help}</span>
						{#each problems as p, i (i)}
							<span class="st-hk-problem is-{p.level}">
								<Icon name={p.level === 'error' ? 'error' : 'warning'} size={16} />
								<span
									><strong>{p.level === 'error' ? 'Problem:' : 'Note:'}</strong> {p.message}</span
								>
							</span>
						{/each}
					</div>
					<div class="st-hk-controls">
						<HotkeyField
							{name}
							title={info.title}
							hotkeys={s.hotkeys}
							onset={(v) => set(name, v)}
							{setCapture}
							describedby="st-hk-{name}"
						/>
						{#if s.hotkeys[name] !== DEFAULT_HOTKEYS[name]}
							<Key
								icon="refresh"
								label="Reset {info.title} to {acceleratorParts(DEFAULT_HOTKEYS[name]).join(' + ')}"
								onclick={() => set(name, DEFAULT_HOTKEYS[name])}
							/>
						{/if}
					</div>
				</li>
			{/each}
		</ul>
	</Card>

	<Notice tone="info" lead="Good hotkeys">
		Use Ctrl, Alt and Shift together, as the defaults do, so a hotkey is unlikely to clash with
		SolidWorks or another program. A function key on its own works too. Windows keeps a few
		shortcuts for itself; if it refuses one, this page says so.
	</Notice>
</div>

<style>
	.st-stack {
		display: flex;
		flex-direction: column;
		gap: 1.6rem;
	}
	.st-hk-list {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	.st-hk-row {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto;
		align-items: center;
		gap: 0.6rem 1.5rem;
		padding: 1rem 0;
	}
	.st-hk-row + .st-hk-row {
		box-shadow:
			inset 0 1px 0 0 var(--plate-groove-dk),
			inset 0 2px 0 0 var(--plate-groove-lt);
	}
	.st-hk-words {
		display: flex;
		flex-direction: column;
		gap: 0.2rem;
	}
	.st-hk-title {
		font-weight: 600;
		font-size: 1.08rem;
	}
	.st-hk-help {
		color: var(--plate-ink-2);
		font-size: 0.95rem;
	}
	.st-hk-problem {
		display: flex;
		align-items: flex-start;
		gap: 0.35rem;
		margin-top: 0.25rem;
		font-size: 0.93rem;
		line-height: 1.35;
	}
	.st-hk-problem.is-error {
		color: var(--plate-danger-ink);
	}
	.st-hk-problem.is-warning {
		color: var(--plate-warn-ink);
	}
	.st-hk-controls {
		display: flex;
		align-items: flex-start;
		gap: 0.5rem;
	}
</style>
