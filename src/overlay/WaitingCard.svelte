<!--
	The connection card shown in a browser (or with status=1) until the app
	answers: a floating housing in the active theme with a status lamp, the
	address it is trying and a short hint.
-->
<script lang="ts">
	import type { ConnectionStatus } from '../lib/renderer/engine';
	import { waitingText } from './status';

	interface Props {
		connection: ConnectionStatus;
		port: string;
		host: string;
	}

	let { connection, port, host }: Props = $props();

	const text = $derived(waitingText(connection, host, port));
</script>

<div class="ks-waiting" role="status" aria-live="polite">
	<div class="ks-waiting-head">
		<span class="ks-waiting-lamp" class:is-open={connection === 'open'} aria-hidden="true"></span>
		<span class="ks-waiting-label">{connection === 'open' ? 'Connected' : 'Not connected'}</span>
	</div>
	<h1 class="ks-waiting-title">{text.title}</h1>
	<p class="ks-waiting-line">{text.line}</p>
	<p class="ks-waiting-hint">{text.hint}</p>
</div>
