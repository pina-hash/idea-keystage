/**
 * Measured, not argued: the settings window's text and boundary colors in
 * both registers (IDEA dark and Space White), read from the stylesheets the
 * window loads, and the copy rules (American spelling, no em dashes, no
 * leftover markers, no grid backgrounds, only the three bundled fonts).
 */
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { contrastOf } from '../lib/themes/color';

const ROOT = resolve(__dirname, '../..');
const read = (p: string) => readFileSync(resolve(ROOT, p), 'utf8');

/** `--name: value;` pairs of the first rule whose selector matches. */
function block(css: string, selectorStart: string): Record<string, string> {
	const at = css.indexOf(selectorStart);
	if (at < 0) throw new Error(`no block ${selectorStart}`);
	const open = css.indexOf('{', at);
	const close = css.indexOf('}', open);
	const out: Record<string, string> = {};
	for (const m of css.slice(open + 1, close).matchAll(/(--[\w-]+):\s*([^;]+);/g))
		out[m[1]!] = m[2]!.trim();
	return out;
}

const plate = read('src/lib/styles/plate.css');
const settings = read('src/settings/settings.css');
// plate.css declares the shape block first and the IDEA color block second, both on `.ks-plate {`.
const ideaColors = (() => {
	const first = plate.indexOf('.ks-plate {');
	const second = plate.indexOf('.ks-plate {', first + 1);
	return block(plate.slice(second), '.ks-plate {');
})();
const whiteColors = block(plate, ":root[data-plate='space-white'] .ks-plate,");
const ideaExtra = block(settings, '.ks-plate {');
const whiteExtra = block(settings, ":root[data-plate='space-white'] .ks-plate,");

const REGISTERS = {
	dark: { ...ideaColors, ...ideaExtra },
	light: { ...ideaColors, ...ideaExtra, ...whiteColors, ...whiteExtra },
} as const;

/** Text pairs the window draws: [ink token or color, ground token or color, where]. */
const TEXT_PAIRS: [string, string, string][] = [
	['--plate-ink', '--plate-panel-top', 'card text'],
	['--plate-ink', '--plate-panel-bot', 'card text'],
	['--plate-ink-2', '--plate-panel-top', 'card help'],
	['--plate-ink-2', '--plate-panel-bot', 'card help'],
	['--plate-caption', '--plate-plate-top', 'section label on the page'],
	['--plate-caption', '--plate-plate-bot', 'section label on the page'],
	['--plate-ink-2', '--plate-plate-bot', 'page lead'],
	['--plate-ink', '--plate-plate-top', 'rail'],
	['--plate-btn-ink', '--plate-face-top', 'key word'],
	['--plate-btn-ink', '--plate-face-bot', 'key word'],
	['--plate-btn-ink', '--plate-lit-top', 'lit key word'],
	['--plate-ink-2', '--plate-face-bot', 'card-key description'],
	['--plate-mark-ink', '--plate-lit-top', 'In use on a lit card'],
	['--plate-accent', '--plate-panel-top', 'accent words in cards'],
	['--plate-primary-ink', '--plate-primary-face', 'primary key'],
	['--plate-danger-ink', '--plate-face-top', 'danger key'],
	['--plate-danger-ink', '--plate-face-bot', 'danger key'],
	['--plate-danger-ink', '--plate-panel-bot', 'field errors'],
	['--plate-ink', '--plate-chip-top', 'chips and tiles'],
	['--plate-ink', '--plate-chip-bot', 'chips and tiles'],
	['--plate-ink-2', '--plate-chip-bot', 'tile detail'],
	['--plate-warn-ink', '--plate-chip-bot', 'warning lead word'],
	['--plate-warn-ink', '--plate-chip-top', 'warning lead word'],
	['--plate-warn-ink', '--plate-panel-top', 'unused import line'],
	['--plate-warn-ink', '--plate-panel-bot', 'unused import line'],
	['--plate-ok-ink', '--plate-chip-bot', 'done lead word'],
	['--plate-info-ink', '--plate-chip-bot', 'note lead word'],
	['--plate-danger-ink', '--plate-chip-bot', 'problem lead word'],
	['--plate-ink', '--plate-well-face', 'field text'],
	['--plate-ink-2', '--plate-well-face', 'placeholder'],
	['--plate-ink', '--plate-list-face', 'table text'],
	['--plate-ink-2', '--plate-list-face', 'table context'],
	['--plate-caption', '--plate-list-face', 'table headings'],
	['--plate-screen-ink', '--plate-screen-top', 'readout'],
	['--plate-screen-ink', '--plate-screen-bot', 'readout'],
	['#f0b36a', '--plate-screen-top', 'paused readout'],
	['#f39a9a', '--plate-screen-top', 'problem readout'],
	['--plate-sel-ink', '--plate-sel-face', 'selected session'],
	['--plate-caption', '--plate-inset-top', 'preview labels'],
	['--plate-ink-2', '--plate-inset-bot', 'preview help'],
];

/** Load-bearing edges: a control's outer hairline against the grounds it sits on. */
const EDGE_PAIRS: [string, string][] = [
	['--plate-hair', '--plate-panel-bot'],
	['--plate-hair', '--plate-panel-top'],
	['--plate-hair', '--plate-plate-bot'],
];

function value(reg: Record<string, string>, token: string): string {
	if (token.startsWith('#')) return token;
	let v = reg[token];
	for (let i = 0; i < 4 && v && v.startsWith('var('); i++) v = reg[v.slice(4, -1).trim()];
	if (!v) throw new Error(`missing ${token}`);
	return v;
}

describe('contrast in both registers', () => {
	for (const [name, reg] of Object.entries(REGISTERS)) {
		it(`${name}: every text pair reads at 4.5 to 1 or better`, () => {
			const failures: string[] = [];
			for (const [fg, bg, where] of TEXT_PAIRS) {
				const r = contrastOf(value(reg, fg), value(reg, bg));
				if (r < 4.5) failures.push(`${where}: ${fg} on ${bg} is ${r.toFixed(2)}`);
			}
			expect(failures).toEqual([]);
		});

		it(`${name}: control edges hold 3 to 1`, () => {
			for (const [edge, ground] of EDGE_PAIRS) {
				expect(
					contrastOf(value(reg, edge), value(reg, ground)),
					`${edge} on ${ground}`
				).toBeGreaterThanOrEqual(3);
			}
		});
	}
});

function files(dir: string, out: string[] = []): string[] {
	for (const name of readdirSync(dir)) {
		const p = join(dir, name);
		if (statSync(p).isDirectory()) files(p, out);
		else if (/\.(ts|svelte|css|html)$/.test(name)) out.push(p);
	}
	return out;
}

const SOURCES = [...files(resolve(ROOT, 'src/settings')), resolve(ROOT, 'settings.html')].filter(
	(p) => !p.endsWith('style.test.ts')
);

describe('copy and design rules', () => {
	it('no em or en dashes, no leftover markers, American spelling', () => {
		const bad: string[] = [];
		const words =
			/\b(colour|behaviour|centre|favour|licence|grey|organise|recognise|analyse|catalogue|cancelled)\b/i;
		for (const f of SOURCES) {
			const text = readFileSync(f, 'utf8');
			text.split('\n').forEach((line, i) => {
				if (/[–—]/.test(line)) bad.push(`${f}:${i + 1} dash`);
				if (/\b(TODO|FIXME|XXX|lorem)\b/i.test(line)) bad.push(`${f}:${i + 1} marker`);
				if (words.test(line)) bad.push(`${f}:${i + 1} spelling`);
			});
		}
		expect(bad).toEqual([]);
	});

	it('only the three bundled fonts are named', () => {
		const allowed = new Set([
			'rajdhani',
			'share tech mono',
			'orbitron',
			'segoe ui',
			'consolas',
			'sans-serif',
			'monospace',
		]);
		for (const f of SOURCES) {
			for (const m of readFileSync(f, 'utf8').matchAll(/font-family:\s*([^;}\n]+)/g)) {
				const families = m[1]!
					.split(',')
					.map((s) =>
						s
							.trim()
							.replace(/^['"]|['"]$/g, '')
							.toLowerCase()
					)
					.filter(
						(s) =>
							!s.startsWith('var(') && !s.startsWith('{') && !s.includes('$') && s !== 'inherit'
					);
				for (const fam of families) expect(allowed.has(fam), `${f}: ${fam}`).toBe(true);
			}
		}
	});

	it('no grid backgrounds and nothing loaded from the network', () => {
		for (const f of SOURCES) {
			const text = readFileSync(f, 'utf8');
			expect(
				/repeating-linear-gradient|background-size:\s*\d+px\s+\d+px\s*;\s*\/\*\s*grid/i.test(text),
				f
			).toBe(false);
			expect(
				/https?:\/\/(?!127\.0\.0\.1|localhost|www\.w3\.org)[^\s'"`)]*\.(css|js|woff2?)/i.test(text),
				f
			).toBe(false);
			expect(/fonts\.googleapis|cdn\./i.test(text), f).toBe(false);
		}
	});

	it('nothing in the window logs', () => {
		for (const f of SOURCES.filter((p) => !p.endsWith('.test.ts'))) {
			expect(/console\.(log|info|debug|warn|error)\(/.test(readFileSync(f, 'utf8')), f).toBe(false);
		}
	});
});
