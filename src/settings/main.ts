/**
 * Settings window entry (settings.html). Connects to the app (or to the mock
 * backend outside the app), loads the settings and mounts the window.
 */
import { mount } from 'svelte';
import '../lib/styles/fonts.css';
import '../lib/styles/tokens.css';
import '../lib/styles/plate.css';
import './settings.css';
import App from './App.svelte';
import { connectIpc } from './ipc';
import { applyAppearance, readAppearance } from './lib/appearance';
import { pageFromLocation } from './lib/nav';
import { SettingsController } from './state.svelte';

async function start(): Promise<void> {
	const target = document.getElementById('app');
	if (!target) return;
	const appearance = readAppearance(window.location.search);
	applyAppearance(appearance);
	const ipc = await connectIpc(window.location.search);
	const ctl = new SettingsController(ipc);
	ctl.page = pageFromLocation(window.location.hash, window.location.search);
	ctl.appearance = appearance;
	mount(App, { target, props: { ctl } });
	window.addEventListener('pagehide', () => {
		void ctl.flush();
		ctl.destroy();
	});
	await ctl.init();
}

void start();
