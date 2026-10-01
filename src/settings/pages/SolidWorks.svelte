<!--
	SolidWorks: the profile's mode and display switches, every sourced
	shortcut and mouse caption with its version and source page, what the help
	does not document, and the import of the user's own shortcuts.
-->
<script lang="ts">
	import type { SolidworksMode } from '../../lib/protocol';
	import { useController } from '../state.svelte';
	import { profileCounts } from '../lib/shortcuts';
	import { acceleratorParts } from '../lib/accelerator';
	import PageTitle from '../ui/PageTitle.svelte';
	import Card from '../ui/Card.svelte';
	import Row from '../ui/Row.svelte';
	import Switch from '../ui/Switch.svelte';
	import Segmented from '../ui/Segmented.svelte';
	import Notice from '../ui/Notice.svelte';
	import Key from '../ui/Key.svelte';
	import KeyCombo from '../ui/KeyCombo.svelte';
	import Icon from '../ui/Icon.svelte';
	import ShortcutTable from '../parts/ShortcutTable.svelte';
	import ImportPanel from '../parts/ImportPanel.svelte';

	const ctl = useController();
	const s = $derived(ctl.settings!);
	const d = $derived(s.profiles.solidworks_display);
	const counts = $derived(ctl.profile ? profileCounts(ctl.profile) : null);

	async function setMode(mode: SolidworksMode) {
		s.profiles.solidworks_mode = mode;
		await ctl.statusCommand(() => ctl.ipc.setSolidworksMode(mode));
	}

	function open(url: string) {
		void ctl.run(() => ctl.ipc.openUrl(url));
	}

	const MODE_HELP: Record<SolidworksMode, string> = {
		auto: 'On while SolidWorks is the app in front, off everywhere else.',
		on: 'On in every app, for example while you explain a dialog in another window.',
		off: 'Never on. SolidWorks gets the general rules: shortcuts only, no captions.',
	};
</script>

<PageTitle
	title="SolidWorks"
	lead="Captions for SolidWorks shortcuts and mouse moves, taken only from the published help for 2025 and 2026."
/>

<div class="st-stack">
	<Card title="Profile">
		<Row label="SolidWorks mode" help={MODE_HELP[s.profiles.solidworks_mode]}>
			<Segmented
				value={s.profiles.solidworks_mode}
				options={[
					{ value: 'auto', label: 'Auto' },
					{ value: 'on', label: 'On' },
					{ value: 'off', label: 'Off' },
				]}
				label="SolidWorks mode"
				onchange={setMode}
			/>
		</Row>
		<Row label="Hotkey" help="Steps through Auto, On and Off.">
			<KeyCombo parts={acceleratorParts(s.hotkeys.toggle_solidworks)} size="s" />
		</Row>
		<Row
			label="Typed values"
			help="Numbers you type into a dimension or a property box appear as one readout, such as 25.4 Enter. Off: they appear as Typing, but single-key shortcuts still show."
		>
			<Switch
				checked={d.typed_values ?? true}
				label="Typed values"
				onchange={(v) => (d.typed_values = v)}
			/>
		</Row>
		<Row
			label="Mouse captions"
			help="Callouts for middle drags, the wheel and other mouse moves, with the captions the help gives them."
		>
			<Switch
				checked={d.mouse_captions ?? true}
				label="Mouse captions"
				onchange={(v) => (d.mouse_captions = v)}
			/>
		</Row>
	</Card>

	<Card
		title="Shortcuts and mouse captions"
		lead={counts
			? `${counts.keyboard} keyboard and ${counts.mouse} mouse entries from ${counts.sources} help pages (${counts.versions.join(' and ')}), read ${counts.read ?? 'from the help'}.`
			: undefined}
	>
		{#if ctl.profile}
			<ShortcutTable profile={ctl.profile} {open} />
			{#if ctl.profile.gaps.length > 0}
				<details class="st-gaps">
					<summary>
						<span class="st-chev"><Icon name="arrowRight" size={16} /></span>
						What the help does not document ({ctl.profile.gaps.length})
					</summary>
					<p class="st-gaps-lead">
						No caption is shown for these, because no 2025 or 2026 help page documents them. A Copy
						List import brings in your own keys, including these.
					</p>
					<ul>
						{#each ctl.profile.gaps as g, i (i)}
							<li>
								<span class="st-gap-topic">{g.topic}</span>
								{#if g.url}
									<Key
										icon="external"
										label="Open the page that was checked: {g.topic}"
										onclick={() => open(g.url)}
									/>
								{/if}
							</li>
						{/each}
					</ul>
				</details>
			{/if}
		{:else if ctl.profileError}
			<div class="st-pad">
				<Notice tone="danger" lead="Could not load the SolidWorks data" announce>
					{ctl.profileError}
					{#snippet actions()}
						<Key icon="refresh" onclick={() => void ctl.loadProfile(true)}>Try again</Key>
					{/snippet}
				</Notice>
			</div>
		{:else}
			<p class="st-loading" role="status">Loading the SolidWorks data</p>
		{/if}
	</Card>

	<Card
		title="Your own shortcuts"
		lead="Bring in the keys from your own SolidWorks, including ones you changed and ones the help does not list."
	>
		<ImportPanel />
	</Card>
</div>

<style>
	.st-stack {
		display: flex;
		flex-direction: column;
		gap: 1.6rem;
	}
	.st-pad {
		padding: 1rem 0;
	}
	.st-loading {
		margin: 0;
		padding: 1.5rem 0;
		text-align: center;
		font-family: var(--font-mono);
		font-size: 0.8rem;
		letter-spacing: 0.12em;
		text-transform: uppercase;
		color: var(--plate-caption);
	}
	.st-gaps {
		padding: 0 0 1rem;
	}
	.st-gaps summary {
		cursor: pointer;
		min-height: 44px;
		display: flex;
		align-items: center;
		gap: 0.4rem;
		font-weight: 600;
		list-style: none;
	}
	.st-gaps summary::-webkit-details-marker {
		display: none;
	}
	.st-chev {
		display: inline-flex;
		color: var(--plate-accent);
	}
	.st-gaps[open] .st-chev {
		transform: rotate(90deg);
	}
	.st-gaps-lead {
		margin: 0 0 0.5rem;
		color: var(--plate-ink-2);
	}
	.st-gaps ul {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
	}
	.st-gaps li {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		padding: 0.3rem 0;
		box-shadow:
			inset 0 -1px 0 0 var(--plate-groove-dk),
			inset 0 -2px 0 0 var(--plate-groove-lt);
	}
	.st-gap-topic {
		color: var(--plate-ink-2);
		line-height: 1.35;
	}
</style>
