<!--
	A key whose action cannot be undone: the first press asks, a second key
	confirms. Cancel, Escape or leaving it alone for ten seconds puts it back.
-->
<script lang="ts">
	import Key from './Key.svelte';
	import type { IconName } from './icons';

	interface Props {
		label: string;
		confirmLabel: string;
		question?: string;
		icon?: IconName;
		disabled?: boolean;
		busy?: boolean;
		onconfirm: () => void | Promise<void>;
	}

	let {
		label,
		confirmLabel,
		question,
		icon = 'trash',
		disabled = false,
		busy = false,
		onconfirm,
	}: Props = $props();

	let asking = $state(false);
	let timer: ReturnType<typeof setTimeout> | null = null;

	function ask() {
		asking = true;
		if (timer) clearTimeout(timer);
		timer = setTimeout(() => (asking = false), 10_000);
	}

	function cancel() {
		asking = false;
		if (timer) clearTimeout(timer);
		timer = null;
	}

	async function confirm() {
		cancel();
		await onconfirm();
	}

	$effect(() => () => {
		if (timer) clearTimeout(timer);
	});
</script>

<svelte:window
	onkeydown={(e) => {
		if (asking && e.key === 'Escape') cancel();
	}}
/>

{#if asking}
	<span class="st-confirm" role="group" aria-label={question ?? confirmLabel}>
		{#if question}<span class="st-confirm-q">{question}</span>{/if}
		<Key variant="danger" {icon} onclick={confirm} {busy}>{confirmLabel}</Key>
		<Key onclick={cancel}>Cancel</Key>
	</span>
{:else}
	<Key variant="danger" {icon} onclick={ask} {disabled} {busy}>{label}</Key>
{/if}

<style>
	.st-confirm {
		display: inline-flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 0.5rem;
	}
	.st-confirm-q {
		font-weight: 600;
		color: var(--plate-ink);
	}
</style>
