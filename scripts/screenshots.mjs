#!/usr/bin/env node
// Screenshot harness: every overlay route, theme and state, and every
// settings page and state, with a contact sheet.
//
//   node scripts/screenshots.mjs [--only=overlay|settings] [--states=a,b]
//        [--routes=a,b] [--themes=a,b] [--res=1080p,4k] [--concurrency=4]
//        [--build] [--out=.work/shots/final]
//
// Overlay (Host B, the OBS Browser Source page): for each shot the demo
// server (cargo build --release -p keystage-server --example demo, run
// against dist/) feeds the real overlay through its WebSocket. Shots are
// taken at 1920x1080 (scale 1) and 3840x2160 (scale=2 on the URL, as an OBS
// 4K canvas would use), with a transparent page that is then composited over
// a light gray and a dark background (the gray and dark the theme contrast
// tests use, src/lib/themes/themes.ts TEST_BACKGROUNDS).
//
// States:
//   active        the SolidWorks script; captions come from the data file
//                 through the Processor (this harness names no command)
//   general       the general profile: shortcuts only, typing masked
//   paused        the Paused indicator
//   password      hidden for a password field
//   disconnected  the app went away while a browser had the page open
//   empty         connected, nothing pressed
// Scripted states are captured at a moment found in the message stream
// (scripts/lib/harness.mjs MOMENTS), then the demo is frozen (SIGSTOP) so no
// new input arrives while the page settles: fonts, a fixed delay, then two
// animation frames.
//
// Settings: settings.html from `vite preview` of dist/ with the in-memory
// mock backend, every page in both registers and every mock scenario, at
// 1920x1080 and at 1920x1080 with deviceScaleFactor 2.
//
// Output: <out>/overlay/<state>/<route>-<theme>-<res>-<bg>.png,
// <out>/settings/<name>-<1x|2x>.png, thumbnails, manifest.json and index.html.
// Exits 1 if any shot failed. Chromium comes from Playwright (pinned
// devDependency). Privacy: synthetic input only.

import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { chromium } from 'playwright';
import {
	MOMENTS,
	ROOT,
	buildDemo,
	ensureFrontEnd,
	observeDemo,
	pool,
	portIsFree,
	readConstList,
	sleep,
	startDemo,
	startPreview,
} from './lib/harness.mjs';

const args = Object.fromEntries(
	process.argv.slice(2).map((a) => {
		const [k, v] = a.replace(/^--/, '').split('=');
		return [k, v ?? 'true'];
	})
);
const list = (name, all) => {
	if (!args[name]) return all;
	const want = args[name].split(',').map((s) => s.trim());
	const unknown = want.filter((w) => !all.includes(w));
	if (unknown.length) {
		console.error(`--${name}: unknown ${unknown.join(', ')} (known: ${all.join(', ')})`);
		process.exit(2);
	}
	return all.filter((x) => want.includes(x));
};

const OUT = resolve(ROOT, args.out ?? '.work/shots/final');
const CONCURRENCY = Math.max(1, Number(args.concurrency ?? 4) || 4);
const ONLY = args.only ?? 'all';
if (!['all', 'overlay', 'settings'].includes(ONLY)) {
	console.error('--only must be overlay or settings');
	process.exit(2);
}

const protocolSrc = readFileSync(join(ROOT, 'src/lib/protocol.ts'), 'utf8');
const ALL_ROUTES = readConstList(protocolSrc, 'ROUTES');
const ALL_THEMES = readConstList(protocolSrc, 'THEME_IDS');
const SETTINGS_PAGES = readConstList(
	readFileSync(join(ROOT, 'src/settings/lib/nav.ts'), 'utf8'),
	'PAGES'
);
const SCENARIOS = readConstList(
	readFileSync(join(ROOT, 'src/settings/mock/fixtures.ts'), 'utf8'),
	'SCENARIOS'
);

const ENABLE = ['--enable', 'keyboard,mouse,cursor'];
const STATES = {
	active: {
		title: 'Active, SolidWorks profile with captions',
		demo: ['--script', 'solidworks', '--rate', '2', ...ENABLE],
		moment: (route) => (route === 'keys' ? 'typed' : 'drag'),
	},
	general: {
		title: 'General profile: shortcuts only, typing masked',
		demo: ['--script', 'general', '--rate', '1', ...ENABLE],
		moment: () => 'combo3',
	},
	paused: { title: 'Paused', demo: ['--script', 'idle', '--state', 'paused', ...ENABLE] },
	password: {
		title: 'Hidden for a password field',
		demo: ['--script', 'idle', '--state', 'password', ...ENABLE],
	},
	disconnected: {
		title: 'Disconnected, in a browser',
		demo: ['--script', 'idle', ...ENABLE],
		disconnect: true,
	},
	empty: { title: 'Connected, nothing to show', demo: ['--script', 'idle', ...ENABLE] },
};
const RESOLUTIONS = {
	'1080p': { title: '1920x1080, scale 1', viewport: { width: 1920, height: 1080 }, scale: 1 },
	'4k': { title: '3840x2160, scale 2', viewport: { width: 3840, height: 2160 }, scale: 2 },
};
const BACKGROUNDS = { light: '#d3d6da', dark: '#262a2e' };
const THUMB_WIDTH = 480;

/** Delays after the capture moment (ms). The longest overlay animation is 220 ms. */
const SETTLE_MS = 260;
const STATIC_SETTLE_MS = 700;
const CARD_SETTLE_MS = 1300;
const SETTINGS_SETTLE_MS = 1600;
const SETTINGS_ACT_SETTLE_MS = 700;

const states = list('states', Object.keys(STATES));
const routes = list('routes', ALL_ROUTES);
const themes = list('themes', ALL_THEMES);
const resolutions = list('res', Object.keys(RESOLUTIONS));
const filtered = ['states', 'routes', 'themes', 'res', 'only'].some((k) => args[k]);

/* ------------------------------------------------------------------ setup */

if (!filtered) {
	rmSync(join(OUT, 'overlay'), { recursive: true, force: true });
	rmSync(join(OUT, 'settings'), { recursive: true, force: true });
	rmSync(join(OUT, 'thumbs'), { recursive: true, force: true });
}
mkdirSync(OUT, { recursive: true });
ensureFrontEnd({ force: args.build === 'true' });
const demoExe = ONLY === 'settings' ? null : buildDemo();

const manifestPath = join(OUT, 'manifest.json');
const manifest =
	existsSync(manifestPath) && filtered ? JSON.parse(readFileSync(manifestPath, 'utf8')) : {};
manifest.overlay ??= {};
manifest.settings ??= {};
const failures = [];

const browser = await chromium.launch();
const chromiumVersion = browser.version();
console.log(`Chromium ${chromiumVersion} (Playwright, headless)`);

// One page composites every capture: transparent PNG in, PNG over each
// background plus a JPEG thumbnail out, all drawn with canvas.
const compositor = await (await browser.newContext()).newPage();
async function composite(png, backgrounds) {
	return compositor.evaluate(
		async ({ b64, backgrounds, thumbWidth }) => {
			const toB64 = (blob) =>
				new Promise((res, rej) => {
					const r = new FileReader();
					r.onload = () => res(String(r.result).split(',')[1]);
					r.onerror = () => rej(r.error);
					r.readAsDataURL(blob);
				});
			const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
			const bmp = await createImageBitmap(new Blob([bytes], { type: 'image/png' }));
			const out = [];
			for (const bg of backgrounds) {
				const c = new OffscreenCanvas(bmp.width, bmp.height);
				const g = c.getContext('2d');
				if (bg) {
					g.fillStyle = bg;
					g.fillRect(0, 0, c.width, c.height);
				}
				g.drawImage(bmp, 0, 0);
				const t = new OffscreenCanvas(
					thumbWidth,
					Math.round((thumbWidth * bmp.height) / bmp.width)
				);
				const tg = t.getContext('2d');
				tg.imageSmoothingQuality = 'high';
				tg.drawImage(c, 0, 0, t.width, t.height);
				out.push({
					png: bg ? await toB64(await c.convertToBlob({ type: 'image/png' })) : null,
					jpg: await toB64(await t.convertToBlob({ type: 'image/jpeg', quality: 0.86 })),
				});
			}
			return out;
		},
		{ b64: png.toString('base64'), backgrounds, thumbWidth: THUMB_WIDTH }
	);
}

function writeOut(rel, b64) {
	const path = join(OUT, rel);
	mkdirSync(dirname(path), { recursive: true });
	writeFileSync(path, Buffer.from(b64, 'base64'));
}

const thumbOf = (rel) => join('thumbs', rel.replace(/\.png$/, '.jpg'));

/** Fonts, then two animation frames (the frame after next is settled). */
async function settleFrames(page) {
	await page.evaluate(() =>
		document.fonts.ready.then(
			() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(() => r(true))))
		)
	);
}

/** Page errors go to `sink`; console errors that `expected` accepts go to `warnings`. */
function collectErrors(page, sink, warnings = [], expected = () => false) {
	page.on('pageerror', (e) => sink.push(String(e)));
	page.on('console', (m) => {
		if (m.type() !== 'error') return;
		(expected(m.text()) ? warnings : sink).push(m.text());
	});
}

/* ---------------------------------------------------------------- overlay */

const contexts = {};
async function contextFor(res) {
	contexts[res] ??= await browser.newContext({
		viewport: RESOLUTIONS[res].viewport,
		deviceScaleFactor: 1,
	});
	return contexts[res];
}

async function overlayShot(job) {
	const { state, route, theme, res } = job;
	const spec = STATES[state];
	const r = RESOLUTIONS[res];
	const errors = [];
	const warnings = [];
	const started = performance.now();
	let port = 0;
	if (spec.disconnect && (await portIsFree(47820))) port = 47820;
	const demo = await startDemo(demoExe, spec.demo, { port });
	let observer = null;
	const page = await (await contextFor(res)).newPage();
	// A disconnected page keeps retrying, and anything it requests after the
	// server is gone (a font the card needs first, for one) is refused.
	collectErrors(
		page,
		errors,
		warnings,
		(t) => !!spec.disconnect && /WebSocket connection|ERR_CONNECTION_REFUSED/.test(t)
	);
	try {
		const momentName = spec.moment ? spec.moment(route) : null;
		if (momentName) observer = await observeDemo(demo.port);
		const query = new URLSearchParams({ theme });
		if (r.scale !== 1) query.set('scale', String(r.scale));
		await page.goto(`http://127.0.0.1:${demo.port}/overlay/${route}?${query}`, {
			waitUntil: 'load',
		});
		await page.waitForFunction(
			(t) => {
				const stage = document.querySelector('.ks-stage');
				return (
					!!stage &&
					stage.getAttribute('data-theme') === t &&
					!document.querySelector('.ks-waiting')
				);
			},
			theme,
			{ timeout: 15_000 }
		);
		await page.evaluate(() => document.fonts.ready.then(() => true));
		if (momentName) {
			const reached = await observer.waitUntil(MOMENTS[momentName], 25_000);
			if (!reached) throw new Error(`capture moment "${momentName}" never came`);
			await sleep(SETTLE_MS);
			demo.stop();
		} else if (spec.disconnect) {
			await demo.kill();
			await page.waitForFunction(
				() => /again/.test(document.querySelector('.ks-waiting')?.textContent ?? ''),
				null,
				{ timeout: 15_000 }
			);
			await sleep(CARD_SETTLE_MS);
		} else {
			await sleep(STATIC_SETTLE_MS);
		}
		await settleFrames(page);
		const png = await page.screenshot({ type: 'png', omitBackground: true });
		const bgs = Object.keys(BACKGROUNDS);
		const out = await composite(
			png,
			bgs.map((b) => BACKGROUNDS[b])
		);
		const files = {};
		bgs.forEach((bg, i) => {
			const rel = join('overlay', state, `${route}-${theme}-${res}-${bg}.png`);
			writeOut(rel, out[i].png);
			writeOut(thumbOf(rel), out[i].jpg);
			files[bg] = rel;
		});
		manifest.overlay[`${state}/${route}/${theme}/${res}`] = {
			state,
			route,
			theme,
			res,
			moment: momentName,
			files,
			errors,
			warnings,
			ms: Math.round(performance.now() - started),
		};
		if (errors.length)
			failures.push(`${state}/${route}/${theme}/${res}: page errors: ${errors.join(' | ')}`);
	} catch (e) {
		failures.push(`${state}/${route}/${theme}/${res}: ${e.message}`);
	} finally {
		observer?.close();
		await page.close().catch(() => {});
		await demo.kill();
	}
}

/* --------------------------------------------------------------- settings */

async function clickButton(page, name) {
	await page.getByRole('button', { name, exact: true }).first().click();
	await page.waitForTimeout(250);
}

/** One settings shot: query, page hash, optional actions, scroll capture. */
function settingsShots() {
	const shots = [];
	for (const p of SETTINGS_PAGES)
		shots.push({ name: `page-${p}`, query: '', hash: p, scroll: true });
	for (const p of SETTINGS_PAGES)
		shots.push({ name: `light-${p}`, query: 'appearance=light', hash: p, scroll: true });
	// Where each mock scenario shows; a scenario added later still gets Home.
	const SCENARIO_PAGES = {
		paused: ['home'],
		'obs-error': ['home', 'obs'],
		'port-busy': ['home', 'obs'],
		empty: ['home', 'display', 'sessions', 'obs'],
	};
	for (const sc of SCENARIOS) {
		if (sc === 'normal') continue;
		if (sc === 'first-run') {
			for (let step = 1; step <= 4; step++) {
				shots.push({
					name: `state-first-run-${step}`,
					query: 'state=first-run',
					hash: '',
					act: async (pg) => {
						for (let i = 1; i < step; i++) await clickButton(pg, 'Next');
					},
				});
			}
			continue;
		}
		if (sc === 'import-warnings') {
			shots.push({
				name: 'state-import-warnings',
				query: `state=${sc}`,
				hash: 'solidworks',
				act: importPreview,
			});
			continue;
		}
		for (const p of SCENARIO_PAGES[sc] ?? ['home'])
			shots.push({ name: `state-${sc}-${p}`, query: `state=${sc}`, hash: p });
	}
	shots.push({ name: 'import-clean', query: '', hash: 'solidworks', act: importPreview });
	shots.push({
		name: 'sessions-detail',
		query: '',
		hash: 'sessions',
		act: async (pg) => {
			await pg.locator('.st-session').first().click();
			await pg.waitForSelector('.st-markers', { timeout: 5000 });
			await pg.locator('.st-markers').scrollIntoViewIfNeeded();
		},
	});
	shots.push({
		name: 'sessions-export',
		query: '',
		hash: 'sessions',
		act: async (pg) => {
			await pg.locator('.st-session').first().click();
			await pg.waitForSelector('.st-exports', { timeout: 5000 });
			await pg.locator('.st-detail-foot').evaluate((el) => el.scrollIntoView({ block: 'end' }));
		},
	});
	shots.push({
		name: 'hotkeys-conflict',
		query: '',
		hash: 'hotkeys',
		act: async (pg) => {
			await pg.locator('.st-hk-key').first().click();
			for (const k of ['Control', 'Alt', 'Shift']) await pg.keyboard.down(k);
			await pg.keyboard.press('KeyK');
			for (const k of ['Shift', 'Alt', 'Control']) await pg.keyboard.up(k);
		},
	});
	shots.push({
		name: 'keystrokes-custom-theme',
		query: '',
		hash: 'keystrokes',
		act: async (pg) => {
			await pg.getByRole('radio', { name: /Custom/ }).click();
			await pg.waitForTimeout(300);
			// A dark ink on the dark face, so the adjusted-ink notice shows.
			const ink = pg.getByRole('textbox', { name: 'Ink color', exact: true });
			await ink.fill('#33403a');
			await ink.press('Enter');
			await pg.waitForTimeout(300);
			await pg
				.getByRole('textbox', { name: 'Glow color', exact: true })
				.evaluate((el) => el.scrollIntoView({ block: 'center' }));
		},
	});
	shots.push({
		name: 'preview-live-input',
		query: '',
		hash: 'keystrokes',
		act: async (pg) => {
			await pg.getByRole('switch', { name: 'Live input' }).click();
			await pg.waitForTimeout(2500);
		},
	});
	return shots;
}

async function importPreview(pg) {
	await clickButton(pg, 'Load .txt or .sldreg file');
	await pg.waitForSelector('.st-preview-result', { timeout: 8000 });
	await pg.locator('.st-preview-result').scrollIntoViewIfNeeded();
}

async function settingsRun(base) {
	const shots = settingsShots();
	for (const dpr of [1, 2]) {
		const ctx = await browser.newContext({
			viewport: { width: 1920, height: 1080 },
			deviceScaleFactor: dpr,
		});
		await ctx.grantPermissions(['clipboard-read', 'clipboard-write'], { origin: base });
		await pool(shots, CONCURRENCY, async (shot) => {
			const page = await ctx.newPage();
			const errors = [];
			collectErrors(page, errors);
			const key = `${shot.name}/${dpr}x`;
			try {
				const url = `${base}/settings.html${shot.query ? `?${shot.query}` : ''}${shot.hash ? `#${shot.hash}` : ''}`;
				await page.goto(url, { waitUntil: 'networkidle' });
				await page.waitForSelector('.st-app', { timeout: 15_000 });
				await page.evaluate(() => document.fonts.ready.then(() => true));
				await page.waitForTimeout(SETTINGS_SETTLE_MS);
				if (shot.act) {
					await shot.act(page);
					await page.waitForTimeout(SETTINGS_ACT_SETTLE_MS);
				}
				const files = [];
				const capture = async (suffix) => {
					await settleFrames(page);
					const png = await page.screenshot({ type: 'png' });
					const rel = join('settings', `${shot.name}${suffix}-${dpr}x.png`);
					writeOut(rel, png.toString('base64'));
					const [t] = await composite(png, [null]);
					writeOut(thumbOf(rel), t.jpg);
					files.push(rel);
				};
				await capture('');
				if (shot.scroll) {
					// The page column scrolls inside the window; capture it further down.
					const max = await page.evaluate(() => {
						const el = document.querySelector('.st-page');
						return el ? el.scrollHeight - el.clientHeight : 0;
					});
					let i = 1;
					for (let y = 900; y < max + 900 && i < 6; y += 900, i++) {
						await page.evaluate((top) => document.querySelector('.st-page')?.scrollTo(0, top), y);
						await page.waitForTimeout(300);
						await capture(`-s${i}`);
					}
				}
				manifest.settings[key] = {
					name: shot.name,
					dpr,
					query: shot.query,
					hash: shot.hash,
					files,
					errors,
				};
				if (errors.length) failures.push(`settings ${key}: page errors: ${errors.join(' | ')}`);
			} catch (e) {
				failures.push(`settings ${key}: ${e.message.split('\n')[0]}`);
			} finally {
				await page.close().catch(() => {});
			}
		});
		await ctx.close();
	}
}

/* -------------------------------------------------------------------- run */

const t0 = performance.now();
if (ONLY !== 'settings') {
	const jobs = [];
	for (const state of states)
		for (const res of resolutions)
			for (const route of routes)
				for (const theme of themes) jobs.push({ state, route, theme, res });
	// Disconnected shots use the default port 47820 one at a time, so the
	// card shows the address a real setup would have.
	const disconnected = jobs.filter((j) => STATES[j.state].disconnect);
	const others = jobs.filter((j) => !STATES[j.state].disconnect);
	let done = 0;
	const tick = async (job) => {
		await overlayShot(job);
		done++;
		if (done % 20 === 0 || done === jobs.length)
			console.log(
				`overlay ${done}/${jobs.length} (${Math.round((performance.now() - t0) / 1000)} s)`
			);
	};
	await Promise.all([pool(others, CONCURRENCY, tick), pool(disconnected, 1, tick)]);
}
if (ONLY !== 'overlay') {
	const preview = await startPreview();
	try {
		await settingsRun(preview.base);
	} finally {
		await preview.stop();
	}
	console.log(`settings done (${Math.round((performance.now() - t0) / 1000)} s)`);
}
await browser.close();

/* ------------------------------------------------------------ contact sheet */

const FONT_FILES = {
	'rajdhani-600.woff2': 'node_modules/@fontsource/rajdhani/files/rajdhani-latin-600-normal.woff2',
	'share-tech-mono-400.woff2':
		'node_modules/@fontsource/share-tech-mono/files/share-tech-mono-latin-400-normal.woff2',
};
mkdirSync(join(OUT, 'fonts'), { recursive: true });
for (const [name, src] of Object.entries(FONT_FILES))
	copyFileSync(join(ROOT, src), join(OUT, 'fonts', name));

manifest.generated = new Date().toISOString();
manifest.chromium = chromiumVersion;
manifest.failures = failures;
writeFileSync(manifestPath, JSON.stringify(manifest, null, '\t') + '\n');
writeFileSync(join(OUT, 'index.html'), contactSheet(manifest));

const overlayCount = Object.values(manifest.overlay).reduce(
	(n, s) => n + Object.keys(s.files).length,
	0
);
const settingsCount = Object.values(manifest.settings).reduce((n, s) => n + s.files.length, 0);
console.log(
	`${overlayCount} overlay PNGs, ${settingsCount} settings PNGs in ${relative(ROOT, OUT)} (${Math.round(
		(performance.now() - t0) / 1000
	)} s); contact sheet ${relative(ROOT, join(OUT, 'index.html'))}`
);
if (failures.length) {
	console.error(`${failures.length} shots failed:\n  ${failures.join('\n  ')}`);
	process.exit(1);
}

function esc(s) {
	return String(s).replace(
		/[&<>"]/g,
		(c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]
	);
}

function contactSheet(m) {
	const href = (rel) => esc(rel.split('\\').join('/'));
	const thumb = (rel, alt) =>
		`<a href="${href(rel)}"><img src="${href(thumbOf(rel))}" alt="${esc(alt)}" loading="lazy" width="${THUMB_WIDTH}" height="${Math.round(
			(THUMB_WIDTH * 9) / 16
		)}"></a>`;
	const overlay = Object.values(m.overlay);
	const stateIds = Object.keys(STATES).filter((s) => overlay.some((o) => o.state === s));
	const sections = stateIds.map((state) => {
		const rows = ALL_ROUTES.filter((rt) =>
			overlay.some((o) => o.state === state && o.route === rt)
		).map((rt) => {
			const cells = ALL_THEMES.map((th) => {
				const hd = m.overlay[`${state}/${rt}/${th}/1080p`];
				const uhd = m.overlay[`${state}/${rt}/${th}/4k`];
				if (!hd && !uhd) return `<td class="missing">Not captured</td>`;
				const main = hd ?? uhd;
				const links = [hd, uhd]
					.filter(Boolean)
					.flatMap((o) =>
						Object.entries(o.files).map(
							([bg, rel]) => `<a href="${href(rel)}">${o.res === '4k' ? '4K' : '1080p'} ${bg}</a>`
						)
					)
					.join(' ');
				return `<td><figure>${thumb(main.files.light ?? Object.values(main.files)[0], `${rt} route, ${th} theme, ${state}, light background`)}${thumb(
					main.files.dark ?? Object.values(main.files)[0],
					`${rt} route, ${th} theme, ${state}, dark background`
				)}<figcaption>${links}</figcaption></figure></td>`;
			});
			return `<tr><th scope="row">${esc(rt)}</th>${cells.join('')}</tr>`;
		});
		return `<section><h2>${esc(STATES[state].title)}</h2><p class="meta">State <code>${esc(state)}</code>. Each cell: the 1080p shot over light gray, then over dark; the links open every resolution and background.</p><div class="scroll"><table><thead><tr><th scope="col">Route</th>${ALL_THEMES.map(
			(t) => `<th scope="col">${esc(t)}</th>`
		).join('')}</tr></thead><tbody>${rows.join('')}</tbody></table></div></section>`;
	});
	const settings = Object.values(m.settings)
		.filter((s) => s.dpr === 1)
		.sort((a, b) => a.name.localeCompare(b.name))
		.map((s) => {
			const hi = m.settings[`${s.name}/2x`];
			const extra = s.files.slice(1).map((f, i) => `<a href="${href(f)}">scrolled ${i + 1}</a>`);
			const two = hi
				? hi.files.map((f, i) => `<a href="${href(f)}">2x${i ? ` scrolled ${i}` : ''}</a>`)
				: [];
			return `<figure class="card">${thumb(s.files[0], `settings ${s.name}`)}<figcaption><strong>${esc(s.name)}</strong> ${[...extra, ...two].join(' ')}</figcaption></figure>`;
		});
	const fails = m.failures.length
		? `<section class="fail"><h2>Failed shots (${m.failures.length})</h2><ul>${m.failures.map((f) => `<li>${esc(f)}</li>`).join('')}</ul></section>`
		: '';
	return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>IDEA Keystage screenshots</title>
<style>
@font-face { font-family: Rajdhani; font-weight: 600; src: url(fonts/rajdhani-600.woff2) format('woff2'); }
@font-face { font-family: 'Share Tech Mono'; src: url(fonts/share-tech-mono-400.woff2) format('woff2'); }
:root { color-scheme: dark; --bg: #121413; --panel: #1b1e1c; --line: #2f3531; --ink: #e8ece9; --muted: #a9b3ad; --accent: #78b870; --warn: #e0a35a; }
* { box-sizing: border-box; }
body { margin: 0; padding: 24px 16px 48px; background: var(--bg); color: var(--ink); font: 16px/1.45 Rajdhani, system-ui, sans-serif; }
h1 { font-size: 30px; margin: 0 0 4px; letter-spacing: 0.02em; }
h2 { font-size: 22px; margin: 32px 0 4px; border-left: 4px solid var(--accent); padding-left: 10px; }
.meta, figcaption, code, td.missing { font-family: 'Share Tech Mono', ui-monospace, monospace; font-size: 13px; color: var(--muted); }
.scroll { overflow-x: auto; }
table { border-collapse: collapse; }
th { text-align: left; font-weight: 600; padding: 6px 8px; color: var(--muted); }
td { vertical-align: top; padding: 6px; border-top: 1px solid var(--line); }
figure { margin: 0; }
img { display: block; width: 240px; height: auto; margin-bottom: 4px; border: 1px solid var(--line); border-radius: 4px; }
a { color: var(--ink); text-decoration-color: var(--accent); }
figcaption a { margin-right: 6px; }
.cards { display: flex; flex-wrap: wrap; gap: 16px; }
.card { background: var(--panel); border: 1px solid var(--line); border-radius: 6px; padding: 8px; width: 258px; }
.card img { width: 240px; }
.fail { border: 1px solid var(--warn); border-radius: 6px; padding: 0 16px 8px; }
.fail h2 { border-left-color: var(--warn); }
</style>
</head>
<body>
<h1>IDEA Keystage screenshots</h1>
<p class="meta">Generated ${esc(m.generated)} with Chromium ${esc(m.chromium)}. Overlay shots come from the demo server through the real WebSocket overlay page, composited over light gray ${esc(BACKGROUNDS.light)} and dark ${esc(BACKGROUNDS.dark)}. Settings shots use the in-memory mock backend.</p>
${fails}
${sections.join('\n')}
<section><h2>Settings window</h2><p class="meta">Every page in both registers (page-*, light-*) and every mock scenario (state-*), at 1920x1080 and at 2x.</p><div class="cards">${settings.join('')}</div></section>
</body>
</html>
`;
}
