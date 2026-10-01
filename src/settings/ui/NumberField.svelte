<!--
	A number in a well with a step key on each side. Typing is committed on
	Enter or when the field loses focus, clamped to the range; a value that
	cannot be read is reported under the field and not saved.
-->
<script lang="ts">
	interface Props {
		value: number;
		min: number;
		max: number;
		step?: number;
		/** Accessible name. */
		label: string;
		unit?: string;
		id?: string;
		disabled?: boolean;
		describedby?: string;
		onchange?: (v: number) => void;
	}

	let {
		value = $bindable(),
		min,
		max,
		step = 1,
		label,
		unit,
		id,
		disabled = false,
		describedby,
		onchange,
	}: Props = $props();

	const uid = $props.id();
	let text = $state('');
	let error = $state<string | null>(null);
	let editing = $state(false);

	$effect(() => {
		if (!editing) text = String(value);
	});

	function clamp(v: number): number {
		const stepped = Math.round(v / step) * step;
		return Math.min(max, Math.max(min, Number(stepped.toFixed(6))));
	}

	function set(v: number) {
		const next = clamp(v);
		error = null;
		text = String(next);
		if (next !== value) {
			value = next;
			onchange?.(next);
		}
	}

	function commit() {
		editing = false;
		const t = text.trim();
		const v = Number(t);
		if (t.length === 0 || !Number.isFinite(v)) {
			error = `Type a number from ${min} to ${max}.`;
			return;
		}
		set(v);
	}
</script>

<span class="st-num-wrap">
	<span class="st-num">
		<button
			type="button"
			class="plate-key st-step"
			aria-label={`Decrease ${label}`}
			disabled={disabled || value <= min}
			onclick={() => set(value - step)}
		>
			<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path d="M6 12h12" /></svg>
		</button>
		<span class="plate-well st-num-well" class:is-error={!!error}>
			<input
				id={id ?? uid}
				type="text"
				inputmode="numeric"
				bind:value={text}
				{disabled}
				aria-label={label}
				aria-invalid={error ? true : undefined}
				aria-describedby={[describedby, error ? `${uid}-err` : null].filter(Boolean).join(' ') ||
					undefined}
				onfocus={() => (editing = true)}
				onblur={commit}
				onkeydown={(e) => {
					if (e.key === 'Enter') {
						e.preventDefault();
						commit();
						editing = true;
					} else if (e.key === 'ArrowUp') {
						e.preventDefault();
						set(value + step);
					} else if (e.key === 'ArrowDown') {
						e.preventDefault();
						set(value - step);
					}
				}}
			/>
			{#if unit}<span class="st-unit" aria-hidden="true">{unit}</span>{/if}
		</span>
		<button
			type="button"
			class="plate-key st-step"
			aria-label={`Increase ${label}`}
			disabled={disabled || value >= max}
			onclick={() => set(value + step)}
		>
			<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"
				><path d="M6 12h12M12 6v12" /></svg
			>
		</button>
	</span>
	{#if error}<span class="st-num-error" id="{uid}-err">{error}</span>{/if}
</span>

<style>
	.st-num-wrap {
		display: inline-flex;
		flex-direction: column;
		gap: 0.35rem;
	}
	.st-num {
		display: inline-flex;
		align-items: center;
		gap: 6px;
	}
	:global(.ks-plate) .st-step {
		width: 44px;
		padding: 0;
	}
	.st-step svg path {
		fill: none;
		stroke: currentColor;
		stroke-width: 2;
		stroke-linecap: round;
	}
	.st-num-well {
		display: inline-flex;
		align-items: center;
		padding: 0 0.6rem;
		width: 8.5rem;
	}
	.st-num-well.is-error {
		border-color: var(--plate-danger-ink);
	}
	input {
		width: 100%;
		min-width: 0;
		height: 42px;
		border: 0;
		outline: none;
		background: transparent;
		text-align: right;
		font-family: var(--font-mono);
		font-size: 0.95rem;
		font-variant-numeric: tabular-nums;
		color: var(--plate-ink);
	}
	.st-unit {
		margin-left: 0.4rem;
		font-family: var(--font-mono);
		font-size: 0.8rem;
		color: var(--plate-ink-2);
	}
	.st-num-error {
		color: var(--plate-danger-ink);
		font-size: 0.92rem;
	}
</style>
