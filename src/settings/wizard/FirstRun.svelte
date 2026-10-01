<!--
	First run (settings.first_run_done is false): monitor, then look (theme
	and keyboard), then output (OBS sources or the native overlay). Every
	choice is saved as it is made and shows in the preview at once; Finish
	writes first_run_done.
-->
<script lang="ts">
	import { useController } from '../state.svelte';
	import { captionFor } from '../lib/shortcuts';
	import { chosenMonitor, displayLabel } from '../lib/monitors';
	import { sourceCards } from '../lib/obs';
	import { THEMES } from '../../lib/themes/themes';
	import { LAYOUT_INFO } from '../lib/layouts';
	import {
		OUTPUT_INFO,
		WIZARD_STEPS,
		applyOutput,
		outputOf,
		stepIndex,
		type OutputChoice,
		type WizardStepId,
	} from '../lib/wizard';
	import { DEFAULT_CUSTOM } from '../../lib/themes/themes';
	import Mark from '../parts/Mark.svelte';
	import MonitorMap from '../parts/MonitorMap.svelte';
	import ThemePicker from '../parts/ThemePicker.svelte';
	import LayoutPicker from '../parts/LayoutPicker.svelte';
	import SourceCard from '../parts/SourceCard.svelte';
	import Key from '../ui/Key.svelte';
	import Row from '../ui/Row.svelte';
	import Switch from '../ui/Switch.svelte';
	import Notice from '../ui/Notice.svelte';
	import Icon from '../ui/Icon.svelte';

	const ctl = useController();
	const s = $derived(ctl.settings!);
	let step = $state<WizardStepId>('monitor');
	const index = $derived(stepIndex(step));
	const output = $derived(outputOf(s));
	const monitor = $derived(chosenMonitor(ctl.monitors, s.host.monitor));
	const cards = $derived(sourceCards(ctl.urls, s.server.port, s.overlay, monitor));
	const caption = $derived(captionFor(ctl.profile, ['Ctrl', 'S'])?.caption ?? null);
	let heading = $state<HTMLElement | null>(null);

	function go(to: WizardStepId) {
		step = to;
		queueMicrotask(() => heading?.focus());
	}

	function next() {
		const n = WIZARD_STEPS[index + 1];
		if (n) go(n.id);
	}

	function back() {
		const p = WIZARD_STEPS[index - 1];
		if (p) go(p.id);
	}

	async function finish() {
		s.first_run_done = true;
		await ctl.flush();
		ctl.go('home');
		ctl.toast('Setup is done. Everything can be changed here at any time.', 'ok');
	}

	function chooseTheme(theme: string) {
		s.overlay.theme = theme;
		if (theme === 'custom' && !s.overlay.custom) s.overlay.custom = { ...DEFAULT_CUSTOM };
	}

	const OUTPUTS: OutputChoice[] = ['obs', 'native', 'both'];
	let outputKeys: HTMLButtonElement[] = $state([]);

	function outputKey(e: KeyboardEvent, i: number) {
		let n = -1;
		if (e.key === 'ArrowRight' || e.key === 'ArrowDown') n = (i + 1) % OUTPUTS.length;
		else if (e.key === 'ArrowLeft' || e.key === 'ArrowUp')
			n = (i - 1 + OUTPUTS.length) % OUTPUTS.length;
		if (n < 0) return;
		e.preventDefault();
		const o = OUTPUTS[n];
		if (!o) return;
		applyOutput(s, o);
		outputKeys[n]?.focus();
	}

	const copy = (text: string) => ctl.ipc.copyText(text);
	const themeName = $derived(THEMES.find((t) => t.id === s.overlay.theme)?.name ?? s.overlay.theme);
</script>

<div class="st-wizard">
	<header class="st-wiz-head">
		<Mark />
		<ol class="st-wiz-steps" aria-label="Setup steps">
			{#each WIZARD_STEPS as st, i (st.id)}
				<li>
					<button
						type="button"
						class="st-pad st-wiz-pad"
						aria-current={st.id === step ? 'step' : undefined}
						disabled={i > index + 1}
						onclick={() => go(st.id)}
					>
						<span class="st-wiz-num">{i + 1}</span>
						{st.title}
						{#if i < index}<Icon name="check" size={16} />{/if}
					</button>
				</li>
			{/each}
		</ol>
	</header>

	<section class="plate-housing st-wiz-body" aria-labelledby="st-wiz-title">
		{#if step === 'monitor'}
			<h1 class="st-title" id="st-wiz-title" tabindex="-1" bind:this={heading}>
				Welcome to IDEA Keystage
			</h1>
			<p class="st-wiz-lead">
				Three quick choices and your keystrokes are ready to record. First: which monitor do you
				record?
			</p>
			{#if ctl.monitors.length > 0}
				<MonitorMap
					monitors={ctl.monitors}
					value={s.host.monitor}
					onchoose={(id) => (s.host.monitor = id)}
				/>
				{#if monitor}
					<p class="st-wiz-note">
						The overlay will cover {displayLabel(monitor, ctl.monitors.indexOf(monitor))}, {monitor.width}
						x
						{monitor.height}.
					</p>
				{/if}
			{:else}
				<Notice tone="info" lead="No monitors reported">
					Windows did not list any monitors just now. The overlay will use the primary monitor; you
					can pick another on the Display page later.
				</Notice>
			{/if}
		{:else if step === 'look'}
			<h1 class="st-title" id="st-wiz-title" tabindex="-1" bind:this={heading}>Pick a look</h1>
			<p class="st-wiz-lead">
				The theme colors the callouts. The preview on the right shows it at once.
			</p>
			<ThemePicker config={s.overlay} {caption} onchoose={chooseTheme} />
			<Row label="Keyboard visual" help="A keyboard drawing that lights the keys you press.">
				<Switch
					bind:checked={s.overlay.keyboard.enabled}
					label="Keyboard visual"
					onWord="Shown"
					offWord="Hidden"
				/>
			</Row>
			{#if s.overlay.keyboard.enabled}
				<LayoutPicker
					config={s.overlay}
					value={s.overlay.keyboard.layout}
					onchoose={(l) => (s.overlay.keyboard.layout = l)}
				/>
			{/if}
		{:else if step === 'output'}
			<h1 class="st-title" id="st-wiz-title" tabindex="-1" bind:this={heading}>
				How do you record?
			</h1>
			<p class="st-wiz-lead">
				Choose where the overlay is drawn. You can change it on the Display and OBS pages.
			</p>
			<div class="st-outputs" role="radiogroup" aria-label="Where the overlay is drawn">
				{#each OUTPUTS as o, i (o)}
					{@const on = output === o}
					<button
						bind:this={outputKeys[i]}
						type="button"
						role="radio"
						aria-checked={on}
						tabindex={on ? 0 : -1}
						class="plate-key st-card-pick st-output"
						onclick={() => applyOutput(s, o)}
						onkeydown={(e) => outputKey(e, i)}
					>
						<span class="st-output-icon"
							><Icon name={o === 'native' ? 'display' : 'broadcast'} size={26} /></span
						>
						<span class="st-output-title">
							{OUTPUT_INFO[o].title}
							{#if on}<span class="st-output-on"><Icon name="check" size={15} />Chosen</span>{/if}
						</span>
						<span class="st-output-desc">{OUTPUT_INFO[o].description}</span>
					</button>
				{/each}
			</div>
			{#if output !== 'native'}
				<p class="st-wiz-sub">Add these to OBS as Browser Sources</p>
				<div class="st-wiz-sources">
					{#each cards.filter((c) => c.route === 'keys' || c.route === 'all') as card (card.route)}
						<SourceCard {card} {copy} />
					{/each}
				</div>
				<p class="st-wiz-note">More sources and the exact OBS fields are on the OBS page.</p>
			{/if}
		{:else}
			<h1 class="st-title" id="st-wiz-title" tabindex="-1" bind:this={heading}>Ready</h1>
			<p class="st-wiz-lead">Here is what you chose. Press Finish, then start recording.</p>
			<dl class="st-summary">
				<dt>Monitor</dt>
				<dd>
					{monitor
						? `${displayLabel(monitor, ctl.monitors.indexOf(monitor))}, ${monitor.width} x ${monitor.height}`
						: 'Primary monitor'}
				</dd>
				<dt>Theme</dt>
				<dd>{themeName}</dd>
				<dt>Keyboard</dt>
				<dd>
					{s.overlay.keyboard.enabled ? LAYOUT_INFO[s.overlay.keyboard.layout].name : 'Hidden'}
				</dd>
				<dt>Output</dt>
				<dd>{OUTPUT_INFO[output].title}</dd>
			</dl>
			<Notice tone="info" lead="Privacy">
				Typing outside SolidWorks shows only as "Typing", password fields are always hidden, and
				nothing you type is saved unless you start a session log.
			</Notice>
		{/if}

		<footer class="st-wiz-foot">
			{#if index > 0}
				<Key icon="arrowLeft" onclick={back}>Back</Key>
			{:else}
				<span></span>
			{/if}
			{#if step === 'done'}
				<Key variant="primary" icon="check" onclick={finish}>Finish</Key>
			{:else}
				<Key variant="primary" icon="arrowRight" onclick={next}>Next</Key>
			{/if}
		</footer>
	</section>
</div>

<style>
	.st-wizard {
		display: flex;
		flex-direction: column;
		gap: 1.25rem;
		/* One width for every step, so the card does not jump between them. */
		box-sizing: border-box;
		width: 100%;
		max-width: 980px;
		margin: 0 auto;
		padding: 1.4rem 2.25rem 2.5rem;
	}
	.st-wiz-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1.5rem;
		flex-wrap: wrap;
	}
	.st-wiz-steps {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		gap: 0.6rem;
		flex-wrap: wrap;
	}
	:global(.ks-plate) .st-wiz-pad {
		width: auto;
		padding-right: calc(0.9rem + 9px);
	}
	:global(.ks-plate) .st-wiz-pad[aria-current='step'] {
		background:
			linear-gradient(to bottom, var(--plate-lit-top), var(--plate-lit-bot)) padding-box,
			linear-gradient(to bottom, var(--plate-band-top), var(--plate-band-bot)) border-box;
		box-shadow:
			0 0 0 1px var(--plate-hair),
			inset 0 0 0 1px var(--plate-lit-glow),
			inset 0 0 0 3px var(--plate-lit-glow),
			inset 0 0 6px 4px var(--plate-lit-core),
			0 0 10px 1px var(--plate-bloom);
		font-weight: 700;
	}
	:global(.ks-plate) .st-wiz-pad[aria-current='step']::after {
		background:
			radial-gradient(
				circle,
				var(--plate-accent-glow) 0 0.5px,
				var(--plate-accent) 1.5px,
				var(--plate-led-ring) 2.1px 2.4px,
				transparent 2.8px
			),
			radial-gradient(circle at 50% 30%, var(--plate-cup-in) 0 2.1px, var(--plate-cup) 3.5px);
	}
	.st-wiz-num {
		font-family: var(--font-title);
		font-weight: 700;
		color: var(--plate-accent);
	}
	.st-wiz-body {
		display: flex;
		flex-direction: column;
		gap: 0.9rem;
		padding: 1.4rem 1.75rem 1.5rem;
	}
	.st-wiz-body h1:focus {
		outline: none;
	}
	.st-wiz-lead {
		margin: 0;
		text-align: center;
		color: var(--plate-ink-2);
		font-size: 1.05rem;
	}
	.st-wiz-note {
		margin: 0;
		color: var(--plate-ink-2);
		text-align: center;
	}
	.st-wiz-sub {
		margin: 0.4rem 0 0;
		font-weight: 700;
		font-size: 1.05rem;
	}
	.st-wiz-sources {
		display: flex;
		flex-direction: column;
		gap: 0.7rem;
	}
	.st-outputs {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: 0.85rem;
		padding: 0.4rem 0;
	}
	:global(.ks-plate) .st-output {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		justify-content: flex-start;
		gap: 0.35rem;
		padding: 1rem 1rem 1.05rem;
		text-align: left;
		white-space: normal;
		font-weight: 500;
		letter-spacing: 0;
	}
	.st-output-icon {
		color: var(--plate-accent);
	}
	.st-output-title {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.5rem;
		width: 100%;
		font-weight: 700;
		font-size: 1.1rem;
		color: var(--plate-ink);
	}
	.st-output-on {
		display: inline-flex;
		align-items: center;
		gap: 0.2rem;
		font-family: var(--font-mono);
		font-size: 0.7rem;
		letter-spacing: 0.1em;
		text-transform: uppercase;
		color: var(--plate-mark-ink);
		font-weight: 400;
	}
	.st-output-desc {
		font-size: 0.93rem;
		line-height: 1.35;
		color: var(--plate-ink-2);
	}
	.st-summary {
		display: grid;
		grid-template-columns: 9rem minmax(0, 1fr);
		gap: 0.55rem 1rem;
		margin: 0.4rem 0;
		padding: 1rem 1.2rem;
		border-radius: var(--plate-r-control);
		background: linear-gradient(to bottom, var(--plate-chip-top), var(--plate-chip-bot));
		box-shadow:
			inset 0 0 0 1px var(--plate-chip-edge),
			inset 0 3px 4px -2px var(--plate-chip-shade);
	}
	.st-summary dt {
		font-family: var(--font-mono);
		font-size: var(--plate-label-size);
		letter-spacing: var(--plate-label-track);
		text-transform: uppercase;
		color: var(--plate-caption);
		align-self: center;
	}
	.st-summary dd {
		margin: 0;
		font-weight: 600;
		font-size: 1.05rem;
	}
	.st-wiz-foot {
		display: flex;
		justify-content: space-between;
		gap: 0.75rem;
		padding-top: 0.9rem;
		box-shadow:
			inset 0 1px 0 0 var(--plate-groove-dk),
			inset 0 2px 0 0 var(--plate-groove-lt);
	}
</style>
