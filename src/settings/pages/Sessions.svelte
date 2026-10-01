<!--
	Sessions: the session log (start, stop, chapter markers), the saved
	sessions with their markers, and exports for the video description.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { useController } from '../state.svelte';
	import type { SessionSummary } from '../types';
	import { acceleratorParts } from '../lib/accelerator';
	import {
		EXPORT_KINDS,
		cleanMarkerName,
		exportFileName,
		markerTime,
		readSession,
		type ExportKind,
		type SessionDetail,
	} from '../lib/session';
	import { elapsedSince, formatClock, formatDateTime, formatDuration, plural } from '../lib/format';
	import PageTitle from '../ui/PageTitle.svelte';
	import Card from '../ui/Card.svelte';
	import Row from '../ui/Row.svelte';
	import Key from '../ui/Key.svelte';
	import ConfirmKey from '../ui/ConfirmKey.svelte';
	import Switch from '../ui/Switch.svelte';
	import TextField from '../ui/TextField.svelte';
	import KeyCombo from '../ui/KeyCombo.svelte';
	import Notice from '../ui/Notice.svelte';
	import Readout from '../ui/Readout.svelte';
	import Icon from '../ui/Icon.svelte';

	const ctl = useController();
	const s = $derived(ctl.settings!);
	const live = $derived(ctl.status?.session?.active ? ctl.status.session : null);

	let markerName = $state('');
	let list = $state<SessionSummary[] | null>(null);
	let listError = $state<string | null>(null);
	let selectedId = $state<string | null>(null);
	let detail = $state<SessionDetail | null>(null);
	let edits = $state<{ name: string }[]>([]);
	let busy = $state<string | null>(null);
	let lastExport = $state<string | null>(null);

	async function loadList() {
		try {
			list = await ctl.ipc.sessionsList();
			listError = null;
			if (selectedId && !list.some((x) => x.id === selectedId)) {
				selectedId = null;
				detail = null;
			}
		} catch (e) {
			listError = e instanceof Error ? e.message : String(e);
		}
	}

	async function select(id: string) {
		selectedId = id;
		lastExport = null;
		const raw = await ctl.run(() => ctl.ipc.sessionGet(id));
		detail = raw ? readSession(raw, id) : null;
		edits = detail ? detail.markers.map((m) => ({ name: m.name })) : [];
	}

	async function toggleSession() {
		busy = 'session';
		const wasActive = !!live;
		const ok = await ctl.statusCommand(() =>
			wasActive ? ctl.ipc.sessionStop() : ctl.ipc.sessionStart()
		);
		busy = null;
		if (ok && wasActive) {
			ctl.toast('Session log saved.', 'ok');
			await loadList();
		}
	}

	async function addMarker() {
		busy = 'marker';
		const name = cleanMarkerName(markerName);
		const ok = await ctl.statusCommand(
			() => ctl.ipc.sessionMarker(name.length > 0 ? name : null),
			'Marker added.'
		);
		busy = null;
		if (ok) markerName = '';
	}

	async function rename(index: number) {
		if (!detail) return;
		const name = cleanMarkerName(edits[index]?.name ?? '');
		if (name.length === 0) {
			ctl.toast('A marker needs a name.', 'danger');
			return;
		}
		busy = `rename-${index}`;
		const raw = await ctl.run(
			() => ctl.ipc.sessionRenameMarker(detail!.id, index, name),
			'Marker renamed.'
		);
		busy = null;
		if (raw) {
			detail = readSession(raw, detail.id);
			edits = detail ? detail.markers.map((m) => ({ name: m.name })) : [];
		}
	}

	async function exportAs(kind: ExportKind) {
		if (!detail) return;
		const d = detail;
		const path = await ctl.run(() =>
			ctl.ipc.pickSaveFile(`Save ${kind.label}`, exportFileName(d.started, kind), [
				{ name: kind.filterName, extensions: [kind.extension] },
			])
		);
		if (!path) return;
		busy = `export-${kind.format}`;
		const r = await ctl.run(() => ctl.ipc.sessionExport(d.id, kind.format, path));
		busy = null;
		if (r) {
			lastExport = r.path;
			ctl.toast(`Saved ${r.path}`, 'ok');
		}
	}

	async function remove() {
		if (!detail) return;
		const id = detail.id;
		const next = await ctl.run(() => ctl.ipc.sessionDelete(id), 'Session deleted.');
		if (next) {
			list = next;
			selectedId = null;
			detail = null;
		}
	}

	async function chooseFolder() {
		const path = await ctl.run(() =>
			ctl.ipc.pickFolder('Folder for session logs', s.session.folder ?? undefined)
		);
		if (path) s.session.folder = path;
	}

	onMount(() => {
		void loadList();
	});
</script>

<PageTitle
	title="Sessions"
	lead="A session log keeps the callouts that were shown and your chapter markers, to build subtitles, a shortcut list and YouTube chapters."
/>

<div class="st-stack">
	<Card title="Session log">
		<div class="st-live">
			<Readout live label="Session log">
				<div class="st-live-lines">
					{#if live}
						<span class="st-live-big"
							>Logging {formatClock(elapsedSince(live.started, ctl.now))}</span
						>
						<span class="st-live-small"
							>{plural(live.callouts, 'callout')} / {plural(live.markers, 'marker')}</span
						>
					{:else}
						<span class="st-live-big">Not logging</span>
						<span class="st-live-small">Nothing is written until you start</span>
					{/if}
				</div>
			</Readout>
			<div class="st-live-keys">
				<Key
					variant={live ? 'plain' : 'primary'}
					icon={live ? 'stop' : 'record'}
					iconFilled
					busy={busy === 'session'}
					onclick={toggleSession}
				>
					{live ? 'Stop and save' : 'Start session log'}
				</Key>
				<span class="st-hotkey"
					><span class="plate-label">Hotkey</span><KeyCombo
						parts={acceleratorParts(s.hotkeys.session_toggle)}
						size="s"
					/></span
				>
			</div>
		</div>
		<Row
			label="Chapter marker"
			help={live
				? 'Marks this moment as a chapter. Leave the name empty to name it later.'
				: 'Start a session log to add chapter markers.'}
			for="st-marker-name"
		>
			<div class="st-marker">
				<TextField
					id="st-marker-name"
					bind:value={markerName}
					placeholder="Chapter name"
					size={18}
					disabled={!live}
					maxlength={120}
					onenter={addMarker}
				/>
				<Key icon="flag" disabled={!live} busy={busy === 'marker'} onclick={addMarker}
					>Add marker</Key
				>
			</div>
		</Row>
		<Row label="Marker hotkey">
			<KeyCombo parts={acceleratorParts(s.hotkeys.chapter_marker)} size="s" />
		</Row>
		{#if s.obs.enabled && s.obs.auto_session}
			<Notice tone="info" lead="With OBS">
				A session log starts and stops with each OBS recording, so the markers line up with the
				video.
			</Notice>
			<div class="st-gap"></div>
		{/if}
	</Card>

	<Card title="Saved sessions" flush>
		{#snippet aside()}
			<Key icon="refresh" onclick={() => void loadList()}>Refresh</Key>
		{/snippet}
		<div class="st-saved">
			{#if listError}
				<Notice tone="danger" lead="Could not list the sessions" announce>{listError}</Notice>
			{:else if list === null}
				<p class="st-quiet" role="status">Loading the saved sessions</p>
			{:else if list.length === 0}
				<div class="st-empty">
					<svg viewBox="0 0 64 48" width="88" height="66" aria-hidden="true">
						<path
							d="M14 6h36a2 2 0 0 1 2 2v34l-20-9-20 9V8a2 2 0 0 1 2-2z"
							fill="none"
							stroke="currentColor"
							stroke-width="2"
							stroke-linejoin="round"
						/>
						<path
							d="M22 16h20M22 23h12"
							stroke="currentColor"
							stroke-width="2"
							stroke-linecap="round"
						/>
					</svg>
					<p class="st-empty-title">No saved sessions yet</p>
					<p class="st-empty-text">
						Start a session log above, press {acceleratorParts(s.hotkeys.session_toggle).join(
							' + '
						)}, or let OBS start one when it records. A session is saved here when it stops.
					</p>
				</div>
			{:else}
				<ul class="st-listwell st-sessions" aria-label="Saved sessions">
					{#each list as item (item.id)}
						<li class="st-etched">
							<button
								type="button"
								class="st-session"
								aria-pressed={selectedId === item.id}
								onclick={() => void select(item.id)}
							>
								<span class="st-session-date">{formatDateTime(item.started)}</span>
								<span class="st-session-meta">
									{formatDuration(item.duration_ms)} / {plural(item.callouts, 'callout')} / {plural(
										item.markers,
										'marker'
									)}
								</span>
								<span class="st-session-open">
									{selectedId === item.id ? 'Shown below' : 'Open'}<Icon
										name="arrowRight"
										size={16}
									/>
								</span>
							</button>
						</li>
					{/each}
				</ul>
			{/if}
		</div>
	</Card>

	{#if detail}
		<Card title="Session from {formatDateTime(detail.started)}">
			<div class="st-detail">
				<p class="st-detail-meta">
					{detail.durationMs !== null ? formatDuration(detail.durationMs) : ''} /
					{plural(detail.callouts, 'callout')} / {plural(detail.markers.length, 'marker')}
				</p>
				<p class="st-sub">Chapter markers</p>
				{#if detail.markers.length === 0}
					<p class="st-quiet">
						No markers in this session. The chapters export then holds only 00:00 Start, and YouTube
						shows chapters only when there are at least three.
					</p>
				{:else}
					<ol class="st-markers">
						{#each detail.markers as m, i (m.index)}
							{@const edit = edits[i]}
							<li>
								<span class="st-marker-time">{markerTime(m)}</span>
								{#if edit}
									<TextField
										bind:value={edit.name}
										label="Name of the marker at {markerTime(m)}"
										maxlength={120}
										onenter={() => void rename(i)}
									/>
								{/if}
								<Key
									icon="pencil"
									busy={busy === `rename-${i}`}
									disabled={cleanMarkerName(edit?.name ?? '') === m.name}
									onclick={() => void rename(i)}
								>
									Rename
								</Key>
							</li>
						{/each}
					</ol>
				{/if}
				<p class="st-sub">Export</p>
				<div class="st-exports">
					{#each EXPORT_KINDS as kind (kind.format)}
						<div class="st-export">
							<Key
								icon="download"
								busy={busy === `export-${kind.format}`}
								onclick={() => void exportAs(kind)}
							>
								{kind.label}
							</Key>
							<span class="st-export-help">{kind.help}</span>
						</div>
					{/each}
				</div>
				{#if lastExport}
					<Notice tone="ok" lead="Saved">{lastExport}</Notice>
				{/if}
				<div class="st-detail-foot">
					<ConfirmKey
						label="Delete session"
						confirmLabel="Delete it"
						question="Delete this session log?"
						onconfirm={remove}
					/>
				</div>
			</div>
		</Card>
	{/if}

	<Card title="Where sessions are kept">
		<Row
			label="Folder"
			help={s.session.folder ?? 'Documents, IDEA Keystage, Sessions (the default).'}
		>
			<div class="st-folder-keys">
				<Key icon="folder" onclick={chooseFolder}>Choose folder</Key>
				{#if s.session.folder}<Key onclick={() => (s.session.folder = null)}>Use the default</Key
					>{/if}
				<Key
					icon="external"
					onclick={() => void ctl.run(() => ctl.ipc.openExternalFolder('sessions'))}>Open</Key
				>
			</div>
		</Row>
		<Row
			label="Save when a session stops"
			help="Writes the session log to the folder as soon as it stops."
		>
			<Switch bind:checked={s.session.autosave} label="Save when a session stops" />
		</Row>
	</Card>
</div>

<style>
	.st-stack {
		display: flex;
		flex-direction: column;
		gap: 1.6rem;
	}
	.st-live {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto;
		gap: 1rem;
		align-items: center;
		padding: 1rem 0 0.6rem;
	}
	.st-live-lines {
		display: flex;
		flex-direction: column;
		gap: 0.2rem;
	}
	.st-live-big {
		font-size: 1.35rem;
		letter-spacing: 0.08em;
		text-transform: uppercase;
	}
	.st-live-small {
		font-size: 0.82rem;
		letter-spacing: 0.08em;
		text-transform: uppercase;
	}
	.st-live-keys {
		display: flex;
		flex-direction: column;
		align-items: stretch;
		gap: 0.5rem;
	}
	.st-hotkey {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 0.5rem;
	}
	.st-marker {
		display: flex;
		align-items: flex-start;
		gap: 0.5rem;
		flex-wrap: wrap;
		justify-content: flex-end;
	}
	.st-gap {
		height: 0.9rem;
	}
	.st-saved {
		padding: 0.9rem;
	}
	.st-quiet {
		margin: 0;
		padding: 0.6rem 0;
		color: var(--plate-ink-2);
	}
	.st-empty {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.35rem;
		padding: 1.4rem 1rem;
		text-align: center;
		color: var(--plate-ink-2);
	}
	.st-empty-title {
		margin: 0.3rem 0 0;
		font-weight: 700;
		font-size: 1.15rem;
		color: var(--plate-ink);
	}
	.st-empty-text {
		margin: 0;
		max-width: 34rem;
		line-height: 1.45;
	}
	.st-sessions {
		list-style: none;
		margin: 0;
		padding: 0.35rem;
	}
	.st-session {
		position: relative;
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto;
		align-items: center;
		gap: 0.1rem 1rem;
		width: 100%;
		min-height: 56px;
		padding: 0.55rem 0.8rem;
		border: 0;
		border-radius: var(--plate-r-small);
		background: transparent;
		text-align: left;
		cursor: pointer;
		color: var(--plate-ink);
	}
	.st-session:hover {
		box-shadow: inset 0 0 0 1px var(--plate-hover-ink);
	}
	.st-session:focus-visible {
		outline: 2px solid var(--plate-focus);
		outline-offset: 1px;
	}
	.st-session[aria-pressed='true'] {
		background: var(--plate-sel-face);
		color: var(--plate-sel-ink);
		box-shadow:
			0 0 0 1px var(--plate-sel-edge),
			0 0 10px 1px var(--plate-sel-glow),
			0 2px 3px 0 var(--plate-drop);
	}
	.st-session-date {
		font-weight: 700;
		font-size: 1.05rem;
	}
	.st-session-meta {
		grid-column: 1;
		font-size: 0.92rem;
		color: var(--plate-ink-2);
	}
	.st-session[aria-pressed='true'] .st-session-meta,
	.st-session[aria-pressed='true'] .st-session-open {
		color: var(--plate-sel-ink);
	}
	.st-session-open {
		grid-column: 2;
		grid-row: 1 / span 2;
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		color: var(--plate-caption);
		font-family: var(--font-mono);
		font-size: 0.72rem;
		letter-spacing: 0.1em;
		text-transform: uppercase;
	}
	.st-detail {
		display: flex;
		flex-direction: column;
		gap: 0.7rem;
		padding: 1rem 0 1.1rem;
	}
	.st-detail-meta {
		margin: 0;
		color: var(--plate-ink-2);
	}
	.st-sub {
		margin: 0.4rem 0 0;
		font-weight: 700;
		font-size: 1.05rem;
	}
	.st-markers {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.45rem;
	}
	.st-markers li {
		display: grid;
		grid-template-columns: 4.5rem minmax(0, 1fr) auto;
		align-items: center;
		gap: 0.6rem;
	}
	.st-markers li :global(.st-field-wrap) {
		width: 100%;
	}
	.st-marker-time {
		font-family: var(--font-mono);
		font-variant-numeric: tabular-nums;
		color: var(--plate-ink-2);
	}
	.st-exports {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
		gap: 0.6rem 1rem;
	}
	.st-export {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 0.3rem;
	}
	.st-export-help {
		font-size: 0.9rem;
		color: var(--plate-ink-2);
	}
	.st-detail-foot {
		display: flex;
		justify-content: flex-end;
		padding-top: 0.6rem;
	}
	.st-folder-keys {
		display: flex;
		gap: 0.5rem;
		flex-wrap: wrap;
		justify-content: flex-end;
	}
</style>
