<!--
	Bring in the shortcuts from the user's own SolidWorks: paste the Keyboard
	tab's Copy List or load a .txt (primary) or a .sldreg file
	(experimental), preview what would change, then apply. Both formats are
	unvalidated until a real export has been checked (DECISIONS item 4), and
	the panel says so every time.
-->
<script lang="ts">
	import { useController } from '../state.svelte';
	import type { ImportFormat, ImportPreview, ImportSource } from '../types';
	import { displayKeyParts, shadowedBy } from '../lib/shortcuts';
	import { plural } from '../lib/format';
	import Key from '../ui/Key.svelte';
	import ConfirmKey from '../ui/ConfirmKey.svelte';
	import Notice from '../ui/Notice.svelte';
	import KeyCombo from '../ui/KeyCombo.svelte';
	import Icon from '../ui/Icon.svelte';

	const ctl = useController();
	const uid = $props.id();

	let text = $state('');
	let file = $state<string | null>(null);
	let preview = $state<ImportPreview | null>(null);
	let previewedFrom = $state<ImportSource | null>(null);
	let busy = $state<'preview' | 'apply' | 'clear' | 'pick' | null>(null);

	const imported = $derived(ctl.settings?.profiles.solidworks_user_shortcuts.length ?? 0);
	const source = $derived<ImportSource | null>(
		file ? { kind: 'file', path: file } : text.trim().length > 0 ? { kind: 'text', text } : null
	);
	const stale = $derived(
		preview !== null && JSON.stringify(previewedFrom) !== JSON.stringify(source)
	);
	const fileName = $derived(file ? (file.split(/[\\/]/).pop() ?? file) : null);

	const FORMAT_WORDS: Record<ImportFormat, string> = {
		copy_list: 'Copy List',
		sldreg: '.sldreg settings file',
		unknown: 'Not recognized',
	};
	const SHOW = 40;

	async function pick() {
		busy = 'pick';
		const path = await ctl.run(() =>
			ctl.ipc.pickOpenFile('Load SolidWorks shortcuts', [
				{ name: 'Copy List or settings file', extensions: ['txt', 'sldreg'] },
				{ name: 'Copy List text', extensions: ['txt'] },
				{ name: 'SolidWorks settings', extensions: ['sldreg'] },
			])
		);
		busy = null;
		if (path) {
			file = path;
			await runPreview();
		}
	}

	async function runPreview() {
		const src = source;
		if (!src) return;
		busy = 'preview';
		const r = await ctl.run(() => ctl.ipc.shortcutsImportPreview(src));
		busy = null;
		if (r) {
			preview = r;
			previewedFrom = src;
		}
	}

	async function apply() {
		const src = previewedFrom;
		if (!src || !preview) return;
		busy = 'apply';
		await ctl.flush();
		const s = await ctl.run(() => ctl.ipc.shortcutsImportApply(src));
		busy = null;
		if (s) {
			ctl.adopt(s);
			ctl.toast(
				`${plural(s.profiles.solidworks_user_shortcuts.length, 'shortcut')} from your SolidWorks are in use.`,
				'ok'
			);
			preview = null;
			previewedFrom = null;
			text = '';
			file = null;
			await ctl.loadProfile(true);
		}
	}

	async function clearImported() {
		busy = 'clear';
		await ctl.flush();
		const s = await ctl.run(() => ctl.ipc.shortcutsImportClear());
		busy = null;
		if (s) {
			ctl.adopt(s);
			ctl.toast('Imported shortcuts removed. Only the sourced captions are used.', 'ok');
			await ctl.loadProfile(true);
		}
	}

	function reset() {
		text = '';
		file = null;
		preview = null;
		previewedFrom = null;
	}
</script>

<div class="st-import">
	<Notice tone="warn" lead="Unvalidated format">
		These importers follow the help's description of the Keyboard tab and have not yet been checked
		against a real export from SolidWorks 2025 or 2026. Look over the preview before you apply it.
	</Notice>

	<ol class="st-import-steps">
		<li>In SolidWorks, open Tools, Customize, then the Keyboard tab.</li>
		<li>Set Category to All Commands and click Copy List.</li>
		<li>Paste here, or save it in Notepad as a .txt file and load that.</li>
	</ol>

	<label class="st-import-label" for="{uid}-paste">Paste the Copy List</label>
	<textarea
		id="{uid}-paste"
		class="st-paste"
		bind:value={text}
		rows="6"
		spellcheck="false"
		placeholder="Category, Command and Shortcut(s) columns, as Copy List gives them"
		disabled={file !== null}></textarea>

	<div class="st-import-keys">
		<Key icon="upload" busy={busy === 'pick'} onclick={pick}>Load .txt or .sldreg file</Key>
		<Key
			icon="eye"
			variant="primary"
			busy={busy === 'preview'}
			disabled={!source}
			onclick={runPreview}>Preview</Key
		>
		{#if source || preview}
			<Key icon="close" onclick={reset}>Start over</Key>
		{/if}
	</div>
	{#if fileName}
		<p class="st-import-file">
			<Icon name="folder" size={16} />Loaded {fileName}. The paste box is set aside while a file is
			loaded.
		</p>
	{/if}

	{#if preview}
		<div class="st-preview-result plate-panel" aria-live="polite">
			<div class="st-result-head">
				<span class="plate-label">Preview</span>
				<span class="plate-chip">{FORMAT_WORDS[preview.format]}</span>
				{#if !preview.validated}<span class="plate-chip st-unval"
						><Icon name="warning" size={14} />Unvalidated</span
					>{/if}
			</div>
			<div class="st-counts">
				<div class="st-count">
					<span class="st-count-num">{preview.added.length}</span><span class="plate-label"
						>Added</span
					>
				</div>
				<div class="st-count">
					<span class="st-count-num">{preview.changed.length}</span><span class="plate-label"
						>Changed</span
					>
				</div>
				<div class="st-count">
					<span class="st-count-num">{preview.unchanged}</span><span class="plate-label"
						>Unchanged</span
					>
				</div>
				<div class="st-count" class:is-warn={preview.warnings.length > 0}>
					<span class="st-count-num">{preview.warnings.length}</span><span class="plate-label"
						>Warnings</span
					>
				</div>
			</div>
			{#if stale}
				<Notice tone="info" lead="Out of date"
					>The text or file changed after this preview. Preview again before you apply.</Notice
				>
			{/if}
			{#if preview.warnings.length > 0}
				<div class="st-warnings">
					<p class="st-sub"><Icon name="warning" size={16} />Warnings</p>
					<ul>
						{#each preview.warnings as w, i (i)}<li>{w}</li>{/each}
					</ul>
				</div>
			{/if}
			{#if preview.changed.length > 0}
				<p class="st-sub">Changed captions</p>
				<ul class="st-changes">
					{#each preview.changed.slice(0, SHOW) as c, i (i)}
						{@const winner = shadowedBy(preview.entries, c)}
						<li class:is-unused={!!winner}>
							<KeyCombo parts={displayKeyParts(c.keys)} size="s" />
							<span class="st-change-text">
								<span class="st-new">{c.command}</span>
								{#if c.previous}<span class="st-old">was {c.previous}</span>{/if}
								{#if winner}<span class="st-unused">Not used: {winner} comes first</span>{/if}
							</span>
						</li>
					{/each}
				</ul>
				{#if preview.changed.length > SHOW}<p class="st-more">
						and {preview.changed.length - SHOW} more
					</p>{/if}
			{/if}
			{#if preview.added.length > 0}
				<p class="st-sub">Added captions</p>
				<ul class="st-changes">
					{#each preview.added.slice(0, SHOW) as c, i (i)}
						{@const winner = shadowedBy(preview.entries, c)}
						<li class:is-unused={!!winner}>
							<KeyCombo parts={displayKeyParts(c.keys)} size="s" />
							<span class="st-change-text">
								<span class="st-new">{c.command}</span>
								{#if c.category}<span class="st-old">{c.category}</span>{/if}
								{#if winner}<span class="st-unused">Not used: {winner} comes first</span>{/if}
							</span>
						</li>
					{/each}
				</ul>
				{#if preview.added.length > SHOW}<p class="st-more">
						and {preview.added.length - SHOW} more
					</p>{/if}
			{/if}
			{#if preview.entries.length === 0}
				<Notice tone="danger" lead="Nothing to apply">
					No shortcut could be read. For a .sldreg file, use Copy List instead.
				</Notice>
			{/if}
			<div class="st-import-keys">
				<Key
					icon="check"
					variant="primary"
					busy={busy === 'apply'}
					disabled={preview.entries.length === 0 || stale}
					onclick={apply}
				>
					Apply {plural(preview.entries.length, 'shortcut')}
				</Key>
				<span class="st-apply-note">
					Applying replaces any earlier import. Your shortcuts win over the sourced captions for the
					same keys.
				</span>
			</div>
		</div>
	{/if}

	<div class="st-current">
		<div>
			<p class="st-current-title">
				{imported > 0 ? `${plural(imported, 'imported shortcut')} in use` : 'No imported shortcuts'}
			</p>
			<p class="st-current-help">
				{imported > 0
					? 'They are marked "Your import" in the list and keep their own captions.'
					: 'Only the sourced captions from the help pages are used.'}
			</p>
		</div>
		<ConfirmKey
			label="Clear imported shortcuts"
			confirmLabel="Clear them"
			disabled={imported === 0}
			busy={busy === 'clear'}
			onconfirm={clearImported}
		/>
	</div>
</div>

<style>
	.st-import {
		display: flex;
		flex-direction: column;
		gap: 0.8rem;
		padding: 1rem 0 1.1rem;
	}
	.st-import-steps {
		margin: 0;
		padding-left: 1.3rem;
		color: var(--plate-ink-2);
		line-height: 1.5;
	}
	.st-import-label {
		font-weight: 600;
		font-size: 1.05rem;
	}
	.st-paste {
		width: 100%;
		min-height: 140px;
		resize: vertical;
		padding: 0.7rem 0.8rem;
		border: 1px solid var(--plate-hair);
		border-radius: var(--plate-r-small);
		font-family: var(--font-mono);
		font-size: 0.88rem;
		line-height: 1.45;
		color: var(--plate-ink);
		background-color: var(--plate-panel-bot);
		background-image: linear-gradient(
			to bottom,
			var(--plate-chip-top),
			var(--plate-panel-bot) 18px
		);
		box-shadow:
			inset 0 1px 0 0 var(--plate-chip-shade),
			inset 0 2px 3px -1px var(--plate-chip-shade),
			0 1px 0 0 var(--plate-well-lip);
		outline: none;
	}
	.st-paste:focus-visible {
		outline: 2px solid var(--plate-focus);
		outline-offset: 2px;
	}
	.st-paste::placeholder {
		color: var(--plate-ink-2);
	}
	.st-paste:disabled {
		color: var(--plate-disabled);
		background: var(--plate-disabled-face);
	}
	.st-import-keys {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.5rem;
	}
	.st-import-file {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		margin: 0;
		color: var(--plate-ink-2);
	}
	.st-preview-result {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		padding: 1rem 1.1rem;
	}
	.st-result-head {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		flex-wrap: wrap;
	}
	.st-unval {
		color: var(--plate-warn-ink);
	}
	.st-counts {
		display: grid;
		grid-template-columns: repeat(4, minmax(0, 1fr));
		gap: 0.6rem;
	}
	.st-count {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.2rem;
		padding: 0.6rem 0.4rem;
		border-radius: var(--plate-r-control);
		background: linear-gradient(to bottom, var(--plate-screen-top), var(--plate-screen-bot));
		box-shadow:
			inset 0 0 0 1px var(--plate-screen-rim),
			inset 0 4px 8px -4px var(--plate-screen-shade);
	}
	.st-count .plate-label {
		color: var(--plate-screen-ink);
	}
	.st-count-num {
		font-family: var(--font-mono);
		font-size: 1.6rem;
		color: var(--plate-screen-ink);
		text-shadow: 0 0 6px var(--plate-screen-glow);
		font-variant-numeric: tabular-nums;
	}
	.st-count.is-warn .st-count-num,
	.st-count.is-warn .plate-label {
		color: #e7a85e;
		text-shadow: 0 0 6px rgba(231, 168, 94, 0.5);
	}
	.st-sub {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		margin: 0.2rem 0 0;
		font-weight: 700;
	}
	.st-warnings ul {
		margin: 0.35rem 0 0;
		padding-left: 1.3rem;
		color: var(--plate-ink-2);
		line-height: 1.45;
		font-size: 0.95rem;
	}
	.st-changes {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
	}
	.st-changes li {
		display: grid;
		grid-template-columns: minmax(10rem, 30%) minmax(0, 1fr);
		align-items: center;
		gap: 0.75rem;
	}
	.st-change-text {
		display: flex;
		flex-direction: column;
	}
	.st-new {
		font-weight: 600;
	}
	.st-old {
		font-size: 0.9rem;
		color: var(--plate-ink-2);
	}
	/* A duplicate binding that loses to an earlier line: struck through and said in words. */
	.is-unused .st-new {
		text-decoration: line-through;
		color: var(--plate-ink-2);
	}
	.st-unused {
		font-size: 0.9rem;
		color: var(--plate-warn-ink);
	}
	.st-more {
		margin: 0;
		color: var(--plate-ink-2);
		font-size: 0.92rem;
	}
	.st-apply-note {
		color: var(--plate-ink-2);
		font-size: 0.92rem;
		max-width: 32rem;
	}
	.st-current {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		flex-wrap: wrap;
		padding-top: 0.9rem;
		box-shadow:
			inset 0 1px 0 0 var(--plate-groove-dk),
			inset 0 2px 0 0 var(--plate-groove-lt);
	}
	.st-current-title {
		margin: 0;
		font-weight: 700;
		font-size: 1.05rem;
	}
	.st-current-help {
		margin: 0.1rem 0 0;
		color: var(--plate-ink-2);
		font-size: 0.95rem;
	}
</style>
