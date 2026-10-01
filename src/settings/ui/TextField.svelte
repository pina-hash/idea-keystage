<!--
	A text field set into the surface (a well). Mono for addresses, process
	names and other machine text. An error is written under the field and
	tied to it with aria-describedby, so it is never shown by color alone.
-->
<script lang="ts">
	import Icon from './Icon.svelte';
	import type { IconName } from './icons';

	interface Props {
		value: string;
		/** Accessible name (or use an outside <label for={id}>). */
		label?: string;
		id?: string;
		type?: 'text' | 'password' | 'search';
		placeholder?: string;
		mono?: boolean;
		error?: string | null;
		disabled?: boolean;
		readonly?: boolean;
		maxlength?: number;
		icon?: IconName;
		/** Characters wide (sets the field's width). */
		size?: number;
		describedby?: string;
		autocomplete?: 'off' | 'new-password' | 'current-password';
		onenter?: () => void;
		oninput?: (value: string) => void;
		onblur?: () => void;
	}

	let {
		value = $bindable(),
		label,
		id,
		type = 'text',
		placeholder,
		mono = false,
		error = null,
		disabled = false,
		readonly = false,
		maxlength,
		icon,
		size,
		describedby,
		autocomplete = 'off',
		onenter,
		oninput,
		onblur,
	}: Props = $props();

	const uid = $props.id();
	const errorId = $derived(`${id ?? uid}-error`);
	const described = $derived(
		[describedby, error ? errorId : null].filter(Boolean).join(' ') || undefined
	);
</script>

<span class="st-field-wrap" class:has-size={size !== undefined} style:--chars={size}>
	<span
		class="plate-well st-field"
		class:is-error={!!error}
		class:is-mono={mono}
		class:is-disabled={disabled}
	>
		{#if icon}<Icon name={icon} size={18} />{/if}
		<input
			id={id ?? uid}
			{type}
			bind:value
			{placeholder}
			{disabled}
			{readonly}
			{maxlength}
			{autocomplete}
			spellcheck="false"
			aria-label={label}
			aria-invalid={error ? true : undefined}
			aria-describedby={described}
			oninput={() => oninput?.(value)}
			onkeydown={(e) => {
				if (e.key === 'Enter') {
					e.preventDefault();
					onenter?.();
				}
			}}
			{onblur}
		/>
	</span>
	{#if error}
		<span class="st-field-error" id={errorId}><Icon name="warning" size={16} />{error}</span>
	{/if}
</span>

<style>
	.st-field-wrap {
		display: inline-flex;
		flex-direction: column;
		gap: 0.35rem;
		min-width: 0;
		max-width: 100%;
	}
	.st-field-wrap.has-size {
		width: calc(var(--chars) * 1ch + 3.5rem);
	}
	.st-field {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0 0.75rem;
		min-width: 0;
		color: var(--plate-ink-2);
	}
	.st-field.is-error {
		border-color: var(--plate-danger-ink);
	}
	:global(.ks-plate) .st-field.is-disabled {
		background: var(--plate-disabled-face);
		border-color: var(--plate-disabled-edge);
		box-shadow: none;
	}
	input {
		flex: 1 1 auto;
		min-width: 0;
		height: 42px;
		border: 0;
		outline: none;
		background: transparent;
		color: var(--plate-ink);
		font-family: var(--font-display);
		font-size: 1rem;
		font-weight: 500;
	}
	.is-mono input {
		font-family: var(--font-mono);
		font-size: 0.92rem;
		letter-spacing: 0.02em;
	}
	input::placeholder {
		color: var(--plate-ink-2);
		opacity: 1;
	}
	.is-disabled input {
		color: var(--plate-disabled);
	}
	.st-field-error {
		display: inline-flex;
		align-items: flex-start;
		gap: 0.35rem;
		color: var(--plate-danger-ink);
		font-size: 0.92rem;
		line-height: 1.3;
	}
</style>
