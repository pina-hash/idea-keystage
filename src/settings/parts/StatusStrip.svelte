<!--
	Always at hand under the preview: a display with the recording and
	session readouts, Pause or Resume, and the session log keys.
-->
<script lang="ts">
	import { useController } from '../state.svelte';
	import { activityWord, obsWord, recordingText } from '../lib/nav';
	import { elapsedSince, formatClock, plural } from '../lib/format';
	import Readout from '../ui/Readout.svelte';
	import Key from '../ui/Key.svelte';
	import type { Status } from '../types';

	const ctl = useController();
	const status = $derived(ctl.status);
	const activity = $derived(status ? activityWord(status) : null);
	const rec = $derived(status ? recordingText(status) : null);
	const session = $derived(status?.session?.active ? status.session : null);
	const obsLine = $derived.by(() => {
		if (!status) return '';
		if (rec) return rec;
		if (!status.obs.enabled) return 'OBS link off';
		switch (status.obs.state) {
			case 'connected':
				return 'Not recording';
			case 'connecting':
				return 'Connecting to OBS';
			case 'auth_failed':
				return 'OBS sign-in failed';
			default:
				return 'OBS not connected';
		}
	});
	/** Problem lines glow red or amber; the words already say what is wrong. */
	const obsTone = $derived.by(() => {
		if (!status || rec || !status.obs.enabled) return 'ok';
		const t = obsWord(status).tone;
		return t === 'danger' || t === 'warn' ? t : 'ok';
	});
	const activityTone = $derived(activity?.tone === 'warn' ? 'warn' : 'ok');
	let busy = $state<string | null>(null);

	async function act(name: string, fn: () => Promise<Status>) {
		busy = name;
		const s = await ctl.run(fn);
		if (s) ctl.setStatus(s);
		busy = null;
	}
</script>

{#if status}
	<div class="st-strip">
		<Readout live label="Recording and session">
			<div class="st-strip-lines">
				<div class="st-strip-line is-{obsTone}">
					<span class="st-rec-lamp" class:is-on={!!rec} aria-hidden="true"></span>
					<span class="st-strip-big">{obsLine}</span>
				</div>
				<div class="st-strip-line st-strip-small">
					{#if session}
						Session {formatClock(elapsedSince(session.started, ctl.now))}
						<span aria-hidden="true">/</span>
						{plural(session.callouts, 'callout')}
						<span aria-hidden="true">/</span>
						{plural(session.markers, 'marker')}
					{:else}
						No session log running
					{/if}
				</div>
				<div class="st-strip-line st-strip-small is-{activityTone}">
					{activity?.word ?? ''}
					<span aria-hidden="true">/</span>
					{status.profile ? `${status.profile.name} profile` : 'General profile'}
				</div>
			</div>
		</Readout>
		<div class="st-strip-keys">
			<Key
				variant={status.paused ? 'primary' : 'plain'}
				icon={status.paused ? 'play' : 'pause'}
				iconFilled={status.paused}
				busy={busy === 'pause'}
				onclick={() => act('pause', () => ctl.ipc.setPaused(!status.paused))}
			>
				{status.paused ? 'Resume' : 'Pause'}
			</Key>
			<Key
				icon={session ? 'stop' : 'record'}
				iconFilled
				busy={busy === 'session'}
				onclick={() =>
					act('session', () => (session ? ctl.ipc.sessionStop() : ctl.ipc.sessionStart()))}
			>
				{session ? 'Stop log' : 'Start log'}
			</Key>
			<Key
				icon="flag"
				disabled={!session}
				busy={busy === 'marker'}
				title={session ? 'Add a chapter marker now' : 'Start a session log to add markers'}
				onclick={() => act('marker', () => ctl.ipc.sessionMarker(null))}
			>
				Marker
			</Key>
		</div>
	</div>
{/if}

<style>
	.st-strip {
		display: flex;
		flex-direction: column;
		gap: 0.7rem;
		margin-top: auto;
		padding-top: 0.4rem;
	}
	.st-strip-lines {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
	}
	.st-strip-line {
		display: flex;
		align-items: center;
		gap: 0.55rem;
		flex-wrap: wrap;
	}
	/* Same inks as the Readout tones, measured on the screen in style.test.ts. */
	.st-strip-line.is-warn {
		color: #f0b36a;
		text-shadow: 0 0 8px rgba(240, 179, 106, 0.5);
	}
	.st-strip-line.is-danger {
		color: #f39a9a;
		text-shadow: 0 0 8px rgba(243, 154, 154, 0.5);
	}
	.st-strip-line.is-warn .st-rec-lamp,
	.st-strip-line.is-danger .st-rec-lamp {
		box-shadow: inset 0 0 0 2px currentColor;
	}
	.st-strip-big {
		font-size: 1.25rem;
		letter-spacing: 0.08em;
		text-transform: uppercase;
	}
	.st-strip-small {
		font-size: 0.82rem;
		letter-spacing: 0.08em;
		text-transform: uppercase;
	}
	.st-rec-lamp {
		width: 11px;
		height: 11px;
		border-radius: 50%;
		background: transparent;
		box-shadow: inset 0 0 0 2px var(--plate-screen-ink);
	}
	.st-rec-lamp.is-on {
		background: radial-gradient(circle at 42% 36%, #ffd9d9 0 1px, #e05555 3px);
		box-shadow: 0 0 8px 2px rgba(224, 85, 85, 0.55);
	}
	.st-strip-keys {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: 0.5rem;
	}
	@media (prefers-reduced-motion: no-preference) {
		.st-rec-lamp.is-on {
			animation: st-rec 1.6s ease-in-out infinite;
		}
	}
	@keyframes st-rec {
		50% {
			opacity: 0.45;
		}
	}
</style>
