<!--
	General: startup, this window's look, the settings file, folders, and
	about (version and licenses).
-->
<script lang="ts">
	import { useController } from '../state.svelte';
	import PageTitle from '../ui/PageTitle.svelte';
	import Card from '../ui/Card.svelte';
	import Row from '../ui/Row.svelte';
	import Switch from '../ui/Switch.svelte';
	import Segmented from '../ui/Segmented.svelte';
	import Key from '../ui/Key.svelte';
	import ConfirmKey from '../ui/ConfirmKey.svelte';
	import Mark from '../parts/Mark.svelte';

	const ctl = useController();
	const s = $derived(ctl.settings!);
	let busy = $state<string | null>(null);

	async function exportSettings() {
		const path = await ctl.run(() =>
			ctl.ipc.pickSaveFile('Export settings', 'keystage-settings.json', [
				{ name: 'Settings', extensions: ['json'] },
			])
		);
		if (!path) return;
		busy = 'export';
		await ctl.flush();
		const r = await ctl.run(() => ctl.ipc.settingsExport(path), `Settings saved to ${path}`);
		busy = null;
		void r;
	}

	async function importSettings() {
		const path = await ctl.run(() =>
			ctl.ipc.pickOpenFile('Import settings', [{ name: 'Settings', extensions: ['json'] }])
		);
		if (!path) return;
		busy = 'import';
		await ctl.flush();
		const r = await ctl.run(() => ctl.ipc.settingsImport(path), 'Settings imported.');
		busy = null;
		if (r) ctl.adopt(r);
	}

	async function reset() {
		busy = 'reset';
		await ctl.flush();
		const r = await ctl.run(() => ctl.ipc.resetSettings(), 'Settings are back to their defaults.');
		busy = null;
		if (r) {
			ctl.adopt(r);
			await ctl.loadProfile(true);
		}
	}

	const FONTS = [
		{ name: 'Rajdhani', by: 'Indian Type Foundry', use: 'Text and captions' },
		{
			name: 'Share Tech Mono',
			by: 'Carrois Type Design, Ralph du Carrois',
			use: 'Labels and readouts',
		},
		{ name: 'Orbitron', by: 'The Orbitron Project Authors', use: 'Titles' },
	];
	const PARTS = [
		{ name: 'Tauri', license: 'MIT or Apache 2.0' },
		{ name: 'Svelte', license: 'MIT' },
	];
</script>

<PageTitle
	title="General"
	lead="Startup, this window, your settings file and the folders the app uses."
/>

<div class="st-stack">
	<Card title="Startup">
		<Row label="Start with Windows" help="Starts IDEA Keystage in the tray when you sign in.">
			<Switch bind:checked={s.startup.start_with_windows} label="Start with Windows" />
		</Row>
		<Row
			label="Open this window at start"
			help="Off: the app starts quietly in the tray. Open settings from the tray icon."
		>
			<Switch
				checked={s.startup.show_settings ?? true}
				label="Open this window at start"
				onchange={(v) => (s.startup.show_settings = v)}
			/>
		</Row>
	</Card>

	<Card title="This window">
		<Row
			label="Look"
			help="IDEA dark plates or Space White light plates. Changes only this window, not the overlay."
		>
			<Segmented
				value={ctl.appearance}
				options={[
					{ value: 'dark', label: 'Dark', icon: 'moon' },
					{ value: 'light', label: 'Light', icon: 'sun' },
				]}
				label="Window look"
				onchange={(v) => ctl.setAppearance(v)}
			/>
		</Row>
		<Row label="Setup" help="Walk through the first-run steps again: monitor, look and output.">
			<Key icon="sparkle" onclick={() => (s.first_run_done = false)}>Run setup again</Key>
		</Row>
	</Card>

	<Card title="Settings file">
		<Row
			label="Export"
			help="Saves every setting to a file, for a backup or another computer. The OBS password is not included."
		>
			<Key icon="download" busy={busy === 'export'} onclick={exportSettings}>Export settings</Key>
		</Row>
		<Row label="Import" help="Replaces your settings with the ones in a file you exported.">
			<Key icon="upload" busy={busy === 'import'} onclick={importSettings}>Import settings</Key>
		</Row>
		<Row
			label="Reset"
			help="Every setting goes back to its default, imported SolidWorks shortcuts included."
		>
			<ConfirmKey
				label="Reset to defaults"
				confirmLabel="Reset everything"
				icon="refresh"
				busy={busy === 'reset'}
				onconfirm={reset}
			/>
		</Row>
	</Card>

	<Card title="Folders">
		<Row label="Session logs" help="Where saved sessions and their exports go.">
			<Key icon="folder" onclick={() => void ctl.run(() => ctl.ipc.openExternalFolder('sessions'))}
				>Open sessions folder</Key
			>
		</Row>
		<Row label="Settings" help="Where the app keeps its settings file.">
			<Key icon="folder" onclick={() => void ctl.run(() => ctl.ipc.openExternalFolder('config'))}
				>Open config folder</Key
			>
		</Row>
	</Card>

	<Card title="About">
		<div class="st-about">
			<div class="st-about-head">
				<Mark />
				<div class="st-about-meta">
					<span class="st-about-version">Version {ctl.status?.version || 'unknown'}</span>
					<span class="st-ink-2"
						>A keystroke, keyboard and mouse visualizer for SolidWorks tutorial recording.</span
					>
				</div>
			</div>
			<p class="st-sub">Fonts</p>
			<table class="st-lic">
				<tbody>
					{#each FONTS as f (f.name)}
						<tr>
							<th scope="row" style:font-family={`'${f.name}'`}>{f.name}</th>
							<td>{f.by}</td>
							<td>{f.use}</td>
							<td><span class="plate-chip">SIL Open Font License 1.1</span></td>
						</tr>
					{/each}
				</tbody>
			</table>
			<p class="st-sub">Built with</p>
			<table class="st-lic">
				<tbody>
					{#each PARTS as p (p.name)}
						<tr>
							<th scope="row">{p.name}</th>
							<td colspan="2">Open source</td>
							<td><span class="plate-chip">{p.license}</span></td>
						</tr>
					{/each}
				</tbody>
			</table>
			<p class="st-about-note">
				The key sounds are synthesized by the app. The SolidWorks captions are quoted from the
				published help pages listed on the SolidWorks page, each with its address.
			</p>
		</div>
	</Card>
</div>

<style>
	.st-stack {
		display: flex;
		flex-direction: column;
		gap: 1.6rem;
	}
	.st-about {
		display: flex;
		flex-direction: column;
		gap: 0.6rem;
		padding: 1rem 0 1.1rem;
	}
	.st-about-head {
		display: flex;
		align-items: center;
		gap: 1.25rem;
		flex-wrap: wrap;
	}
	.st-about-meta {
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
	}
	.st-about-version {
		font-family: var(--font-mono);
		letter-spacing: 0.06em;
	}
	.st-sub {
		margin: 0.6rem 0 0;
		font-weight: 700;
		font-size: 1.05rem;
	}
	.st-lic {
		width: 100%;
		border-collapse: collapse;
	}
	.st-lic th,
	.st-lic td {
		text-align: left;
		padding: 0.5rem 0.6rem 0.5rem 0;
		vertical-align: middle;
	}
	.st-lic tr + tr {
		box-shadow:
			inset 0 1px 0 0 var(--plate-groove-dk),
			inset 0 2px 0 0 var(--plate-groove-lt);
	}
	.st-lic th {
		font-weight: 600;
		width: 26%;
	}
	.st-lic td {
		color: var(--plate-ink-2);
	}
	.st-lic td:last-child {
		text-align: right;
	}
	.st-about-note {
		margin: 0.5rem 0 0;
		color: var(--plate-ink-2);
		font-size: 0.95rem;
	}
</style>
