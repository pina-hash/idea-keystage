<!--
	The app's mark, drawn after the app icon (a chamfered dark plate, a light
	keycap with a K, a green lit line) as vector, beside the name.
-->
<div class="st-mark">
	<svg class="st-mark-icon" viewBox="0 0 48 48" width="44" height="44" aria-hidden="true">
		<defs>
			<linearGradient id="st-mark-plate" x1="0" y1="0" x2="0" y2="1">
				<stop offset="0" stop-color="#3a403c" />
				<stop offset="1" stop-color="#1b1f1d" />
			</linearGradient>
			<linearGradient id="st-mark-cap" x1="0" y1="0" x2="0" y2="1">
				<stop offset="0" stop-color="#f4f6f5" />
				<stop offset="1" stop-color="#cfd5d2" />
			</linearGradient>
		</defs>
		<path
			d="M11 2h26l9 9v26l-9 9H11l-9-9V11z"
			fill="url(#st-mark-plate)"
			stroke="#6f7b73"
			stroke-width="1.2"
		/>
		<rect x="13" y="9.5" width="22" height="21" rx="3.2" fill="#0e110f" />
		<rect x="14.5" y="10.5" width="19" height="18" rx="2.6" fill="url(#st-mark-cap)" />
		<path
			d="M20 14.5v10M20 20l6-5.5M21.6 18.6 26.4 24.5"
			fill="none"
			stroke="#141816"
			stroke-width="2.6"
			stroke-linecap="round"
			stroke-linejoin="round"
		/>
		<rect x="12" y="35.5" width="24" height="2.2" rx="1.1" fill="#8fe08a" />
		<rect
			x="12"
			y="35.5"
			width="24"
			height="2.2"
			rx="1.1"
			fill="#78b870"
			opacity="0.6"
			style="filter: blur(1.2px)"
		/>
	</svg>
	<div class="st-mark-words">
		<span class="st-mark-org">IDEA</span>
		<span class="st-mark-name">Keystage</span>
	</div>
</div>

<style>
	.st-mark {
		display: flex;
		align-items: center;
		gap: 0.7rem;
	}
	.st-mark-icon {
		flex: none;
		filter: drop-shadow(0 2px 3px var(--plate-drop));
	}
	.st-mark-words {
		display: flex;
		flex-direction: column;
		line-height: 1.05;
	}
	.st-mark-org {
		font-family: var(--font-mono);
		font-size: 0.72rem;
		letter-spacing: 0.32em;
		color: var(--plate-accent);
	}
	.st-mark-name {
		font-family: var(--font-title);
		font-weight: 700;
		font-size: 1.12rem;
		letter-spacing: 0.08em;
		text-transform: uppercase;
		color: var(--plate-ink);
	}
</style>
