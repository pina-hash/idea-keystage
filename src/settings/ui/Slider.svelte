<!--
	A slider: a recessed groove with the accent filling up to the value, a
	raised round thumb, and the value in an inset readout beside it. The
	native range input keeps keyboard and assistive support; the 44 px row is
	the target.
-->
<script lang="ts">
	interface Props {
		value: number;
		min: number;
		max: number;
		step?: number;
		/** Accessible name. */
		label: string;
		/** The readout text (also announced as aria-valuetext). */
		format?: (v: number) => string;
		disabled?: boolean;
		id?: string;
		describedby?: string;
		onchange?: (v: number) => void;
	}

	let {
		value = $bindable(),
		min,
		max,
		step = 1,
		label,
		format = (v: number) => String(v),
		disabled = false,
		id,
		describedby,
		onchange,
	}: Props = $props();

	const pct = $derived(
		max > min ? Math.min(100, Math.max(0, ((value - min) / (max - min)) * 100)) : 0
	);
	const text = $derived(format(value));

	function oninput(e: Event) {
		const v = Number((e.currentTarget as HTMLInputElement).value);
		if (!Number.isFinite(v)) return;
		value = v;
		onchange?.(v);
	}
</script>

<div class="st-slider" class:is-disabled={disabled}>
	<input
		{id}
		type="range"
		{min}
		{max}
		{step}
		{value}
		{disabled}
		aria-label={label}
		aria-describedby={describedby}
		aria-valuetext={text}
		style:--pct="{pct}%"
		{oninput}
	/>
	<output class="st-readout" aria-hidden="true">{text}</output>
</div>

<style>
	.st-slider {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		min-height: 44px;
		width: 360px;
		max-width: 100%;
	}
	input {
		-webkit-appearance: none;
		appearance: none;
		flex: 1 1 auto;
		min-width: 120px;
		height: 44px;
		margin: 0;
		background: transparent;
		cursor: pointer;
		outline: none;
	}
	input:disabled {
		cursor: default;
	}
	input::-webkit-slider-runnable-track {
		height: 8px;
		border-radius: 4px;
		background:
			linear-gradient(
					to right,
					var(--plate-accent) 0,
					var(--plate-accent) var(--pct),
					transparent var(--pct)
				)
				0 2px / 100% 4px no-repeat,
			linear-gradient(to bottom, var(--plate-well-shade), var(--plate-track) 4px);
		box-shadow:
			inset 0 1px 2px 0 var(--plate-drop),
			0 1px 0 0 var(--plate-well-lip);
	}
	input:disabled::-webkit-slider-runnable-track {
		background: var(--plate-disabled-face);
	}
	input::-webkit-slider-thumb {
		-webkit-appearance: none;
		appearance: none;
		width: 22px;
		height: 22px;
		margin-top: -7px;
		border-radius: 50%;
		border: 1px solid var(--plate-hair);
		background: radial-gradient(
			circle at 50% 35%,
			var(--plate-knob-top),
			var(--plate-knob-bot) 72%
		);
		box-shadow:
			inset 0 1px 0 0 var(--plate-bezel-hi),
			0 0 0 1px var(--plate-rim-out),
			0 2px 3px 0 var(--plate-knob-shade);
	}
	input:hover::-webkit-slider-thumb {
		border-color: var(--plate-hover-ink);
	}
	input:focus-visible::-webkit-slider-thumb {
		outline: 2px solid var(--plate-focus);
		outline-offset: 2px;
	}
	input:disabled::-webkit-slider-thumb {
		background: var(--plate-disabled-face);
		border-color: var(--plate-disabled-edge);
	}
	.st-readout {
		flex: none;
		min-width: 5.2rem;
		text-align: center;
		padding: 0.3rem 0.5rem;
		border-radius: var(--plate-r-small);
		font-family: var(--font-mono);
		font-size: 0.85rem;
		letter-spacing: 0.02em;
		font-variant-numeric: tabular-nums;
		color: var(--plate-ink);
		background-color: var(--plate-chip-ground);
		background-image: linear-gradient(to bottom, var(--plate-chip-top), var(--plate-chip-bot));
		box-shadow:
			inset 0 0 0 1px var(--plate-chip-edge),
			inset 0 2px 2px -1px var(--plate-chip-shade),
			inset 0 -1px 0 0 var(--plate-chip-lip),
			0 1px 0 0 var(--plate-chip-lip);
	}
	.is-disabled .st-readout {
		color: var(--plate-disabled);
	}
</style>
