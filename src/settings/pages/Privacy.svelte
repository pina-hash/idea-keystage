<!--
	Privacy: in plain words, what the app watches, what it keeps and what it
	never does (ARCHITECTURE.md section 8), the password guard (always on),
	and the excluded apps.
-->
<script lang="ts">
	import { useController } from '../state.svelte';
	import { acceleratorParts } from '../lib/accelerator';
	import { addExcludedApp, removeExcludedApp } from '../lib/excluded';
	import { DEFAULT_EXCLUDED_APPS } from '../defaults';
	import PageTitle from '../ui/PageTitle.svelte';
	import Card from '../ui/Card.svelte';
	import Row from '../ui/Row.svelte';
	import Key from '../ui/Key.svelte';
	import ConfirmKey from '../ui/ConfirmKey.svelte';
	import TextField from '../ui/TextField.svelte';
	import KeyCombo from '../ui/KeyCombo.svelte';
	import StatusChip from '../ui/StatusChip.svelte';
	import Icon from '../ui/Icon.svelte';

	const ctl = useController();
	const s = $derived(ctl.settings!);
	let entry = $state('');
	let error = $state<string | null>(null);
	let note = $state<string | null>(null);

	function add() {
		const r = addExcludedApp(s.privacy.excluded_apps, entry);
		if (!r.ok) {
			error = r.error;
			note = null;
			return;
		}
		s.privacy.excluded_apps = r.list;
		error = null;
		note = r.note ?? `${r.added} added.`;
		entry = '';
	}

	function remove(name: string) {
		s.privacy.excluded_apps = removeExcludedApp(s.privacy.excluded_apps, name);
		note = `${name} removed.`;
	}

	const sameAsDefault = $derived(
		s.privacy.excluded_apps.length === DEFAULT_EXCLUDED_APPS.length &&
			s.privacy.excluded_apps.every((v, i) => v === DEFAULT_EXCLUDED_APPS[i])
	);

	const WATCHES = [
		'The keys you press, mouse buttons, the wheel and where the pointer is, through Windows input hooks, only while it is active. Pause removes the hooks; nothing is seen until you resume.',
		'Which app is in front, to switch to the SolidWorks profile and to hide everything in an excluded app.',
		'Whether a password field has focus, to hide everything while it does.',
	];
	const KEEPS = [
		"Your settings, in the app's config folder.",
		'A session log, only when you start one or turn on OBS auto sessions. It holds only the callouts that were shown and your chapter markers.',
		'Your OBS password, encrypted by Windows for your account only. It is never shown again.',
	];
	const NEVER = [
		'Connects to the internet: no account, no telemetry, no update check.',
		'Writes what you type to disk, except a session log you started.',
		'Puts key names, typed text, callouts or captions in its diagnostic log.',
		'Lets another computer or a web page read your keys. The local server answers on 127.0.0.1 only and only to pages from that address.',
		'Opens any other connection than the OBS WebSocket address you set, and only while you have it turned on.',
	];
</script>

<PageTitle title="Privacy" lead="What IDEA Keystage sees, what it keeps, and what it never does." />

<div class="st-stack">
	<div class="st-facts">
		<section class="plate-panel st-fact" aria-labelledby="st-f1">
			<h2 id="st-f1" class="st-fact-title"><Icon name="eye" size={20} />What it watches</h2>
			<ul>
				{#each WATCHES as w, i (i)}<li>{w}</li>{/each}
			</ul>
		</section>
		<section class="plate-panel st-fact" aria-labelledby="st-f2">
			<h2 id="st-f2" class="st-fact-title"><Icon name="folder" size={20} />What it keeps</h2>
			<ul>
				{#each KEEPS as w, i (i)}<li>{w}</li>{/each}
			</ul>
		</section>
		<section class="plate-panel st-fact is-never" aria-labelledby="st-f3">
			<h2 id="st-f3" class="st-fact-title"><Icon name="shield" size={20} />What it never does</h2>
			<ul>
				{#each NEVER as w, i (i)}<li>{w}</li>{/each}
			</ul>
		</section>
	</div>

	<Card title="Password guard">
		{#snippet aside()}
			<StatusChip tone="ok" text="Always on" />
		{/snippet}
		<div class="st-guard">
			<span class="st-guard-icon"><Icon name="lock" size={30} /></span>
			<div class="st-guard-text">
				<p>
					While a password field has focus, nothing is shown, lit on the keyboard, logged or played.
					Windows accessibility reports most password boxes, and classic Windows password fields are
					checked directly. If the app cannot tell, it stays hidden.
				</p>
				<p>
					There is no switch: the guard cannot be turned off. A program that does not report its
					password boxes to Windows cannot be guarded this way, so add it to the excluded apps
					below.
				</p>
			</div>
		</div>
	</Card>

	<Card
		title="Excluded apps"
		lead="While one of these is the app in front, nothing is shown, logged or played. The list starts with common password managers and Windows sign-in screens."
	>
		<Row
			label="Add an app"
			help="Its process name, for example KeePass.exe. Find it in Task Manager, Details."
			for="st-excl"
		>
			<div class="st-add">
				<TextField
					id="st-excl"
					bind:value={entry}
					mono
					size={20}
					placeholder="Process name"
					{error}
					onenter={add}
					oninput={() => (error = null)}
				/>
				<Key icon="plus" onclick={add}>Add</Key>
			</div>
		</Row>
		<p class="st-excl-note" role="status">{note ?? ''}</p>
		{#if s.privacy.excluded_apps.length === 0}
			<p class="st-excl-empty">No apps are excluded. Only the password guard hides input.</p>
		{:else}
			<ul class="st-excl" aria-label="Excluded apps">
				{#each s.privacy.excluded_apps as name (name)}
					<li>
						<span class="st-excl-name">{name}</span>
						<Key icon="close" label="Remove {name}" onclick={() => remove(name)} />
					</li>
				{/each}
			</ul>
		{/if}
		<div class="st-excl-foot">
			<ConfirmKey
				label="Restore the starting list"
				confirmLabel="Restore it"
				icon="refresh"
				disabled={sameAsDefault}
				onconfirm={() => {
					s.privacy.excluded_apps = [...DEFAULT_EXCLUDED_APPS];
					note = 'The starting list is back.';
				}}
			/>
		</div>
	</Card>

	<Card title="Pause">
		<Row
			label="Pause hotkey"
			help="Stops everything and removes the input hooks at once, from any app."
		>
			<KeyCombo parts={acceleratorParts(s.hotkeys.pause)} />
		</Row>
	</Card>
</div>

<style>
	.st-stack {
		display: flex;
		flex-direction: column;
		gap: 1.6rem;
	}
	.st-facts {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
		gap: 0.9rem;
	}
	.st-fact {
		padding: 1rem 1.1rem 1.1rem;
	}
	.st-fact-title {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		margin: 0 0 0.6rem;
		font-family: var(--font-mono);
		font-size: 0.78rem;
		font-weight: 400;
		letter-spacing: 0.14em;
		text-transform: uppercase;
		color: var(--plate-accent);
	}
	.st-fact ul {
		margin: 0;
		padding-left: 1.1rem;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		line-height: 1.45;
		font-size: 0.98rem;
	}
	.st-guard {
		display: flex;
		gap: 1rem;
		padding: 1rem 0;
	}
	.st-guard-icon {
		flex: none;
		display: grid;
		place-items: center;
		width: 56px;
		height: 56px;
		border-radius: 12px;
		color: var(--plate-accent);
		background: linear-gradient(to bottom, var(--plate-chip-top), var(--plate-chip-bot));
		box-shadow:
			inset 0 0 0 1px var(--plate-chip-edge),
			inset 0 2px 3px -1px var(--plate-chip-shade);
	}
	.st-guard-text p {
		margin: 0 0 0.6rem;
		line-height: 1.5;
	}
	.st-guard-text p:last-child {
		margin: 0;
		color: var(--plate-ink-2);
	}
	.st-add {
		display: flex;
		align-items: flex-start;
		gap: 0.5rem;
	}
	.st-excl-note {
		margin: 0 0 0.4rem;
		min-height: 1.3em;
		color: var(--plate-ink-2);
		font-size: 0.95rem;
	}
	.st-excl-empty {
		margin: 0 0 1rem;
		color: var(--plate-ink-2);
	}
	.st-excl {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
		gap: 0.5rem;
	}
	.st-excl li {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.5rem;
		padding: 4px 4px 4px 0.8rem;
		border-radius: var(--plate-r-control);
		background-color: var(--plate-chip-ground);
		background-image: linear-gradient(to bottom, var(--plate-chip-top), var(--plate-chip-bot));
		box-shadow:
			inset 0 0 0 1px var(--plate-chip-edge),
			inset 0 2px 3px -1px var(--plate-chip-shade),
			inset 0 -1px 0 0 var(--plate-chip-lip),
			0 1px 0 0 var(--plate-chip-lip);
	}
	.st-excl-name {
		font-family: var(--font-mono);
		font-size: 0.92rem;
		overflow-wrap: anywhere;
	}
	.st-excl-foot {
		display: flex;
		justify-content: flex-end;
		padding: 0.9rem 0 1rem;
	}
</style>
