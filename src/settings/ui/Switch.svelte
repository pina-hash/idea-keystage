<!--
	An on/off switch: a raised key holding a recessed track with a disc (IDEA
	Classroom's switch). Off: the disc sits left and carries a cross. On: the
	disc moves right, fills with the accent and carries a tick, and the word
	reads On. Position, glyph and word all change, so color is never the only
	signal; role="switch" with aria-checked says it to assistive technology.
-->
<script lang="ts">
	interface Props {
		checked: boolean;
		/** Accessible name (the visible label usually sits in the row beside it). */
		label: string;
		disabled?: boolean;
		id?: string;
		describedby?: string;
		onchange?: (checked: boolean) => void;
		onWord?: string;
		offWord?: string;
	}

	let {
		checked = $bindable(),
		label,
		disabled = false,
		id,
		describedby,
		onchange,
		onWord = 'On',
		offWord = 'Off',
	}: Props = $props();

	function toggle() {
		if (disabled) return;
		checked = !checked;
		onchange?.(checked);
	}
</script>

<button
	{id}
	type="button"
	role="switch"
	class="st-switch"
	class:is-on={checked}
	aria-checked={checked}
	aria-label={label}
	aria-describedby={describedby}
	{disabled}
	onclick={toggle}
>
	<span class="st-track" aria-hidden="true">
		<span class="st-knob">
			<svg viewBox="0 0 10 10" width="10" height="10">
				{#if checked}
					<path d="M1.8 5.2l2.2 2.2 4.2-4.6" />
				{:else}
					<path d="M2.2 2.2l5.6 5.6M7.8 2.2l-5.6 5.6" />
				{/if}
			</svg>
		</span>
	</span>
	<span class="st-switch-word">{checked ? onWord : offWord}</span>
</button>

<style>
	:global(.ks-plate) .st-switch {
		position: relative;
		display: inline-flex;
		align-items: center;
		gap: 0.6rem;
		min-height: 44px;
		min-width: 44px;
		padding: 0 0.85rem 0 0.7rem;
		cursor: pointer;
		white-space: nowrap;
		border: 1px solid;
		border-color: var(--plate-hair-top) var(--plate-hair) var(--plate-hair);
		border-radius: var(--plate-r-control);
		background:
			linear-gradient(to bottom, var(--plate-face-top), var(--plate-face-bot)) padding-box,
			linear-gradient(to bottom, var(--plate-hair-top), var(--plate-hair)) border-box;
		background-color: var(--plate-face-ground);
		box-shadow:
			inset 0 0 0 1px var(--plate-bezel-mid),
			inset 0 2px 0 0 var(--plate-bezel-hi),
			inset 0 -2px 2px -1px var(--plate-foot),
			0 0 0 1px var(--plate-rim-out),
			0 2px 3px 0 var(--plate-drop),
			0 6px 10px -5px var(--plate-drop-far);
		color: var(--plate-btn-ink);
		outline: none;
		font-family: var(--font-mono);
		font-size: var(--plate-label-size);
		letter-spacing: var(--plate-label-track);
		text-transform: uppercase;
	}
	:global(.ks-plate) .st-switch:hover {
		border-color: var(--plate-hover-ink);
	}
	:global(.ks-plate) .st-switch:focus-visible {
		outline: 2px solid var(--plate-focus);
		outline-offset: 2px;
	}
	:global(.ks-plate) .st-switch:active:not(:disabled) {
		background:
			linear-gradient(to bottom, var(--plate-press-top), var(--plate-press-bot)) padding-box,
			linear-gradient(to bottom, var(--plate-hair-top), var(--plate-hair)) border-box;
		box-shadow:
			inset 0 3px 4px 0 var(--plate-press-shade),
			inset 0 -1px 0 0 var(--plate-bezel-hi),
			0 0 0 1px var(--plate-rim-out);
	}
	:global(.ks-plate) .st-switch:disabled {
		cursor: default;
		background: var(--plate-disabled-face);
		box-shadow:
			inset 0 1px 0 0 var(--plate-plate-hi),
			inset 0 -2px 2px -1px var(--plate-tube-lo);
		color: var(--plate-disabled);
		border: 1px solid var(--plate-disabled-edge);
	}
	.st-track {
		position: relative;
		flex: none;
		width: 40px;
		height: 20px;
		border-radius: 10px;
		background: linear-gradient(to bottom, var(--plate-well-shade), var(--plate-track) 6px);
		box-shadow:
			inset 0 1px 2px 0 var(--plate-drop),
			0 1px 0 0 var(--plate-well-lip);
	}
	.st-knob {
		position: absolute;
		top: 1px;
		left: 1px;
		width: 18px;
		height: 18px;
		border-radius: 50%;
		display: grid;
		place-items: center;
		background: radial-gradient(
			circle at 50% 35%,
			var(--plate-knob-top),
			var(--plate-knob-bot) 70%
		);
		box-shadow:
			0 0 0 1px var(--plate-hair),
			0 2px 3px 0 var(--plate-knob-shade);
		color: var(--plate-knob-icon);
	}
	.is-on .st-knob {
		left: 21px;
		background: radial-gradient(
			circle at 50% 35%,
			var(--plate-accent-glow),
			var(--plate-accent) 55%
		);
		box-shadow:
			0 0 0 1px var(--plate-hair),
			0 0 6px 1px var(--plate-accent-tint),
			0 2px 3px 0 var(--plate-knob-shade);
		color: var(--plate-knob-on-icon);
	}
	.st-switch:disabled .st-knob {
		background: var(--plate-disabled-face);
		color: var(--plate-disabled);
	}
	.st-knob svg {
		display: block;
	}
	.st-knob path {
		fill: none;
		stroke: currentColor;
		stroke-width: 1.4;
		stroke-linecap: round;
		stroke-linejoin: round;
	}
	.st-switch-word {
		min-width: 2.2em;
		padding-top: 1px;
	}
	.is-on .st-switch-word {
		color: var(--plate-ink);
	}
	@media (prefers-reduced-motion: no-preference) {
		.st-knob {
			transition:
				left 140ms var(--ease-out),
				background-color 140ms ease-out;
		}
	}
</style>
