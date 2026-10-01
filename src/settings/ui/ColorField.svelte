<!--
	A color: a raised swatch key that opens the system color picker, and the
	hex value in a mono well beside it (typed values are checked before they
	are saved). The hex text always says the color, so it is not shown by the
	swatch alone.
-->
<script lang="ts">
	interface Props {
		value: string;
		label: string;
		id?: string;
		disabled?: boolean;
		onchange?: (v: string) => void;
	}

	let { value = $bindable(), label, id, disabled = false, onchange }: Props = $props();

	const uid = $props.id();
	let text = $state('');
	let error = $state<string | null>(null);
	let editing = $state(false);

	$effect(() => {
		if (!editing) text = value;
	});

	/** #rgb, #rgba, #rrggbb or #rrggbbaa (keystage-core accepts these). */
	function isHex(s: string): boolean {
		return /^#([0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/.test(s);
	}

	/** The color input only takes #rrggbb. */
	const pickerValue = $derived.by(() => {
		const v = value.trim();
		if (/^#[0-9a-fA-F]{6}/.test(v)) return v.slice(0, 7).toLowerCase();
		if (/^#[0-9a-fA-F]{3,4}$/.test(v)) {
			const [r, g, b] = [v[1], v[2], v[3]];
			return `#${r}${r}${g}${g}${b}${b}`.toLowerCase();
		}
		return '#000000';
	});

	function set(v: string) {
		error = null;
		if (v !== value) {
			value = v;
			onchange?.(v);
		}
	}

	function commit() {
		editing = false;
		let t = text.trim();
		if (t.length > 0 && !t.startsWith('#')) t = `#${t}`;
		if (!isHex(t)) {
			error = 'Type a hex color like #78b870.';
			return;
		}
		set(t.toLowerCase());
	}
</script>

<span class="st-color-wrap">
	<span class="st-color">
		<label class="st-swatch plate-key" class:is-disabled={disabled} title="Choose {label}">
			<span class="st-swatch-fill" style:background={value}></span>
			<input
				type="color"
				value={pickerValue}
				{disabled}
				aria-label={`Choose ${label}`}
				oninput={(e) => set((e.currentTarget as HTMLInputElement).value)}
			/>
		</label>
		<span class="plate-well st-hex" class:is-error={!!error}>
			<input
				id={id ?? uid}
				type="text"
				bind:value={text}
				{disabled}
				maxlength="9"
				spellcheck="false"
				aria-label={label}
				aria-invalid={error ? true : undefined}
				aria-describedby={error ? `${uid}-err` : undefined}
				onfocus={() => (editing = true)}
				onblur={commit}
				onkeydown={(e) => {
					if (e.key === 'Enter') {
						e.preventDefault();
						commit();
						editing = true;
					}
				}}
			/>
		</span>
	</span>
	{#if error}<span class="st-color-error" id="{uid}-err">{error}</span>{/if}
</span>

<style>
	.st-color-wrap {
		display: inline-flex;
		flex-direction: column;
		gap: 0.35rem;
	}
	.st-color {
		display: inline-flex;
		align-items: center;
		gap: 8px;
	}
	:global(.ks-plate) .st-swatch {
		position: relative;
		width: 52px;
		padding: 5px;
	}
	.st-swatch-fill {
		display: block;
		width: 100%;
		height: 30px;
		border-radius: 6px;
		box-shadow:
			inset 0 0 0 1px rgba(0, 0, 0, 0.35),
			inset 0 0 0 2px rgba(255, 255, 255, 0.25);
	}
	.st-swatch input {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		opacity: 0;
		cursor: pointer;
		border: 0;
		padding: 0;
	}
	.st-swatch:focus-within {
		outline: 2px solid var(--plate-focus);
		outline-offset: 2px;
	}
	.st-hex {
		display: inline-flex;
		align-items: center;
		padding: 0 0.6rem;
		width: 7.5rem;
	}
	.st-hex.is-error {
		border-color: var(--plate-danger-ink);
	}
	.st-hex input {
		width: 100%;
		height: 42px;
		border: 0;
		outline: none;
		background: transparent;
		font-family: var(--font-mono);
		font-size: 0.92rem;
		color: var(--plate-ink);
	}
	.st-color-error {
		color: var(--plate-danger-ink);
		font-size: 0.92rem;
	}
</style>
