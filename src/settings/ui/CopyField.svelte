<!--
	A value to copy into another program: the text in a read-only mono well
	(selectable), and a Copy key that confirms with a tick and the word
	"Copied" for two seconds, or says why it could not copy.
-->
<script lang="ts">
	import Key from './Key.svelte';

	interface Props {
		value: string;
		label: string;
		copy: (text: string) => Promise<void>;
	}

	let { value, label, copy }: Props = $props();

	const uid = $props.id();
	let copyState = $state<'idle' | 'copied' | 'failed'>('idle');
	let error = $state('');
	let timer: ReturnType<typeof setTimeout> | null = null;

	async function onclick() {
		if (timer) clearTimeout(timer);
		try {
			await copy(value);
			copyState = 'copied';
		} catch (e) {
			copyState = 'failed';
			error =
				typeof e === 'string'
					? e
					: e instanceof Error
						? e.message
						: 'The clipboard is not available.';
		}
		timer = setTimeout(() => (copyState = 'idle'), copyState === 'copied' ? 2000 : 6000);
	}

	$effect(() => () => {
		if (timer) clearTimeout(timer);
	});
</script>

<div class="st-copy">
	<div class="st-copy-line">
		<input
			class="plate-well st-copy-value"
			id={uid}
			type="text"
			readonly
			{value}
			aria-label={label}
			spellcheck="false"
			onfocus={(e) => (e.currentTarget as HTMLInputElement).select()}
		/>
		<Key icon={copyState === 'copied' ? 'check' : 'copy'} {onclick} pressed={undefined}>
			{copyState === 'copied' ? 'Copied' : 'Copy'}
		</Key>
	</div>
	<span class="st-copy-status" class:is-empty={copyState !== 'failed'} role="status">
		{#if copyState === 'failed'}{error}{/if}
	</span>
</div>

<style>
	.st-copy {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}
	.st-copy-line {
		display: flex;
		gap: 0.5rem;
		align-items: center;
		min-width: 0;
	}
	:global(.ks-plate) .st-copy-value {
		flex: 1 1 auto;
		min-width: 0;
		height: 44px;
		padding: 0 0.8rem;
		font-family: var(--font-mono);
		font-size: 0.9rem;
		letter-spacing: 0.01em;
		color: var(--plate-ink);
		outline: none;
	}
	.st-copy-status.is-empty {
		padding: 0;
	}
	.st-copy-status {
		padding-top: 0.25rem;
		color: var(--plate-danger-ink);
		font-size: 0.92rem;
	}
</style>
