<!--
	Home: what the app is doing now, Pause and Resume, and the SolidWorks mode.
-->
<script lang="ts">
	import type { SolidworksMode } from '../../lib/protocol';
	import { useController } from '../state.svelte';
	import { activityWord, obsWord, serverWord } from '../lib/nav';
	import { acceleratorParts } from '../lib/accelerator';
	import { elapsedSince, formatClock, formatTime, plural } from '../lib/format';
	import PageTitle from '../ui/PageTitle.svelte';
	import Card from '../ui/Card.svelte';
	import Row from '../ui/Row.svelte';
	import Key from '../ui/Key.svelte';
	import Segmented from '../ui/Segmented.svelte';
	import Switch from '../ui/Switch.svelte';
	import StatusTile from '../ui/StatusTile.svelte';
	import Notice from '../ui/Notice.svelte';
	import KeyCombo from '../ui/KeyCombo.svelte';
	import Readout from '../ui/Readout.svelte';

	const ctl = useController();
	const status = $derived(ctl.status);
	const settings = $derived(ctl.settings!);
	const activity = $derived(status ? activityWord(status) : null);
	const obs = $derived(status ? obsWord(status) : null);
	const server = $derived(status ? serverWord(status) : null);
	let pausing = $state(false);

	async function togglePause() {
		if (!status) return;
		pausing = true;
		await ctl.statusCommand(() => ctl.ipc.setPaused(!status.paused));
		pausing = false;
	}

	async function setMode(mode: SolidworksMode) {
		settings.profiles.solidworks_mode = mode;
		await ctl.statusCommand(() => ctl.ipc.setSolidworksMode(mode));
	}

	const MODE_HELP: Record<SolidworksMode, string> = {
		auto: 'SolidWorks captions turn on while SolidWorks is the app in front.',
		on: 'Captions and SolidWorks rules are used in every app.',
		off: 'SolidWorks captions are never shown. Every app uses the general rules.',
	};
</script>

<PageTitle title="Home" lead="What IDEA Keystage is doing right now." />

{#if status && activity}
	<div class="st-home-hero">
		<Readout
			live
			label="Status"
			tone={activity.tone === 'warn' ? 'warn' : activity.tone === 'danger' ? 'danger' : 'ok'}
		>
			<div class="st-hero-lines">
				<span class="st-hero-label">Status</span>
				<span class="st-hero-word">{activity.word}</span>
				<span class="st-hero-detail">{activity.detail}</span>
			</div>
		</Readout>
		<div class="st-hero-side">
			<Key
				size="l"
				variant={status.paused ? 'primary' : 'plain'}
				icon={status.paused ? 'play' : 'pause'}
				iconFilled={status.paused}
				busy={pausing}
				onclick={togglePause}
			>
				{status.paused ? 'Resume' : 'Pause'}
			</Key>
			<span class="st-hero-hotkey">
				<span class="plate-label">Hotkey</span>
				<KeyCombo parts={acceleratorParts(settings.hotkeys.pause)} size="s" />
			</span>
		</div>
	</div>

	<div class="st-notices">
		{#if !status.server.running}
			<Notice tone="danger" lead="OBS sources cannot connect" announce>
				{status.server.error ?? `The local server is not running on port ${status.server.port}.`}
				{#snippet actions()}
					<Key icon="arrowRight" onclick={() => ctl.go('obs')}>Choose another port</Key>
				{/snippet}
			</Notice>
		{/if}
		{#if status.obs.enabled && status.obs.state === 'auth_failed'}
			<Notice tone="danger" lead="OBS sign-in failed">
				{status.obs.error ?? 'OBS did not accept the password.'}
				{#snippet actions()}
					<Key icon="arrowRight" onclick={() => ctl.go('obs')}>Fix the OBS password</Key>
				{/snippet}
			</Notice>
		{/if}
		{#if status.hook.dropped > 0}
			<Notice tone="warn" lead="Some input was not shown">
				{plural(status.hook.dropped, 'event')} arrived faster than they could be shown and were skipped.
				This can happen while the computer is very busy.
			</Notice>
		{/if}
	</div>

	<div class="st-tiles">
		<StatusTile
			label="Profile"
			icon="cube"
			tone={status.profile ? 'ok' : 'off'}
			word={status.profile?.name ?? 'General'}
			detail={status.profile
				? 'Captions from the SolidWorks data.'
				: 'Shortcuts only. Typing is hidden as "Typing".'}
		/>
		{#if obs}
			<StatusTile
				label="OBS"
				icon="broadcast"
				tone={obs.tone}
				word={obs.word}
				detail={obs.detail}
			/>
		{/if}
		{#if server}
			<StatusTile
				label="Local server"
				icon="link"
				tone={server.tone}
				word={server.word}
				detail={server.detail}
			/>
		{/if}
		<StatusTile
			label="Input hooks"
			icon="keyboard"
			tone={status.hook.installed ? 'ok' : status.paused ? 'off' : 'warn'}
			word={status.hook.installed ? 'Installed' : 'Removed'}
			detail={status.hook.installed
				? 'Keyboard and mouse input reach the app.'
				: status.paused
					? 'Paused: the hooks are off until you resume.'
					: 'Waiting for Windows to accept the hooks.'}
		/>
		<StatusTile
			label="Dropped events"
			icon="warning"
			tone={status.hook.dropped > 0 ? 'warn' : 'ok'}
			word={status.hook.dropped.toLocaleString('en-US')}
			detail={status.hook.dropped > 0
				? 'Input skipped under heavy load.'
				: 'Nothing skipped. This should stay 0.'}
		/>
		<StatusTile
			label="Session log"
			icon="sessions"
			tone={status.session?.active ? 'danger' : 'off'}
			word={status.session?.active ? 'Recording' : 'Not running'}
			detail={status.session?.active
				? `Since ${formatTime(status.session.started)} (${formatClock(elapsedSince(status.session.started, ctl.now))}), ${plural(status.session.callouts, 'callout')}, ${plural(status.session.markers, 'marker')}.`
				: 'Start one on the Sessions page or with its hotkey.'}
		/>
	</div>
{/if}

<div class="st-stack">
	<Card title="SolidWorks mode" lead="Which rules and captions the overlay uses.">
		<Row label="Mode" help={MODE_HELP[settings.profiles.solidworks_mode]}>
			<Segmented
				value={settings.profiles.solidworks_mode}
				options={[
					{ value: 'auto', label: 'Auto', hint: 'Follow the app in front' },
					{ value: 'on', label: 'On', hint: 'Always SolidWorks' },
					{ value: 'off', label: 'Off', hint: 'Never SolidWorks' },
				]}
				label="SolidWorks mode"
				onchange={setMode}
			/>
		</Row>
	</Card>

	<Card title="Quick switches">
		<Row
			label="Keyboard visual"
			help="Shows or hides the keyboard everywhere until you switch it back. Same as its hotkey."
		>
			{#if status}
				<Switch
					checked={status.keyboard_visible}
					label="Keyboard visual"
					onWord="Shown"
					offWord="Hidden"
					onchange={(v) => void ctl.statusCommand(() => ctl.ipc.setKeyboardVisible(v))}
				/>
			{/if}
		</Row>
		<Row
			label="Native overlay"
			help="The transparent window on your screen. Turn it off when you only use OBS sources."
		>
			<Switch bind:checked={settings.host.native_overlay} label="Native overlay" />
		</Row>
	</Card>
</div>

<style>
	.st-home-hero {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto;
		gap: 1.25rem;
		align-items: stretch;
		margin-bottom: 1.1rem;
	}
	.st-hero-lines {
		display: flex;
		flex-direction: column;
		gap: 0.2rem;
		padding: 0.3rem 0.4rem 0.2rem;
	}
	.st-hero-label {
		font-size: 0.72rem;
		letter-spacing: 0.22em;
		text-transform: uppercase;
		opacity: 0.85;
	}
	.st-hero-word {
		font-family: var(--font-title);
		font-weight: 700;
		font-size: 2.3rem;
		letter-spacing: 0.12em;
		text-transform: uppercase;
		line-height: 1.1;
	}
	.st-hero-detail {
		font-size: 0.85rem;
		letter-spacing: 0.06em;
		text-transform: uppercase;
	}
	.st-hero-side {
		display: flex;
		flex-direction: column;
		justify-content: center;
		align-items: stretch;
		gap: 0.6rem;
		min-width: 13rem;
	}
	.st-hero-hotkey {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 0.5rem;
	}
	.st-notices {
		display: flex;
		flex-direction: column;
		gap: 0.6rem;
		margin-bottom: 1.1rem;
	}
	.st-notices:empty {
		display: none;
	}
	.st-tiles {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
		gap: 0.75rem;
		margin-bottom: 1.75rem;
	}
	.st-stack {
		display: flex;
		flex-direction: column;
		gap: 1.6rem;
	}
</style>
