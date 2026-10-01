import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('.', import.meta.url));

// Two pages share one renderer: overlay.html (Host A, OBS Browser Sources, previews)
// and settings.html (the settings window). Assets are served from the site root,
// so `base` is '/' and the server maps /overlay/<route> to overlay.html.
export default defineConfig({
	base: '/',
	plugins: [svelte()],
	clearScreen: false,
	resolve: {
		alias: {
			$lib: resolve(root, 'src/lib'),
			$data: resolve(root, 'data'),
		},
	},
	server: {
		port: 5173,
		strictPort: true,
		host: '127.0.0.1',
	},
	build: {
		// OBS 30 embeds Chromium 103, OBS 31 and 32 embed Chromium 127 (docs/DECISIONS.md item 5).
		// One build serves the overlay to both, so the floor is Chromium 103.
		target: 'chrome103',
		cssTarget: 'chrome103',
		outDir: 'dist',
		emptyOutDir: true,
		assetsInlineLimit: 0,
		rollupOptions: {
			input: {
				overlay: resolve(root, 'overlay.html'),
				settings: resolve(root, 'settings.html'),
			},
		},
	},
	test: {
		environment: 'jsdom',
		include: ['src/**/*.test.ts', 'tests/unit/**/*.test.ts'],
	},
});
