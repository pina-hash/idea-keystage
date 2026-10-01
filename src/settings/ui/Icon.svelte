<!-- One line icon from icons.ts, drawn in the current text color. -->
<script lang="ts">
	import { ICONS, type IconName } from './icons';

	interface Props {
		name: IconName;
		size?: number;
		/** Filled shapes (play, record, stop) instead of outlines. */
		filled?: boolean;
	}

	let { name, size = 20, filled = false }: Props = $props();
</script>

<svg
	class="st-icon"
	width={size}
	height={size}
	viewBox="0 0 24 24"
	aria-hidden="true"
	focusable="false"
>
	<path
		d={ICONS[name]}
		fill={filled ? 'currentColor' : 'none'}
		stroke="currentColor"
		stroke-width="1.8"
		stroke-linecap="round"
		stroke-linejoin="round"
	/>
</svg>

<style>
	.st-icon {
		flex: none;
		display: block;
	}
</style>
