<!--
	A hotkey capture field: a raised key showing the shortcut. Press it, then
	press the new shortcut; Escape or leaving the field cancels. While it
	waits, the app's own hotkeys are paused (hotkeys_capture) so the shortcut
	being typed reaches this window. A shortcut that cannot be used is
	explained and not saved.
-->
<script lang="ts">
	import type { HotkeyName, Hotkeys } from '../types';
	import {
		acceleratorParts,
		captureStep,
		formatAccelerator,
		heldModifiers,
		type ModName,
	} from '../lib/accelerator';
	import { blockingProblems, type HotkeyProblem } from '../lib/hotkeys';
	import KeyCombo from '../ui/KeyCombo.svelte';
	import Icon from '../ui/Icon.svelte';

	interface Props {
		name: HotkeyName;
		title: string;
		hotkeys: Hotkeys;
		onset: (value: string) => void;
		setCapture: (active: boolean) => void;
		describedby?: string;
	}

	let { name, title, hotkeys, onset, setCapture, describedby }: Props = $props();

	let capturing = $state(false);
	let held = $state<ModName[]>([]);
	let rejected = $state<HotkeyProblem[]>([]);
	let attempt = $state<string | null>(null);
	let button = $state<HTMLButtonElement | null>(null);

	function start() {
		capturing = true;
		held = [];
		rejected = [];
		attempt = null;
		setCapture(true);
	}

	function stop() {
		if (!capturing) return;
		capturing = false;
		held = [];
		setCapture(false);
	}

	function onkeydown(e: KeyboardEvent) {
		if (!capturing) {
			return;
		}
		e.preventDefault();
		e.stopPropagation();
		const step = captureStep(e);
		if (step.kind === 'modifiers') {
			held = step.mods;
			return;
		}
		if (step.kind === 'cancel') {
			stop();
			return;
		}
		if (step.kind === 'unknown') {
			rejected = [{ level: 'error', message: 'That key cannot be used in a hotkey.' }];
			return;
		}
		const text = formatAccelerator(step.accel);
		const problems = blockingProblems(name, step.accel, hotkeys);
		attempt = text;
		if (problems.length > 0) {
			rejected = problems;
			held = [];
			return;
		}
		rejected = [];
		stop();
		onset(text);
	}

	function onkeyup(e: KeyboardEvent) {
		if (!capturing) return;
		e.preventDefault();
		held = heldModifiers(e);
	}

	$effect(() => () => {
		if (capturing) setCapture(false);
	});
</script>

<div class="st-hk">
	<button
		bind:this={button}
		type="button"
		class="plate-key st-hk-key"
		class:is-capturing={capturing}
		aria-pressed={capturing}
		aria-label={capturing
			? `Press the new shortcut for ${title}. Escape cancels.`
			: `${title}: ${acceleratorParts(hotkeys[name]).join(' + ')}. Press to change.`}
		aria-describedby={describedby}
		onclick={() => (capturing ? stop() : start())}
		{onkeydown}
		{onkeyup}
		onblur={stop}
	>
		{#if capturing}
			<span class="st-hk-wait">
				{#if held.length > 0}
					<KeyCombo parts={[...held, '...']} size="s" />
				{:else}
					<Icon name="command" size={18} />Press the new shortcut
				{/if}
			</span>
		{:else}
			<KeyCombo parts={acceleratorParts(hotkeys[name])} size="s" />
			<span class="st-hk-change"><Icon name="pencil" size={16} />Change</span>
		{/if}
	</button>
	{#if capturing}
		<p class="st-hk-hint" aria-hidden="true">Escape cancels</p>
	{/if}
	{#if rejected.length > 0}
		<div class="st-hk-rejected" role="alert">
			{#each rejected as p, i (i)}
				<p>
					<Icon name="warning" size={16} />
					<span>{attempt ? `${acceleratorParts(attempt).join(' + ')}: ` : ''}{p.message}</span>
				</p>
			{/each}
		</div>
	{/if}
</div>

<style>
	.st-hk {
		display: flex;
		flex-direction: column;
		align-items: flex-end;
		gap: 0.4rem;
	}
	:global(.ks-plate) .st-hk-key {
		min-width: 15rem;
		justify-content: space-between;
		padding: 0 0.8rem 0 0.6rem;
		gap: 0.9rem;
	}
	.st-hk-change {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		font-family: var(--font-mono);
		font-size: 0.72rem;
		letter-spacing: 0.1em;
		text-transform: uppercase;
		color: var(--plate-ink-2);
	}
	.st-hk-wait {
		display: inline-flex;
		align-items: center;
		gap: 0.45rem;
		font-family: var(--font-mono);
		font-size: 0.8rem;
		letter-spacing: 0.06em;
		text-transform: uppercase;
	}
	.st-hk-hint {
		margin: 0;
		font-size: 0.9rem;
		color: var(--plate-ink-2);
	}
	.st-hk-rejected {
		max-width: 26rem;
	}
	.st-hk-rejected p {
		display: flex;
		align-items: flex-start;
		gap: 0.35rem;
		margin: 0;
		color: var(--plate-danger-ink);
		font-size: 0.93rem;
		line-height: 1.35;
		text-align: right;
	}
</style>
