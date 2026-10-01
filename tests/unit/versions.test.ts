// @vitest-environment node
/// <reference types="node" />
/**
 * One version number everywhere. The release workflow names its files after
 * the tag (v0.1.0 gives IDEA-Keystage_0.1.0_x64-setup.exe) and refuses a tag
 * that differs from tauri.conf.json, so every manifest that carries the
 * version must agree: package.json, the Cargo workspace (which keystage-core
 * reports as APP_VERSION in every hello), and tauri.conf.json.
 */
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const ROOT = fileURLToPath(new URL('../..', import.meta.url));
const read = (path: string): string => readFileSync(resolve(ROOT, path), 'utf8');

/** `version = "x"` inside the `[workspace.package]` table of Cargo.toml. */
function workspaceVersion(cargoToml: string): string | null {
	let inTable = false;
	for (const raw of cargoToml.split('\n')) {
		const line = raw.trim();
		if (line.startsWith('[')) {
			inTable = line === '[workspace.package]';
			continue;
		}
		const m = inTable ? /^version\s*=\s*"([^"]+)"/.exec(line) : null;
		if (m && m[1]) return m[1];
	}
	return null;
}

const SEMVER = /^\d+\.\d+\.\d+$/;

describe('version numbers', () => {
	const pkg = JSON.parse(read('package.json')) as { version: string };
	const tauri = JSON.parse(read('src-tauri/tauri.conf.json')) as { version: string };
	const cargo = workspaceVersion(read('Cargo.toml'));
	const hello = JSON.parse(read('crates/keystage-core/tests/fixtures/protocol/hello.json')) as {
		version: string;
	};

	it('are plain x.y.z (the release tag is v plus this)', () => {
		expect(pkg.version).toMatch(SEMVER);
	});

	it('agree across package.json, Cargo.toml, tauri.conf.json and the hello sample', () => {
		expect(cargo).toBe(pkg.version);
		expect(tauri.version).toBe(pkg.version);
		expect(hello.version).toBe(pkg.version);
	});

	it('every Rust crate takes the workspace version', () => {
		for (const manifest of [
			'crates/keystage-core/Cargo.toml',
			'crates/keystage-session/Cargo.toml',
			'crates/keystage-server/Cargo.toml',
			'crates/keystage-obs/Cargo.toml',
			'src-tauri/Cargo.toml',
		]) {
			expect(read(manifest), manifest).toMatch(/^version\.workspace\s*=\s*true$/m);
		}
	});
});

describe('workspaceVersion', () => {
	it('reads only the [workspace.package] table', () => {
		const toml = [
			'[package]',
			'version = "9.9.9"',
			'[workspace.package]',
			'edition = "2021"',
			'version = "1.2.3"',
			'[workspace.dependencies]',
			'serde = { version = "=1.0.0" }',
		].join('\n');
		expect(workspaceVersion(toml)).toBe('1.2.3');
		expect(workspaceVersion('[package]\nversion = "1.0.0"\n')).toBeNull();
	});
});
