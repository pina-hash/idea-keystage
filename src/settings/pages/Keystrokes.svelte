<!--
	Keystrokes: the callouts' look, place, size, timing and what they show.
-->
<script lang="ts">
	import { LIMITS, type Anchor } from '../../lib/protocol';
	import { customThemeVars, DEFAULT_CUSTOM } from '../../lib/themes/themes';
	import { useController } from '../state.svelte';
	import { captionFor } from '../lib/shortcuts';
	import { naturalDirection, offsetWords } from '../lib/anchors';
	import { formatMs } from '../lib/format';
	import { RULE_LIMITS } from '../defaults';
	import PageTitle from '../ui/PageTitle.svelte';
	import Card from '../ui/Card.svelte';
	import Row from '../ui/Row.svelte';
	import Switch from '../ui/Switch.svelte';
	import Segmented from '../ui/Segmented.svelte';
	import Slider from '../ui/Slider.svelte';
	import AnchorPicker from '../ui/AnchorPicker.svelte';
	import NumberField from '../ui/NumberField.svelte';
	import ColorField from '../ui/ColorField.svelte';
	import Notice from '../ui/Notice.svelte';
	import ThemePicker from '../parts/ThemePicker.svelte';

	const ctl = useController();
	const s = $derived(ctl.settings!);
	const c = $derived(s.overlay.callouts);
	const sampleCaption = $derived(captionFor(ctl.profile, ['Ctrl', 'S'])?.caption ?? null);
	const custom = $derived(s.overlay.theme === 'custom' ? customThemeVars(s.overlay.custom) : null);
	const offsets = $derived(offsetWords(c.anchor));

	function chooseTheme(theme: string) {
		s.overlay.theme = theme;
		if (theme === 'custom' && !s.overlay.custom) s.overlay.custom = { ...DEFAULT_CUSTOM };
	}

	function setAnchor(a: Anchor) {
		c.direction = naturalDirection(a);
	}
</script>

<PageTitle title="Keystrokes" lead="The callouts that show each shortcut as you press it." />

<div class="st-stack">
	<Card title="Callouts">
		<Row
			label="Show keystrokes"
			help="Turn off to show only the keyboard, the mouse or the cursor."
		>
			<Switch bind:checked={c.enabled} label="Show keystrokes" onWord="Shown" offWord="Hidden" />
		</Row>
		<Row
			label="Typing outside SolidWorks"
			help={s.display.typing === 'shortcuts_only'
				? 'Only shortcuts and special keys are shown. Plain typing appears as one "Typing" tag that never shows the letters.'
				: 'Everything you type is shown, except in password fields and excluded apps.'}
		>
			<Segmented
				bind:value={s.display.typing}
				options={[
					{ value: 'shortcuts_only', label: 'Shortcuts only', hint: 'Typing is hidden' },
					{ value: 'all', label: 'Show all typing', hint: 'Typed text is shown' },
				]}
				label="Typing outside SolidWorks"
			/>
		</Row>
		{#if s.display.typing === 'all'}
			<Notice tone="warn" lead="Typed text is visible">
				Everything you type in other apps will appear in your recording. Password fields and
				excluded apps are still hidden.
			</Notice>
		{/if}
		<Row
			label="Captions"
			help="The command name under a SolidWorks shortcut, from the SolidWorks data."
		>
			<Switch bind:checked={c.captions} label="Captions" />
		</Row>
		<Row
			label="Arrows and Enter"
			help="Draw arrows, Enter, Tab and Backspace as symbols, or spell them out."
		>
			<Segmented
				value={c.symbols ? 'symbols' : 'words'}
				options={[
					{ value: 'symbols', label: 'Symbols', hint: 'Arrows and a return mark' },
					{ value: 'words', label: 'Words', hint: 'Up, Down, Enter, Tab' },
				]}
				label="Arrows and Enter"
				onchange={(v) => (c.symbols = v === 'symbols')}
			/>
		</Row>
	</Card>

	<Card
		title="Theme"
		lead="Every theme draws the same shapes in its own colors, measured for contrast on light and dark screens."
	>
		<ThemePicker config={s.overlay} caption={sampleCaption} onchoose={chooseTheme} />
		{#if s.overlay.theme === 'custom' && s.overlay.custom}
			<Row label="Accent" help="Modifier keys, lamps and the accent edge.">
				<ColorField bind:value={s.overlay.custom.accent} label="Accent color" />
			</Row>
			<Row label="Face" help="The callout plate and the keycaps.">
				<ColorField bind:value={s.overlay.custom.face} label="Face color" />
			</Row>
			<Row label="Ink" help="Key legends and captions.">
				<ColorField bind:value={s.overlay.custom.ink} label="Ink color" />
			</Row>
			<Row label="Glow" help="The soft light around each callout.">
				<ColorField bind:value={s.overlay.custom.glow} label="Glow color" />
			</Row>
			{#if custom?.adjusted}
				<Notice tone="info" lead="Ink adjusted for readability">
					Some text would be hard to read on this face, so its lightness was moved until it reads at
					4.5 to 1 or better. The preview shows the result.
				</Notice>
			{/if}
		{/if}
	</Card>

	<Card title="Position">
		<Row label="Place on screen" help="The stack grows away from the edge you pick.">
			<AnchorPicker bind:value={c.anchor} label="Where the callouts sit" onchange={setAnchor} />
		</Row>
		<Row label={offsets.x} help="Pixels at 1080p; they grow with the overall scale." for="st-co-x">
			<NumberField
				id="st-co-x"
				bind:value={c.offset_x}
				min={LIMITS.offset.min}
				max={LIMITS.offset.max}
				step={4}
				unit="px"
				label={offsets.x}
			/>
		</Row>
		<Row label={offsets.y} for="st-co-y">
			<NumberField
				id="st-co-y"
				bind:value={c.offset_y}
				min={LIMITS.offset.min}
				max={LIMITS.offset.max}
				step={4}
				unit="px"
				label={offsets.y}
			/>
		</Row>
		<Row label="Stack grows" help="Where a new callout appears in the stack.">
			<Segmented
				bind:value={c.direction}
				options={[
					{ value: 'up', label: 'Upward', hint: 'The newest sits at the bottom' },
					{ value: 'down', label: 'Downward', hint: 'The newest sits at the top' },
				]}
				label="Stack grows"
			/>
		</Row>
	</Card>

	<Card title="Size and type">
		<Row label="Size" help="The keycap height: S 34, M 46, L 58, XL 76 pixels at 1080p.">
			<Segmented
				bind:value={c.size}
				options={[
					{ value: 's', label: 'S', hint: 'Small' },
					{ value: 'm', label: 'M', hint: 'Medium' },
					{ value: 'l', label: 'L', hint: 'Large' },
					{ value: 'xl', label: 'XL', hint: 'Extra large' },
				]}
				label="Callout size"
			/>
		</Row>
		<Row label="Key font" help="The typeface on the keycaps. Captions stay in Rajdhani.">
			<Segmented
				bind:value={s.overlay.font}
				options={[
					{ value: 'rajdhani', label: 'Rajdhani', font: "'Rajdhani', sans-serif" },
					{ value: 'orbitron', label: 'Orbitron', font: "'Orbitron', sans-serif" },
					{
						value: 'share-tech-mono',
						label: 'Share Tech Mono',
						font: "'Share Tech Mono', monospace",
					},
				]}
				label="Key font"
			/>
		</Row>
		<Row label="Stack length" help="How many callouts can be on screen at once.">
			<Slider
				bind:value={c.max_stack}
				min={LIMITS.maxStack.min}
				max={LIMITS.maxStack.max}
				label="Stack length"
				format={(v) => `${v} ${v === 1 ? 'callout' : 'callouts'}`}
			/>
		</Row>
	</Card>

	<Card title="Timing">
		<Row label="Stay on screen" help="How long a callout stays after its last key.">
			<Slider
				bind:value={c.linger_ms}
				min={LIMITS.lingerMs.min}
				max={LIMITS.lingerMs.max}
				step={100}
				label="Stay on screen"
				format={formatMs}
			/>
		</Row>
		<Row label="Fade out" help="How long it takes to fade away.">
			<Slider
				bind:value={c.fade_ms}
				min={LIMITS.fadeMs.min}
				max={LIMITS.fadeMs.max}
				step={50}
				label="Fade out"
				format={formatMs}
			/>
		</Row>
		<Row
			label="Repeat window"
			help="The same shortcut again within this time counts up on one callout (x2, x3)."
		>
			<Slider
				bind:value={s.display.repeat_window_ms}
				min={RULE_LIMITS.repeat_window_ms.min}
				max={RULE_LIMITS.repeat_window_ms.max}
				step={100}
				label="Repeat window"
				format={formatMs}
			/>
		</Row>
	</Card>

	<Card title="Details">
		<Row label="Left and right modifiers" help="Show Left Ctrl or Right Ctrl instead of Ctrl.">
			<Switch bind:checked={s.display.show_modifier_side} label="Left and right modifiers" />
		</Row>
		<Row
			label="Input from other programs"
			help="Show keys that software sends, such as macros and remote tools."
		>
			<Switch bind:checked={s.display.show_injected} label="Input from other programs" />
		</Row>
		<Row
			label="Status pills"
			help="Show Paused or Hidden beside the callouts while nothing else can show."
		>
			<span class="st-pair">
				<span class="st-pair-item">
					<span class="st-pair-word">Paused</span>
					<Switch bind:checked={s.overlay.indicators.paused} label="Paused pill" />
				</span>
				<span class="st-pair-item">
					<span class="st-pair-word">Hidden</span>
					<Switch bind:checked={s.overlay.indicators.suppressed} label="Hidden pill" />
				</span>
			</span>
		</Row>
	</Card>
</div>

<style>
	.st-stack {
		display: flex;
		flex-direction: column;
		gap: 1.6rem;
	}
	.st-pair {
		display: flex;
		flex-wrap: wrap;
		gap: 1rem;
	}
	.st-pair-item {
		display: inline-flex;
		align-items: center;
		gap: 0.5rem;
	}
	.st-pair-word {
		font-weight: 600;
	}
</style>
