// @vitest-environment node
/// <reference types="node" />
/**
 * The connector bundle (`.mcpb`): the manifest follows MCPB MANIFEST.md
 * 0.3 for a binary server on Windows, carries the app version and every
 * tool, and the ZIP round-trips byte for byte.
 */
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { bundle, manifest } from '../../scripts/pack-mcpb.mjs';
import { unzip, zip } from '../../scripts/lib/zip.mjs';

const ROOT = fileURLToPath(new URL('../..', import.meta.url));
const pkg = JSON.parse(readFileSync(resolve(ROOT, 'package.json'), 'utf8')) as { version: string };

describe('connector bundle', () => {
	it('has a valid binary manifest for this version', () => {
		const m = manifest(pkg.version);
		expect(m.manifest_version).toBe('0.3');
		expect(m.name).toBe('idea-keystage');
		expect(m.version).toBe(pkg.version);
		expect(m.author.name.length).toBeGreaterThan(0);
		expect(m.server.type).toBe('binary');
		expect(m.server.entry_point).toBe('server/idea-keystage-mcp.exe');
		expect(m.server.mcp_config.command).toBe('${__dirname}/server/idea-keystage-mcp.exe');
		expect(m.compatibility.platforms).toEqual(['win32']);
		expect(m.tools_generated).toBe(true);
		const names = m.tools.map((t: { name: string }) => t.name);
		for (const n of ['status', 'session_start', 'perform', 'occluders', 'obs_stop_recording']) {
			expect(names).toContain(n);
		}
		const text = JSON.stringify(m);
		expect(text).not.toMatch(new RegExp('[\\u2013\\u2014]'));
		expect(() => manifest('v1')).toThrow();
	});

	it('zips the exe, the icon and the manifest', () => {
		const exe = Buffer.from('MZ fake connector exe '.repeat(100));
		const icon = Buffer.from([0x89, 0x50, 0x4e, 0x47]);
		const b = bundle(pkg.version, exe, icon);
		const files = unzip(b);
		expect([...files.keys()]).toEqual([
			'manifest.json',
			'server/idea-keystage-mcp.exe',
			'icon.png',
		]);
		expect(Buffer.compare(files.get('server/idea-keystage-mcp.exe'), exe)).toBe(0);
		expect(Buffer.compare(files.get('icon.png'), icon)).toBe(0);
		expect(JSON.parse(files.get('manifest.json').toString('utf8')).version).toBe(pkg.version);
		expect(Buffer.compare(bundle(pkg.version, exe, icon), b)).toBe(0);
		expect(() => unzip(Buffer.from('nope'))).toThrow();
		const empty = unzip(zip([]));
		expect(empty.size).toBe(0);
	});
});
