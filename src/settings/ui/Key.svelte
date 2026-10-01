<!--
	A raised key: anything pressable (plate.css .plate-key). Primary is the
	one accent face on a surface; danger keeps the danger ink on a gray key;
	`pressed` lights it (accent ring plus aria-pressed, never color alone).
-->
<script lang="ts">
	import type { Snippet } from 'svelte';
	import Icon from './Icon.svelte';
	import type { IconName } from './icons';

	interface Props {
		onclick?: (e: MouseEvent) => void;
		variant?: 'plain' | 'primary' | 'danger';
		/** A toggle key: true or false sets aria-pressed; undefined leaves it out. */
		pressed?: boolean;
		disabled?: boolean;
		/** Shows a working state and blocks clicks. */
		busy?: boolean;
		size?: 'm' | 'l';
		icon?: IconName;
		iconFilled?: boolean;
		/** Accessible name when the key shows only an icon. */
		label?: string;
		title?: string;
		type?: 'button' | 'submit';
		id?: string;
		describedby?: string;
		children?: Snippet;
	}

	let {
		onclick,
		variant = 'plain',
		pressed,
		disabled = false,
		busy = false,
		size = 'm',
		icon,
		iconFilled = false,
		label,
		title,
		type = 'button',
		id,
		describedby,
		children,
	}: Props = $props();
</script>

<button
	{id}
	{type}
	class="plate-key st-key"
	class:is-primary={variant === 'primary'}
	class:is-danger={variant === 'danger'}
	class:is-wide={!!children}
	class:is-large={size === 'l'}
	class:is-icon={!children}
	aria-pressed={pressed}
	aria-label={label}
	aria-busy={busy || undefined}
	aria-describedby={describedby}
	title={title ?? label}
	disabled={disabled || busy}
	{onclick}
>
	{#if icon}<Icon name={icon} size={size === 'l' ? 24 : 18} filled={iconFilled} />{/if}
	{#if children}<span class="st-key-word">{@render children()}</span>{/if}
	{#if busy}<span class="st-key-busy" aria-hidden="true"></span>{/if}
</button>

<style>
	:global(.ks-plate) .st-key {
		white-space: nowrap;
		font-size: 0.98rem;
		line-height: 1;
	}
	:global(.ks-plate) .st-key.is-icon {
		padding: 0;
		width: 44px;
	}
	:global(.ks-plate) .st-key.is-large {
		min-height: 60px;
		padding: 0 1.5rem;
		font-size: 1.2rem;
		letter-spacing: 0.06em;
		gap: 0.7rem;
	}
	.st-key-word {
		display: inline-block;
		padding-top: 1px;
	}
	.st-key-busy {
		width: 12px;
		height: 12px;
		border-radius: 50%;
		border: 2px solid currentColor;
		border-right-color: transparent;
	}
	@media (prefers-reduced-motion: no-preference) {
		.st-key-busy {
			animation: st-spin 0.8s linear infinite;
		}
	}
	@keyframes st-spin {
		to {
			transform: rotate(360deg);
		}
	}
</style>
