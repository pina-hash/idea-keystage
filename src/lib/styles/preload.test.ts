import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { BUNDLED_FONTS, preloadFonts } from './preload';

describe('font preload', () => {
	it('names every face fonts.css bundles, and only those', () => {
		const css = readFileSync(resolve(process.cwd(), 'src/lib/styles/fonts.css'), 'utf8');
		const bundled = [...css.matchAll(/@fontsource\/([a-z-]+)\/latin-(\d+)\.css/g)].map(
			([, pkg, weight]) => `${weight} ${pkg}`
		);
		const named = BUNDLED_FONTS.map((f) => {
			const m = /^(\d+) 1em '([^']+)'$/.exec(f);
			expect(m).not.toBeNull();
			const [, weight = '', family = ''] = m ?? [];
			return `${weight} ${family.toLowerCase().replace(/ /g, '-')}`;
		});
		expect(named.sort()).toEqual(bundled.sort());
	});

	it('loads each face and never rejects, even when a load fails or throws', async () => {
		const asked: string[] = [];
		await expect(
			preloadFonts({
				load(font) {
					asked.push(font);
					if (font.includes('Orbitron')) throw new Error('bad');
					if (font.includes('600')) return Promise.reject(new Error('refused'));
					return Promise.resolve([]);
				},
			})
		).resolves.toBeUndefined();
		expect(asked).toEqual([...BUNDLED_FONTS]);
		await expect(preloadFonts(undefined)).resolves.toBeUndefined();
	});
});
