<!--
	The settings window: the navigation rail, the page, and the live preview.
	Until setup is finished (settings.first_run_done) the first-run wizard
	takes the rail's and the page's place; the preview stays beside it.
-->
<script lang="ts">
	import type { Component } from 'svelte';
	import { provideController, type SettingsController } from './state.svelte';
	import type { PageId } from './lib/nav';
	import NavRail from './parts/NavRail.svelte';
	import PreviewPane from './parts/PreviewPane.svelte';
	import Toasts from './parts/Toasts.svelte';
	import Loading from './parts/Loading.svelte';
	import FirstRun from './wizard/FirstRun.svelte';
	import Home from './pages/Home.svelte';
	import Keystrokes from './pages/Keystrokes.svelte';
	import Keyboard from './pages/Keyboard.svelte';
	import Mouse from './pages/Mouse.svelte';
	import Display from './pages/Display.svelte';
	import Obs from './pages/Obs.svelte';
	import SolidWorks from './pages/SolidWorks.svelte';
	import Sessions from './pages/Sessions.svelte';
	import Privacy from './pages/Privacy.svelte';
	import Hotkeys from './pages/Hotkeys.svelte';
	import Sound from './pages/Sound.svelte';
	import General from './pages/General.svelte';

	interface Props {
		ctl: SettingsController;
	}

	let { ctl }: Props = $props();
	// svelte-ignore state_referenced_locally
	provideController(ctl);

	const VIEWS: Record<PageId, Component> = {
		home: Home,
		keystrokes: Keystrokes,
		keyboard: Keyboard,
		mouse: Mouse,
		display: Display,
		obs: Obs,
		solidworks: SolidWorks,
		sessions: Sessions,
		privacy: Privacy,
		hotkeys: Hotkeys,
		sound: Sound,
		general: General,
	};

	// Every edit anywhere in the settings is saved a moment later.
	$effect(() => {
		const s = ctl.settings;
		if (s) ctl.draftChanged($state.snapshot(s));
	});

	const wizard = $derived(ctl.settings !== null && !ctl.settings.first_run_done);
	const View = $derived(VIEWS[ctl.page]);
	let pageEl = $state<HTMLElement | null>(null);

	// A new page starts at its top, and keyboard focus moves to its title.
	$effect(() => {
		void ctl.page;
		if (pageEl) {
			pageEl.scrollTop = 0;
			pageEl.querySelector<HTMLElement>('h1')?.focus({ preventScroll: true });
		}
	});
</script>

<svelte:window onhashchange={() => ctl.goFromHash(window.location.hash)} />

<div class="ks-plate st-app" class:is-wizard={wizard}>
	{#if ctl.loadError || !ctl.settings}
		<Loading error={ctl.loadError} retry={() => void ctl.init()} />
	{:else if wizard}
		<main class="st-wizard-col" id="main">
			<FirstRun />
		</main>
		<div class="st-preview-col"><PreviewPane /></div>
	{:else}
		<NavRail />
		<main class="st-page" id="main" bind:this={pageEl}>
			<div class="st-page-inner">
				<View />
			</div>
		</main>
		<div class="st-preview-col"><PreviewPane /></div>
	{/if}
	<Toasts />
</div>

<style>
	.st-app {
		display: grid;
		grid-template-columns: var(--st-rail-w) minmax(0, 1fr) var(--st-preview-w);
		height: 100vh;
		background-color: var(--plate-plate-bot);
		background-image: linear-gradient(
			to bottom,
			var(--plate-plate-top),
			var(--plate-plate-bot) 28rem
		);
		color: var(--plate-ink);
		font-family: var(--font-display);
	}
	.st-app.is-wizard {
		grid-template-columns: minmax(0, 1fr) var(--st-preview-w);
	}
	.st-page,
	.st-wizard-col {
		min-height: 0;
		overflow-y: auto;
		scroll-behavior: auto;
	}
	.st-wizard-col {
		display: flex;
		flex-direction: column;
		justify-content: safe center;
	}
	.st-page-inner {
		max-width: 920px;
		margin: 0 auto;
		padding: 1.4rem 2.25rem 3rem;
	}
	.st-preview-col {
		min-height: 0;
		height: 100vh;
	}
	@media (max-width: 1180px) {
		.st-app {
			grid-template-columns: 220px minmax(0, 1fr);
		}
		.st-app.is-wizard {
			grid-template-columns: minmax(0, 1fr);
		}
		.st-preview-col {
			display: none;
		}
	}
</style>
