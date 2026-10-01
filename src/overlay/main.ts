/**
 * Overlay entry (overlay.html): Host A (?host=native), OBS Browser Sources
 * (/overlay/<route>) and any browser. Reads the page options from the URL,
 * picks a transport and mounts the renderer.
 */
import { mount } from 'svelte';
import { createTransport } from '../lib/renderer/transport';
import { RendererStore } from '../lib/renderer/store.svelte';
import { isInObs, parsePageOptions } from '../lib/renderer/options';
import { preloadFonts } from '../lib/styles/preload';
import App from './App.svelte';
import './overlay.css';

const inObs = isInObs(window);
const options = parsePageOptions(window.location, inObs);
const store = new RendererStore({ route: options.route, overrides: options.overrides });
const target = document.getElementById('app');

if (target) {
	// While the app that served this page is still there (UX-3).
	void preloadFonts(document.fonts);
	document.documentElement.dataset.route = options.route;
	mount(App, { target, props: { store, options, inObs } });
	store.connect(createTransport(options, { location: window.location }));
	window.addEventListener('pagehide', () => store.destroy());
}
