<!--
	The SolidWorks shortcuts and mouse captions the overlay knows, from the
	profile data (and the user's own import), searchable. Each row names the
	help version it was documented in and links to its source page.
-->
<script lang="ts">
	import type { ProfileData } from '../types';
	import { buildRows, filterRows, type KindFilter, type VersionFilter } from '../lib/shortcuts';
	import TextField from '../ui/TextField.svelte';
	import Segmented from '../ui/Segmented.svelte';
	import KeyCombo from '../ui/KeyCombo.svelte';
	import Key from '../ui/Key.svelte';
	import Icon from '../ui/Icon.svelte';

	interface Props {
		profile: ProfileData;
		open: (url: string) => void;
	}

	let { profile, open }: Props = $props();

	const PAGE = 60;
	let query = $state('');
	let kind = $state<KindFilter>('all');
	let version = $state<VersionFilter>('all');
	let shown = $state(PAGE);

	const rows = $derived(buildRows(profile));
	const imported = $derived(rows.filter((r) => r.imported).length);
	const filtered = $derived(filterRows(rows, { query, kind, version }));
	const visible = $derived(filtered.slice(0, shown));

	$effect(() => {
		void query;
		void kind;
		void version;
		shown = PAGE;
	});
</script>

<div class="st-sw-table">
	<div class="st-sw-filters">
		<TextField
			bind:value={query}
			type="search"
			icon="search"
			label="Search shortcuts and mouse captions"
			placeholder="Search keys, commands or context"
		/>
		<div class="st-sw-segs">
			<Segmented
				bind:value={kind}
				options={[
					{ value: 'all', label: 'All' },
					{ value: 'keyboard', label: 'Keyboard' },
					{ value: 'mouse', label: 'Mouse' },
					...(imported > 0 ? [{ value: 'imported' as const, label: 'Your import' }] : []),
				]}
				label="Kind of entry"
			/>
			<Segmented
				bind:value={version}
				options={[
					{ value: 'all', label: 'Any version' },
					{ value: '2025', label: '2025' },
					{ value: '2026', label: '2026' },
				]}
				label="Help version"
			/>
		</div>
	</div>

	<p class="st-sw-count" role="status">
		{filtered.length.toLocaleString('en-US')} of {rows.length.toLocaleString('en-US')} entries
	</p>

	{#if filtered.length === 0}
		<div class="st-listwell st-sw-empty">
			<p>Nothing matches. Try fewer words, or a key name such as Ctrl or Space.</p>
		</div>
	{:else}
		<div class="st-listwell st-sw-well">
			<table>
				<thead>
					<tr>
						<th scope="col">Keys or mouse</th>
						<th scope="col">Caption</th>
						<th scope="col">Version and source</th>
					</tr>
				</thead>
				<tbody>
					{#each visible as r (r.id)}
						<tr>
							<td class="st-col-keys">
								{#if r.sequence}
									<kbd class="st-doc-keys">{r.docKeys}</kbd>
								{:else}
									<KeyCombo parts={r.parts} size="s" />
								{/if}
							</td>
							<td>
								<div class="st-col-cmd">
									<span class="st-cmd">{r.command}</span>
									{#if r.context}<span class="st-ctx">{r.context}</span>{/if}
									<span class="st-tags">
										{#if r.imported}<span class="plate-chip">Your import</span>{/if}
										{#if r.screenshot}<span
												class="plate-chip"
												title="Read from a screenshot on the help page">Screenshot</span
											>{/if}
										{#if r.group && !r.imported}<span class="st-group">{r.group}</span>{/if}
									</span>
								</div>
							</td>
							<td class="st-col-ver">
								{#if r.imported}
									<span class="st-group">Your SolidWorks</span>
								{:else if r.sources.length > 0}
									<span class="st-links">
										{#each r.sources.slice(0, 3) as src (src.id)}
											<Key
												icon="external"
												label="{src.version} help page: {src.title}"
												title="Open {src.title} ({src.version}) in your browser"
												onclick={() => open(src.url)}
											>
												{src.version}
											</Key>
										{/each}
									</span>
								{:else}
									{#each r.versions as v (v)}<span class="plate-chip st-ver">{v}</span>{/each}
								{/if}
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
		{#if filtered.length > shown}
			<div class="st-sw-more">
				<Key icon="plus" onclick={() => (shown += PAGE)}>
					Show {Math.min(PAGE, filtered.length - shown)} more
				</Key>
			</div>
		{/if}
	{/if}
	<p class="st-sw-note">
		<Icon name="info" size={16} />
		A link opens the help page in your web browser. IDEA Keystage itself never connects to the internet.
	</p>
</div>

<style>
	.st-sw-table {
		display: flex;
		flex-direction: column;
		gap: 0.7rem;
		padding: 1rem 0;
	}
	.st-sw-filters {
		display: flex;
		flex-direction: column;
		gap: 0.6rem;
	}
	.st-sw-filters :global(.st-field-wrap) {
		width: 100%;
	}
	.st-sw-segs {
		display: flex;
		flex-wrap: wrap;
		gap: 0.6rem;
	}
	.st-sw-count {
		margin: 0;
		font-family: var(--font-mono);
		font-size: 0.72rem;
		letter-spacing: 0.1em;
		text-transform: uppercase;
		color: var(--plate-caption);
	}
	.st-sw-well {
		max-height: 620px;
		overflow-y: auto;
	}
	.st-sw-empty p {
		margin: 0;
		padding: 1.4rem;
		text-align: center;
		color: var(--plate-ink-2);
	}
	table {
		width: 100%;
		border-collapse: collapse;
	}
	thead th {
		position: sticky;
		top: 0;
		z-index: 1;
		padding: 0.6rem 0.75rem;
		text-align: left;
		font-family: var(--font-mono);
		font-size: var(--plate-label-size);
		letter-spacing: var(--plate-label-track);
		text-transform: uppercase;
		font-weight: 400;
		color: var(--plate-caption);
		background: var(--plate-list-face);
		box-shadow:
			inset 0 -1px 0 0 var(--plate-groove-dk),
			0 1px 0 0 var(--plate-groove-lt);
	}
	tbody tr + tr td {
		box-shadow:
			inset 0 1px 0 0 var(--plate-groove-dk),
			inset 0 2px 0 0 var(--plate-groove-lt);
	}
	td {
		padding: 0.55rem 0.75rem;
		vertical-align: middle;
	}
	.st-col-keys {
		width: 30%;
	}
	.st-col-cmd {
		display: flex;
		flex-direction: column;
		gap: 0.1rem;
	}
	.st-cmd {
		font-weight: 600;
		color: var(--plate-ink);
		line-height: 1.3;
	}
	.st-ctx {
		font-size: 0.9rem;
		color: var(--plate-ink-2);
		line-height: 1.3;
	}
	.st-tags {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.35rem;
		margin-top: 0.15rem;
	}
	.st-tags:empty {
		display: none;
	}
	.st-group {
		font-family: var(--font-mono);
		font-size: 0.7rem;
		letter-spacing: 0.06em;
		color: var(--plate-caption);
	}
	.st-col-ver {
		white-space: nowrap;
		width: 1%;
	}
	.st-ver + .st-ver {
		margin-left: 0.3rem;
	}
	.st-links {
		display: inline-flex;
		gap: 0.35rem;
	}
	.st-links :global(.st-key) {
		padding: 0 0.6rem;
		gap: 0.35rem;
		font-family: var(--font-mono);
		font-size: 0.8rem;
	}
	.st-doc-keys {
		display: inline-flex;
		align-items: center;
		min-height: 22px;
		padding: 0 7px;
		border-radius: var(--plate-r-small);
		font-family: var(--font-mono);
		font-size: 0.72rem;
		white-space: nowrap;
		color: var(--plate-ink);
		background-color: var(--plate-chip-ground);
		background-image: linear-gradient(to bottom, var(--plate-chip-top), var(--plate-chip-bot));
		box-shadow:
			inset 0 0 0 1px var(--plate-chip-edge),
			inset 0 2px 2px -1px var(--plate-chip-shade),
			inset 0 -2px 0 0 var(--plate-chip-lip);
	}
	.st-sw-more {
		display: flex;
		justify-content: center;
	}
	.st-sw-note {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		margin: 0;
		font-size: 0.9rem;
		color: var(--plate-ink-2);
	}
</style>
