<!--
	Keyboard: the keyboard visual that lights each physical key you press.
-->
<script lang="ts">
	import { LIMITS } from '../../lib/protocol';
	import { useController } from '../state.svelte';
	import { offsetWords } from '../lib/anchors';
	import { acceleratorParts } from '../lib/accelerator';
	import { formatMs } from '../lib/format';
	import PageTitle from '../ui/PageTitle.svelte';
	import Card from '../ui/Card.svelte';
	import Row from '../ui/Row.svelte';
	import Switch from '../ui/Switch.svelte';
	import Segmented from '../ui/Segmented.svelte';
	import Slider from '../ui/Slider.svelte';
	import AnchorPicker from '../ui/AnchorPicker.svelte';
	import NumberField from '../ui/NumberField.svelte';
	import KeyCombo from '../ui/KeyCombo.svelte';
	import Notice from '../ui/Notice.svelte';
	import LayoutPicker from '../parts/LayoutPicker.svelte';

	const ctl = useController();
	const s = $derived(ctl.settings!);
	const k = $derived(s.overlay.keyboard);
	const offsets = $derived(offsetWords(k.anchor));
	const hiddenByHotkey = $derived(ctl.status ? !ctl.status.keyboard_visible : false);
</script>

<PageTitle
	title="Keyboard"
	lead="A keyboard drawing that lights each key you press, so viewers see where your fingers are."
/>

<div class="st-stack">
	<Card title="Keyboard visual">
		<Row
			label="Show the keyboard"
			help="In the native overlay and the Everything source. The Keyboard source always shows it."
		>
			<Switch bind:checked={k.enabled} label="Show the keyboard" onWord="Shown" offWord="Hidden" />
		</Row>
		{#if hiddenByHotkey}
			<Notice tone="info" lead="Hidden by the hotkey">
				The keyboard visual is hidden everywhere right now. Press
				{acceleratorParts(s.hotkeys.toggle_keyboard).join(' + ')} or use Home to show it again.
			</Notice>
		{/if}
		<Row
			label="Mouse beside it"
			help="A small mouse diagram next to the keyboard lights its buttons and wheel."
		>
			<Switch bind:checked={k.mouse} label="Mouse beside the keyboard" />
		</Row>
		<Row label="Hotkey" help="Shows or hides the keyboard visual at any time.">
			<KeyCombo parts={acceleratorParts(s.hotkeys.toggle_keyboard)} size="s" />
		</Row>
	</Card>

	<Card
		title="Layout"
		lead="Pick the shape closest to your keyboard. The left-hand zone keeps the most of your model in view."
	>
		<LayoutPicker config={s.overlay} value={k.layout} onchoose={(l) => (k.layout = l)} />
	</Card>

	<Card title="Size and timing">
		<Row label="Size" help="Key size: S 28, M 38, L 50 pixels at 1080p.">
			<Segmented
				bind:value={k.size}
				options={[
					{ value: 's', label: 'S', hint: 'Small' },
					{ value: 'm', label: 'M', hint: 'Medium' },
					{ value: 'l', label: 'L', hint: 'Large' },
				]}
				label="Keyboard size"
			/>
		</Row>
		<Row
			label="Fade after release"
			help="How long a released key keeps glowing. 0 turns it off at once."
		>
			<Slider
				bind:value={k.decay_ms}
				min={LIMITS.decayMs.min}
				max={LIMITS.decayMs.max}
				step={50}
				label="Fade after release"
				format={formatMs}
			/>
		</Row>
	</Card>

	<Card title="Position">
		<Row
			label="Place on screen"
			help="The corner or edge the keyboard keeps to. The bottom left stays clear of most model views."
		>
			<AnchorPicker bind:value={k.anchor} label="Where the keyboard sits" />
		</Row>
		<Row label={offsets.x} help="Pixels at 1080p; they grow with the overall scale." for="st-kb-x">
			<NumberField
				id="st-kb-x"
				bind:value={k.offset_x}
				min={LIMITS.offset.min}
				max={LIMITS.offset.max}
				step={4}
				unit="px"
				label={offsets.x}
			/>
		</Row>
		<Row label={offsets.y} for="st-kb-y">
			<NumberField
				id="st-kb-y"
				bind:value={k.offset_y}
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
</style>
