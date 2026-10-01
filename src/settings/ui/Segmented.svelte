<!--
	A segmented control: raised keys in a recessed tray, one of them lit (the
	accent ring, aria-checked and a heavier word). A radio group: arrow keys
	move the choice, Home and End jump to the ends.
-->
<script lang="ts" generics="T extends string">
	import Icon from './Icon.svelte';
	import type { IconName } from './icons';

	interface Option {
		value: T;
		label: string;
		icon?: IconName;
		/** Extra words for assistive technology and the tooltip. */
		hint?: string;
		/** A font family for the label (the font picker shows each font in itself). */
		font?: string;
	}

	interface Props {
		value: T;
		options: readonly Option[];
		/** Accessible name of the group. */
		label: string;
		disabled?: boolean;
		id?: string;
		describedby?: string;
		onchange?: (value: T) => void;
		/** Stretch to the row's width. */
		fill?: boolean;
		/** Lay the keys out in this many equal columns. */
		columns?: number;
	}

	let {
		value = $bindable(),
		options,
		label,
		disabled = false,
		id,
		describedby,
		onchange,
		fill = false,
		columns,
	}: Props = $props();

	let buttons: HTMLButtonElement[] = $state([]);

	function choose(v: T) {
		if (disabled || v === value) return;
		value = v;
		onchange?.(v);
	}

	function onkeydown(e: KeyboardEvent, index: number) {
		const n = options.length;
		let next = -1;
		if (e.key === 'ArrowRight' || e.key === 'ArrowDown') next = (index + 1) % n;
		else if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') next = (index - 1 + n) % n;
		else if (e.key === 'Home') next = 0;
		else if (e.key === 'End') next = n - 1;
		if (next < 0) return;
		e.preventDefault();
		const opt = options[next];
		if (!opt) return;
		choose(opt.value);
		buttons[next]?.focus();
	}
</script>

<div
	{id}
	class="st-tray"
	class:is-fill={fill}
	class:is-grid={columns !== undefined}
	style:grid-template-columns={columns ? `repeat(${columns}, minmax(0, 1fr))` : undefined}
	role="radiogroup"
	aria-label={label}
	aria-describedby={describedby}
	aria-disabled={disabled || undefined}
>
	{#each options as opt, i (opt.value)}
		{@const on = opt.value === value}
		<button
			bind:this={buttons[i]}
			type="button"
			role="radio"
			class="plate-key st-seg"
			class:is-wide={opt.label.length > 6}
			aria-checked={on}
			tabindex={on ? 0 : -1}
			title={opt.hint}
			{disabled}
			style:font-family={opt.font}
			onclick={() => choose(opt.value)}
			onkeydown={(e) => onkeydown(e, i)}
		>
			{#if opt.icon}<Icon name={opt.icon} size={18} />{/if}
			<span class="st-seg-word">{opt.label}</span>
			{#if opt.hint}<span class="st-sr">{opt.hint}</span>{/if}
		</button>
	{/each}
</div>

<style>
	.st-tray {
		display: inline-flex;
		flex-wrap: wrap;
		gap: 6px;
		padding: 5px;
		border: 1px solid var(--plate-hair);
		border-radius: calc(var(--plate-r-control) + 4px);
		background-color: var(--plate-well-ground);
		background-image: linear-gradient(to bottom, var(--plate-well-top), var(--plate-well-face) 30%);
		box-shadow:
			inset 0 1px 0 0 var(--plate-well-shade),
			inset 0 2px 3px -1px var(--plate-well-shade),
			0 1px 0 0 var(--plate-well-lip);
		max-width: 100%;
	}
	.st-tray.is-fill {
		display: flex;
	}
	.st-tray.is-fill .st-seg {
		flex: 1 1 0;
	}
	.st-tray.is-grid {
		display: grid;
	}
	:global(.ks-plate) .st-seg {
		min-width: 52px;
		padding: 0 0.95rem;
		font-size: 0.95rem;
	}
	:global(.ks-plate) .st-seg[aria-checked='true'] {
		font-weight: 700;
	}
	.st-seg-word {
		padding-top: 1px;
		white-space: nowrap;
	}
</style>
