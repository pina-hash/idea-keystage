<!--
	Mouse: the cursor highlight and its effects, mouse callouts, and the
	stand-alone mouse diagram.
-->
<script lang="ts">
	import { LIMITS } from '../../lib/protocol';
	import { useController } from '../state.svelte';
	import { offsetWords } from '../lib/anchors';
	import { formatPercent } from '../lib/format';
	import PageTitle from '../ui/PageTitle.svelte';
	import Card from '../ui/Card.svelte';
	import Row from '../ui/Row.svelte';
	import Switch from '../ui/Switch.svelte';
	import Slider from '../ui/Slider.svelte';
	import ColorField from '../ui/ColorField.svelte';
	import AnchorPicker from '../ui/AnchorPicker.svelte';
	import NumberField from '../ui/NumberField.svelte';
	import CursorStylePicker from '../parts/CursorStylePicker.svelte';

	const ctl = useController();
	const s = $derived(ctl.settings!);
	const cu = $derived(s.overlay.cursor);
	const m = $derived(s.overlay.mouse);
	const offsets = $derived(offsetWords(m.anchor));

	const PRESETS = [
		{ name: 'Green', value: '#78b870' },
		{ name: 'Amber', value: '#d08030' },
		{ name: 'Teal', value: '#5abda8' },
		{ name: 'Gold', value: '#c8a848' },
		{ name: 'White', value: '#ffffff' },
	];
</script>

<PageTitle title="Mouse" lead="Help viewers follow the pointer, every click and every drag." />

<div class="st-stack">
	<Card title="Cursor highlight">
		<Row
			label="Highlight the cursor"
			help="Drawn by the overlay around your pointer. The Cursor source always shows it."
		>
			<Switch bind:checked={cu.enabled} label="Highlight the cursor" />
		</Row>
		<CursorStylePicker
			value={cu.style}
			color={cu.color}
			opacity={cu.opacity}
			onchoose={(v) => (cu.style = v)}
		/>
		<Row
			label="Size"
			help="The ring's width at 1080p. The spotlight's opening is about twice as wide."
		>
			<Slider
				bind:value={cu.size}
				min={LIMITS.cursorSize.min}
				max={LIMITS.cursorSize.max}
				step={2}
				label="Cursor highlight size"
				format={(v) => `${v} px`}
			/>
		</Row>
		<Row label="Color">
			<div class="st-color-row">
				<div class="st-presets" role="group" aria-label="Preset colors">
					{#each PRESETS as p (p.value)}
						<button
							type="button"
							class="plate-key st-preset"
							aria-pressed={cu.color.toLowerCase() === p.value}
							onclick={() => (cu.color = p.value)}
						>
							<span class="st-preset-swatch" style:background={p.value} aria-hidden="true"></span>
							{p.name}
						</button>
					{/each}
				</div>
				<ColorField bind:value={cu.color} label="Cursor color" />
			</div>
		</Row>
		<Row label="Opacity">
			<Slider
				bind:value={cu.opacity}
				min={0.1}
				max={1}
				step={0.05}
				label="Cursor highlight opacity"
				format={formatPercent}
			/>
		</Row>
		<Row
			label="Click ripples"
			help="Each button has its own shape: one ring for left, two squares for right, a pulse for middle."
		>
			<Switch bind:checked={cu.ripples} label="Click ripples" />
		</Row>
		<Row label="Drag trail" help="A fading line behind the pointer while a button is held.">
			<Switch bind:checked={cu.trail} label="Drag trail" />
		</Row>
		<Row label="Wheel indicator" help="Chevrons at the pointer when you scroll.">
			<Switch bind:checked={cu.wheel} label="Wheel indicator" />
		</Row>
	</Card>

	<Card title="Mouse callouts">
		<Row
			label="Mouse callouts outside SolidWorks"
			help="Callouts for drags, right and middle clicks and the wheel in other apps. The SolidWorks profile always has them; turn its captions off on the SolidWorks page."
		>
			<Switch bind:checked={s.display.mouse_callouts} label="Mouse callouts outside SolidWorks" />
		</Row>
		<Row
			label="Plain left clicks"
			help="Also show left clicks and drags with no key held. Off by default, because every selection would make a callout."
		>
			<Switch bind:checked={s.display.plain_left_clicks} label="Plain left clicks" />
		</Row>
	</Card>

	<Card title="Mouse diagram">
		<Row
			label="Show the mouse diagram"
			help="A mouse drawing on its own, apart from the keyboard. The Mouse source always shows it."
		>
			<Switch
				bind:checked={m.enabled}
				label="Show the mouse diagram"
				onWord="Shown"
				offWord="Hidden"
			/>
		</Row>
		<Row label="Place on screen" help="The corner or edge the mouse diagram keeps to.">
			<AnchorPicker bind:value={m.anchor} label="Where the mouse diagram sits" />
		</Row>
		<Row label={offsets.x} help="Pixels at 1080p; they grow with the overall scale." for="st-ms-x">
			<NumberField
				id="st-ms-x"
				bind:value={m.offset_x}
				min={LIMITS.offset.min}
				max={LIMITS.offset.max}
				step={4}
				unit="px"
				label={offsets.x}
			/>
		</Row>
		<Row label={offsets.y} for="st-ms-y">
			<NumberField
				id="st-ms-y"
				bind:value={m.offset_y}
				min={LIMITS.offset.min}
				max={LIMITS.offset.max}
				step={4}
				unit="px"
				label={offsets.y}
			/>
		</Row>
	</Card>
</div>

<style>
	.st-stack {
		display: flex;
		flex-direction: column;
		gap: 1.6rem;
	}
	.st-color-row {
		display: flex;
		flex-direction: column;
		align-items: flex-end;
		gap: 0.6rem;
	}
	.st-presets {
		display: flex;
		flex-wrap: wrap;
		justify-content: flex-end;
		gap: 0.4rem;
	}
	:global(.ks-plate) .st-preset {
		padding: 0 0.7rem 0 0.55rem;
		font-size: 0.92rem;
		gap: 0.45rem;
	}
	.st-preset-swatch {
		width: 16px;
		height: 16px;
		border-radius: 4px;
		box-shadow:
			inset 0 0 0 1px rgba(0, 0, 0, 0.4),
			0 0 0 1px rgba(255, 255, 255, 0.15);
	}
</style>
