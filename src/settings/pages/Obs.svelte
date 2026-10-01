<!--
	OBS: the Browser Sources to add (URL, size, the OBS fields to set), the
	local server's port, and the optional obs-websocket link for recording
	state, chapter markers and automatic session logs.
-->
<script lang="ts">
	import { useController } from '../state.svelte';
	import { chosenMonitor } from '../lib/monitors';
	import {
		OBS_DEFAULT_CSS,
		OBS_FIELDS,
		OBS_WEBSOCKET_STEPS,
		isLoopbackHost,
		obsHostError,
		sourceCards,
		sourcesSummary,
	} from '../lib/obs';
	import { obsWord, recordingText, serverWord } from '../lib/nav';
	import { formatTimecode } from '../lib/format';
	import type { ObsTestResult } from '../types';
	import PageTitle from '../ui/PageTitle.svelte';
	import Card from '../ui/Card.svelte';
	import Row from '../ui/Row.svelte';
	import Switch from '../ui/Switch.svelte';
	import Key from '../ui/Key.svelte';
	import ConfirmKey from '../ui/ConfirmKey.svelte';
	import TextField from '../ui/TextField.svelte';
	import NumberField from '../ui/NumberField.svelte';
	import Notice from '../ui/Notice.svelte';
	import StatusChip from '../ui/StatusChip.svelte';
	import Readout from '../ui/Readout.svelte';
	import SourceCard from '../parts/SourceCard.svelte';
	import Icon from '../ui/Icon.svelte';

	const ctl = useController();
	const s = $derived(ctl.settings!);
	const status = $derived(ctl.status);
	const monitor = $derived(chosenMonitor(ctl.monitors, s.host.monitor));
	const cards = $derived(sourceCards(ctl.urls, s.server.port, s.overlay, monitor));
	const server = $derived(status ? serverWord(status) : null);
	const obs = $derived(status ? obsWord(status) : null);
	const rec = $derived(status ? recordingText(status) : null);
	const hostError = $derived(obsHostError(s.obs.host));
	const copy = (text: string) => ctl.ipc.copyText(text);

	let password = $state('');
	let savingPassword = $state(false);
	let testing = $state(false);
	let test = $state<ObsTestResult | null>(null);

	async function savePassword() {
		if (password.length === 0) return;
		savingPassword = true;
		await ctl.flush();
		const r = await ctl.run(() => ctl.ipc.obsSetPassword(password));
		// The password is never kept in this window once it is sent.
		password = '';
		if (r) {
			const fresh = await ctl.run(() => ctl.ipc.getSettings());
			if (fresh) ctl.adopt(fresh);
			ctl.toast(
				r.stored ? 'Password saved, encrypted for your Windows account.' : 'Password not saved.',
				r.stored ? 'ok' : 'danger'
			);
		}
		savingPassword = false;
	}

	async function clearPassword() {
		await ctl.flush();
		const r = await ctl.run(() => ctl.ipc.obsSetPassword(null));
		if (r) {
			const fresh = await ctl.run(() => ctl.ipc.getSettings());
			if (fresh) ctl.adopt(fresh);
			ctl.toast('Password removed.', 'ok');
		}
	}

	async function testConnection() {
		testing = true;
		test = null;
		const typed = password.length > 0 ? password : null;
		test = await ctl.run(() => ctl.ipc.obsTest(s.obs.host.trim(), s.obs.port, typed));
		testing = false;
	}
</script>

<PageTitle
	title="OBS"
	lead="Add IDEA Keystage to OBS as Browser Sources. Each source is fully set up by its address."
/>

<div class="st-stack">
	<Card
		title="Local server"
		lead="OBS loads the sources from this computer only. Nothing is reachable from other computers."
	>
		{#snippet aside()}
			{#if server}<StatusChip tone={server.tone} text={server.word} live />{/if}
		{/snippet}
		{#if status && !status.server.running}
			<div class="st-pad-top">
				<Notice tone="danger" lead="The sources cannot connect" announce>
					{status.server.error ?? `Port ${status.server.port} is not serving.`} Pick a free port below;
					the addresses on the cards change with it, so copy them into OBS again.
				</Notice>
			</div>
		{/if}
		<Row
			label="Port"
			help={status?.server.running
				? (server?.detail ?? '')
				: 'Pick a free port from 1024 to 65535. 47820 is the default.'}
			for="st-port"
		>
			<NumberField
				id="st-port"
				bind:value={s.server.port}
				min={1024}
				max={65535}
				label="Server port"
			/>
		</Row>
	</Card>

	<section class="st-sources" aria-labelledby="st-sources-title">
		<header class="st-sources-head">
			<h2 class="plate-label" id="st-sources-title">Browser Sources</h2>
			<Key
				icon="copy"
				onclick={() =>
					void ctl.run(() => copy(sourcesSummary(cards)), 'Every address and size copied.')}
			>
				Copy all
			</Key>
		</header>
		<p class="st-sources-lead">
			In OBS, add a Browser Source (Sources, +, Browser), paste the URL and type the width and
			height shown. The sizes follow your settings and the chosen monitor.
		</p>
		<div class="st-source-list">
			{#each cards as card (card.route)}
				<SourceCard {card} {copy} />
			{/each}
		</div>
	</section>

	<Card title="Browser Source fields" lead="Set these in each source's properties in OBS.">
		<table class="st-fields">
			<thead>
				<tr>
					<th scope="col">Field in OBS</th>
					<th scope="col">Set to</th>
					<th scope="col">Why</th>
				</tr>
			</thead>
			<tbody>
				{#each OBS_FIELDS as f (f.field)}
					<tr>
						<th scope="row">{f.field}</th>
						<td><span class="plate-chip st-field-value">{f.value}</span></td>
						<td>{f.note}</td>
					</tr>
				{/each}
			</tbody>
		</table>
		<details class="st-css">
			<summary
				><span class="st-chev"><Icon name="arrowRight" size={16} /></span>The default Custom CSS</summary
			>
			<code>{OBS_DEFAULT_CSS}</code>
		</details>
	</Card>

	<Card
		title="obs-websocket"
		lead="Optional. With it, IDEA Keystage knows when OBS records, shows the recording time, and starts a session log with each recording."
	>
		{#snippet aside()}
			{#if obs}<StatusChip tone={obs.tone} text={obs.word} live />{/if}
		{/snippet}
		<Row label="Connect to OBS" help="Uses the WebSocket server built into OBS 28 and later.">
			<Switch bind:checked={s.obs.enabled} label="Connect to OBS" />
		</Row>
		{#if s.obs.enabled}
			{#if status && (status.obs.state === 'auth_failed' || status.obs.state === 'error')}
				<Notice
					tone="danger"
					lead={status.obs.state === 'auth_failed' ? 'Sign-in failed' : 'Not connected'}
					announce
				>
					{status.obs.error ??
						(status.obs.state === 'auth_failed'
							? 'OBS did not accept the password.'
							: 'OBS did not answer.')}
				</Notice>
			{/if}
			{#if rec && status?.obs.recording}
				<div class="st-pad-top">
					<Readout live label="Recording time">
						<div class="st-rec">
							<span class="st-rec-dot" aria-hidden="true"></span>
							<span class="st-rec-word">{status.obs.recording.paused ? 'Rec paused' : 'Rec'}</span>
							<span class="st-rec-time">{formatTimecode(status.obs.recording.timecode)}</span>
						</div>
					</Readout>
				</div>
			{/if}
			<Row label="Set up OBS first" stack>
				<ol class="st-steps">
					{#each OBS_WEBSOCKET_STEPS as step, i (i)}
						<li>{step}</li>
					{/each}
				</ol>
			</Row>
			<Row
				label="Computer"
				help="127.0.0.1 is this computer. Use another address only when OBS runs on another computer."
				for="st-obs-host"
			>
				<TextField id="st-obs-host" bind:value={s.obs.host} mono size={18} error={hostError} />
			</Row>
			{#if !hostError && !isLoopbackHost(s.obs.host)}
				<Notice tone="warn" lead="Another computer">
					The password and the recording state travel over your network. Keep authentication on in
					OBS.
				</Notice>
			{/if}
			<Row
				label="Port"
				help="The Server Port in OBS's WebSocket Server Settings."
				for="st-obs-port"
			>
				<NumberField
					id="st-obs-port"
					bind:value={s.obs.port}
					min={1}
					max={65535}
					label="OBS WebSocket port"
				/>
			</Row>
			<Row
				label="Password"
				help={s.obs.password_dpapi
					? 'A password is saved, encrypted for your Windows account. It is never shown again; type a new one to replace it.'
					: 'Paste the Server Password from Show Connect Info. It is saved encrypted and never shown again.'}
				for="st-obs-pw"
			>
				<div class="st-pw">
					<TextField
						id="st-obs-pw"
						type="password"
						bind:value={password}
						mono
						size={16}
						autocomplete="new-password"
						placeholder={s.obs.password_dpapi ? 'New password' : 'Server password'}
						onenter={savePassword}
					/>
					<Key
						variant="primary"
						icon="lock"
						busy={savingPassword}
						disabled={password.length === 0}
						onclick={savePassword}
					>
						Save password
					</Key>
				</div>
			</Row>
			{#if s.obs.password_dpapi}
				<Row label="Saved password" help="Remove it if OBS no longer asks for one.">
					<span class="st-pw-saved">
						<StatusChip tone="ok" text="Saved" />
						<ConfirmKey label="Remove" confirmLabel="Remove password" onconfirm={clearPassword} />
					</span>
				</Row>
			{/if}
			<Row
				label="Test the connection"
				help="Signs in to OBS once with these values and reports what answered."
			>
				<Key icon="broadcast" busy={testing} disabled={!!hostError} onclick={testConnection}
					>Test connection</Key
				>
			</Row>
			{#if test}
				{#if test.ok}
					<Notice tone="ok" lead="Connected" announce>
						OBS {test.obs_version ?? ''} answered{test.ws_version
							? ` (WebSocket ${test.ws_version})`
							: ''}.
					</Notice>
				{:else}
					<Notice tone="danger" lead="No connection" announce
						>{test.error ?? 'OBS did not answer.'}</Notice
					>
				{/if}
			{/if}
			<Row
				label="Session log with each recording"
				help="Starts a session log when OBS starts recording and stops it with the recording, so chapter markers line up with the video."
			>
				<Switch bind:checked={s.obs.auto_session} label="Session log with each recording" />
			</Row>
			<Notice tone="info" lead="Chapter markers in the video file">
				OBS writes chapter markers into the recording only when its recording format is Hybrid MP4
				(OBS 30.2 or later) or Hybrid MOV (OBS 32 or later). Markers are always kept in the session
				log either way.
			</Notice>
			<div class="st-pad-bottom"></div>
		{/if}
	</Card>
</div>

<style>
	.st-stack {
		display: flex;
		flex-direction: column;
		gap: 1.6rem;
	}
	.st-pad-top {
		padding-top: 0.9rem;
	}
	.st-pad-bottom {
		height: 0.9rem;
	}
	.st-sources {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}
	.st-sources-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 0 0.25rem;
	}
	.st-sources-head h2 {
		margin: 0;
		font-weight: 400;
	}
	.st-sources-lead {
		margin: 0 0.25rem 0.25rem;
		color: var(--plate-ink-2);
	}
	.st-source-list {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}
	.st-fields {
		width: 100%;
		border-collapse: collapse;
		margin: 0.6rem 0 0.4rem;
		font-size: 0.97rem;
	}
	.st-fields th,
	.st-fields td {
		text-align: left;
		vertical-align: top;
		padding: 0.6rem 0.6rem 0.6rem 0;
	}
	.st-fields thead th {
		font-family: var(--font-mono);
		font-size: var(--plate-label-size);
		letter-spacing: var(--plate-label-track);
		text-transform: uppercase;
		font-weight: 400;
		color: var(--plate-caption);
	}
	.st-fields tbody tr {
		box-shadow:
			inset 0 1px 0 0 var(--plate-groove-dk),
			inset 0 2px 0 0 var(--plate-groove-lt);
	}
	.st-fields tbody th {
		font-weight: 600;
		color: var(--plate-ink);
		width: 34%;
	}
	.st-fields td {
		color: var(--plate-ink-2);
	}
	.st-field-value {
		white-space: nowrap;
	}
	.st-css {
		margin: 0.3rem 0 0.9rem;
		color: var(--plate-ink-2);
	}
	.st-css summary {
		cursor: pointer;
		min-height: 44px;
		display: flex;
		align-items: center;
		gap: 0.4rem;
		font-weight: 600;
		color: var(--plate-ink);
		list-style: none;
	}
	.st-css summary::-webkit-details-marker {
		display: none;
	}
	.st-chev {
		display: inline-flex;
		color: var(--plate-accent);
	}
	.st-css[open] .st-chev {
		transform: rotate(90deg);
	}
	.st-css code {
		display: block;
		padding: 0.7rem 0.8rem;
		border-radius: var(--plate-r-small);
		font-family: var(--font-mono);
		font-size: 0.85rem;
		color: var(--plate-ink);
		background: var(--plate-well-face);
		box-shadow: inset 0 1px 2px 0 var(--plate-well-shade);
		overflow-wrap: anywhere;
	}
	.st-rec {
		display: flex;
		align-items: center;
		gap: 0.7rem;
	}
	.st-rec-dot {
		width: 12px;
		height: 12px;
		border-radius: 50%;
		background: radial-gradient(circle at 42% 36%, #ffd9d9 0 1px, #e05555 3px);
		box-shadow: 0 0 8px 2px rgba(224, 85, 85, 0.55);
	}
	.st-rec-word {
		font-size: 0.9rem;
		letter-spacing: 0.16em;
		text-transform: uppercase;
	}
	.st-rec-time {
		font-size: 1.6rem;
		letter-spacing: 0.08em;
	}
	.st-steps {
		margin: 0;
		padding-left: 1.3rem;
		color: var(--plate-ink-2);
		line-height: 1.5;
	}
	.st-steps li + li {
		margin-top: 0.25rem;
	}
	.st-pw {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		flex-wrap: wrap;
		justify-content: flex-end;
	}
	.st-pw-saved {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		flex-wrap: wrap;
		justify-content: flex-end;
	}
</style>
