<!--
	Sound: optional key sounds, made by the app itself (no recordings), off by
	default.
-->
<script lang="ts">
	import { useController } from '../state.svelte';
	import { SOUND_PACKS } from '../defaults';
	import { formatPercent } from '../lib/format';
	import PageTitle from '../ui/PageTitle.svelte';
	import Card from '../ui/Card.svelte';
	import Row from '../ui/Row.svelte';
	import Switch from '../ui/Switch.svelte';
	import Slider from '../ui/Slider.svelte';
	import Key from '../ui/Key.svelte';
	import Notice from '../ui/Notice.svelte';
	import Icon from '../ui/Icon.svelte';

	const ctl = useController();
	const s = $derived(ctl.settings!);
	let playing = $state<string | null>(null);
	let packs: HTMLButtonElement[] = $state([]);

	async function preview(id: string) {
		playing = id;
		await ctl.run(() => ctl.ipc.playSoundPreview(id));
		setTimeout(() => {
			if (playing === id) playing = null;
		}, 600);
	}

	function onkeydown(e: KeyboardEvent, index: number) {
		let next = -1;
		if (e.key === 'ArrowRight' || e.key === 'ArrowDown') next = (index + 1) % SOUND_PACKS.length;
		else if (e.key === 'ArrowLeft' || e.key === 'ArrowUp')
			next = (index - 1 + SOUND_PACKS.length) % SOUND_PACKS.length;
		if (next < 0) return;
		e.preventDefault();
		const p = SOUND_PACKS[next];
		if (!p) return;
		s.sound.pack = p.id;
		packs[next]?.focus();
	}
</script>

<PageTitle
	title="Sound"
	lead="Optional key sounds, made by the app itself. Off unless you turn them on."
/>

<div class="st-stack">
	<Card title="Key sounds">
		<Row
			label="Play key sounds"
			help="A short sound for each key you press, never while paused or in a password field."
		>
			<Switch bind:checked={s.sound.enabled} label="Play key sounds" />
		</Row>
		<Row label="Volume" dim={!s.sound.enabled}>
			<Slider
				bind:value={s.sound.volume}
				min={0}
				max={1}
				step={0.05}
				label="Volume"
				format={formatPercent}
				disabled={!s.sound.enabled}
			/>
		</Row>
		{#if s.sound.enabled}
			<Notice tone="info" lead="In your recording">
				The sounds play on this computer's speakers. If OBS captures desktop audio, it records them
				too.
			</Notice>
			<div class="st-gap"></div>
		{/if}
	</Card>

	<Card title="Sound pack" lead="Each pack is synthesized from scratch. Press Play to hear one.">
		<div class="st-packs" role="radiogroup" aria-label="Sound pack">
			{#each SOUND_PACKS as p, i (p.id)}
				{@const on = s.sound.pack === p.id}
				<div class="st-pack" class:is-on={on}>
					<button
						bind:this={packs[i]}
						type="button"
						role="radio"
						aria-checked={on}
						tabindex={on ? 0 : -1}
						class="plate-key st-pack-pick"
						onclick={() => (s.sound.pack = p.id)}
						onkeydown={(e) => onkeydown(e, i)}
					>
						<span class="st-pack-name">
							{p.name}
							{#if on}<span class="st-pack-on"><Icon name="check" size={15} />In use</span>{/if}
						</span>
						<span class="st-pack-desc">{p.description}</span>
					</button>
					<Key
						icon="play"
						iconFilled
						label="Play {p.name}"
						busy={playing === p.id}
						onclick={() => void preview(p.id)}
					/>
				</div>
			{/each}
		</div>
	</Card>
</div>

<style>
	.st-stack {
		display: flex;
		flex-direction: column;
		gap: 1.6rem;
	}
	.st-gap {
		height: 0.9rem;
	}
	.st-packs {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(250px, 1fr));
		gap: 0.75rem;
		padding: 1rem 0;
	}
	.st-pack {
		display: flex;
		align-items: stretch;
		gap: 0.4rem;
	}
	:global(.ks-plate) .st-pack-pick {
		flex: 1 1 auto;
		flex-direction: column;
		align-items: flex-start;
		justify-content: center;
		gap: 0.15rem;
		padding: 0.55rem 0.8rem;
		text-align: left;
		font-weight: 500;
		letter-spacing: 0;
		white-space: normal;
	}
	.st-pack-name {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.5rem;
		width: 100%;
		font-weight: 700;
		font-size: 1.04rem;
		color: var(--plate-ink);
	}
	.st-pack-on {
		display: inline-flex;
		align-items: center;
		gap: 0.2rem;
		font-family: var(--font-mono);
		font-size: 0.68rem;
		letter-spacing: 0.1em;
		text-transform: uppercase;
		color: var(--plate-mark-ink);
		font-weight: 400;
	}
	.st-pack-desc {
		font-size: 0.88rem;
		color: var(--plate-ink-2);
		line-height: 1.3;
	}
	.st-pack :global(.st-key.is-icon) {
		align-self: stretch;
		height: auto;
	}
</style>
