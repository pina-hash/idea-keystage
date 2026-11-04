/**
 * Builds the IDEA Keystage connector bundle (`.mcpb`, the Claude desktop
 * extension format, MCPB MANIFEST.md 0.3): `manifest.json` with the
 * version and the tool list filled in, `server/idea-keystage-mcp.exe`, and
 * `icon.png`.
 *
 *   node scripts/pack-mcpb.mjs --exe target/release/idea-keystage-mcp.exe \
 *     --version 0.3.0 --out release/IDEA-Keystage_0.3.0_connector.mcpb
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { zip } from './lib/zip.mjs';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

/** The manifest for `version`. */
export function manifest(version) {
	if (!/^\d+\.\d+\.\d+$/.test(version)) throw new Error(`not a version: ${version}`);
	const m = JSON.parse(readFileSync(resolve(ROOT, 'mcpb/manifest.json'), 'utf8'));
	m.version = version;
	m.tools = JSON.parse(readFileSync(resolve(ROOT, 'mcpb/tools.json'), 'utf8'));
	return m;
}

/** The bundle bytes. */
export function bundle(version, exeBytes, iconBytes) {
	const m = manifest(version);
	return zip([
		{ name: 'manifest.json', data: Buffer.from(JSON.stringify(m, null, 2) + '\n', 'utf8') },
		{ name: 'server/idea-keystage-mcp.exe', data: exeBytes },
		{ name: 'icon.png', data: iconBytes },
	]);
}

function arg(name) {
	const i = process.argv.indexOf(`--${name}`);
	const v = i >= 0 ? process.argv[i + 1] : undefined;
	if (!v) throw new Error(`missing --${name}`);
	return v;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
	const out = bundle(
		arg('version'),
		readFileSync(arg('exe')),
		readFileSync(resolve(ROOT, 'src-tauri/icons/128x128.png'))
	);
	writeFileSync(arg('out'), out);
	console.log(`wrote ${arg('out')} (${out.length} bytes)`);
}
