/// <reference types="node" />
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { parseColor, flatten, contrastRatio, type Rgba } from './color';
import {
	CONTRAST_PAIRS,
	FILE_THEMES,
	LENGTH_TOKENS,
	TEST_BACKGROUNDS,
	THEMES,
	THEME_TOKENS,
	customThemeVars,
	resolveTheme,
	type ThemeVars,
} from './themes';
import { THEME_IDS } from '../protocol';

// Vitest runs from the repository root (vite.config.ts). CSS imports are
// stubbed in tests, so the theme files are read from disk.
const themeDir = resolve(process.cwd(), 'src/lib/themes');
const readTheme = (name: string) => readFileSync(resolve(themeDir, `${name}.css`), 'utf8');
const indexCss = readTheme('index');

/** Reads the `--ks-*` declarations of one theme file. */
function readThemeVars(id: string): Record<string, string> {
	const css = readTheme(id);
	const block = new RegExp(`\\.ks-stage\\[data-theme='${id}'\\]\\s*\\{([^}]*)\\}`).exec(css);
	if (!block || !block[1]) throw new Error(`${id}.css has no .ks-stage[data-theme='${id}'] block`);
	const out: Record<string, string> = {};
	for (const m of block[1].matchAll(/(--ks-[a-z0-9-]+)\s*:\s*([^;]+);/g)) {
		if (m[1] && m[2]) out[m[1]] = m[2].trim();
	}
	return out;
}

function worstContrast(vars: Record<string, string>, ink: string, layers: readonly string[]) {
	let worst = Infinity;
	let where = '';
	for (const bg of TEST_BACKGROUNDS) {
		const stack: Rgba[] = [parseColor(bg) as Rgba];
		for (const token of layers) {
			const c = parseColor(vars[token] ?? '');
			if (!c) throw new Error(`${token} is not a color: ${vars[token]}`);
			stack.push(c);
		}
		const face = flatten(stack);
		const inkColor = parseColor(vars[ink] ?? '');
		if (!inkColor) throw new Error(`${ink} is not a color: ${vars[ink]}`);
		const text = flatten([face, inkColor]);
		const ratio = contrastRatio(text, face);
		if (ratio < worst) {
			worst = ratio;
			where = bg;
		}
	}
	return { worst, where };
}

describe('theme registry', () => {
	it('lists every theme id of the protocol once, with a name and a description', () => {
		expect(THEMES.map((t) => t.id).sort()).toEqual([...THEME_IDS].sort());
		for (const t of THEMES) {
			expect(t.name.length).toBeGreaterThan(0);
			expect(t.description.length).toBeGreaterThan(0);
			expect(t.description).not.toMatch(/\u2014/);
		}
	});

	it('imports every theme file from index.css', () => {
		for (const id of FILE_THEMES) expect(indexCss).toContain(`@import './${id}.css';`);
	});

	it('falls back to IDEA for an unknown theme', () => {
		expect(resolveTheme({ theme: 'nope', custom: null })).toEqual({ id: 'idea', style: '' });
		expect(resolveTheme({ theme: 'carbon', custom: null })).toEqual({ id: 'carbon', style: '' });
	});
});

describe.each(FILE_THEMES)('theme %s', (id) => {
	const vars = readThemeVars(id);

	it('declares every token, colors as literal colors', () => {
		for (const token of THEME_TOKENS) {
			expect(vars[token], `${id} ${token}`).toBeDefined();
			if (LENGTH_TOKENS.includes(token)) {
				expect(vars[token]).toMatch(/^\d+(\.\d+)?px$/);
			} else {
				expect(parseColor(vars[token] ?? ''), `${id} ${token} = ${vars[token]}`).not.toBeNull();
			}
		}
		expect(Object.keys(vars).sort()).toEqual([...THEME_TOKENS].sort());
	});

	it.each(CONTRAST_PAIRS)('$label clears its minimum over light and dark video', (pair) => {
		const { worst, where } = worstContrast(vars, pair.ink, pair.layers);
		expect(
			worst,
			`${id}: ${pair.label} is ${worst.toFixed(2)}:1 over ${where}`
		).toBeGreaterThanOrEqual(pair.min);
	});
});

describe('custom theme', () => {
	const inputs = [
		{ accent: '#78b870', face: '#1c1f1d', ink: '#e7eae8', glow: '#78b870' },
		{ accent: '#ff3366', face: '#f4f4f4', ink: '#111111', glow: '#ff3366' },
		// Low-contrast choices: the ink must be moved, never shown unreadable.
		{ accent: '#777777', face: '#808080', ink: '#8a8a8a', glow: '#ffffff' },
		{ accent: '#ffff00', face: '#ffffff', ink: '#ffffaa', glow: '#000000' },
		{ accent: '#000080', face: '#000000', ink: '#000033', glow: '#0000ff' },
	];

	it.each(inputs)('keeps every pair readable for %o', (custom) => {
		const { vars } = customThemeVars(custom);
		for (const pair of CONTRAST_PAIRS) {
			const { worst } = worstContrast(vars, pair.ink, pair.layers);
			expect(worst, `${pair.label} ${worst.toFixed(2)}`).toBeGreaterThanOrEqual(pair.min - 0.01);
		}
	});

	it('reports when an ink was adjusted, and when an input was not a color', () => {
		expect(customThemeVars(inputs[0] ?? null).adjusted).toBe(false);
		expect(customThemeVars(inputs[2] ?? null).adjusted).toBe(true);
		expect(
			customThemeVars({ accent: 'green', face: '#000', ink: '#fff', glow: '#fff' }).invalid
		).toBe(true);
	});

	it('produces every token', () => {
		const { vars } = customThemeVars(null);
		const keys: (keyof ThemeVars)[] = [...THEME_TOKENS];
		for (const k of keys) expect(vars[k]).toBeTruthy();
		expect(resolveTheme({ theme: 'custom', custom: null }).style).toContain('--ks-plate-top:');
	});
});
