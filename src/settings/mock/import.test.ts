import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import type { ProfileData } from '../types';
import { parseCopyList, previewImport, sldregResult } from './import';
import { sampleCopyList } from './fixtures';
import { captionFor } from '../lib/shortcuts';

const FIXTURES = resolve(__dirname, '../../../crates/keystage-core/tests/fixtures/sw-shortcuts');
const fixture = (name: string) => readFileSync(resolve(FIXTURES, name), 'utf8');
const DATA = JSON.parse(
	readFileSync(resolve(__dirname, '../../../data/profiles/solidworks.json'), 'utf8')
) as ProfileData;

describe('the Copy List port matches keystage-core on its fixtures', () => {
	it('copylist_multi_and_bad.txt (same pairs and warnings as tests/import.rs)', () => {
		const r = parseCopyList(fixture('copylist_multi_and_bad.txt'));
		expect(r.entries.map((e) => [e.keys.join('+'), e.command])).toEqual([
			['Ctrl+B', 'Fixture Command One'],
			['Ctrl+B', 'Fixture Command Seven'],
			['Ctrl+R', 'Fixture Command Seven'],
			['Ctrl+B', 'Fixture Command Eight'],
		]);
		expect(r.entries.find((e) => e.command === 'Fixture Command Eight')?.doc_keys).toBe('ctrl+b');
		expect(r.unparsed).toBe(2);
		const w = r.warnings.join('\n');
		expect(w).toContain('unknown key "???"');
		expect(w).toContain('expected 3 columns, found 2');
		expect(w).toContain('Lowercase letters were read as uppercase (line 4).');
		expect(w).toContain('Ctrl + B is assigned to more than one command');
	});

	it('reads tab lists with and without a header, CRLF or LF', () => {
		for (const name of [
			'copylist_tab_header_crlf.txt',
			'copylist_tab_header_lf.txt',
			'copylist_tab_noheader_crlf.txt',
			'copylist_tab_header_trailing_blank.txt',
		]) {
			const r = parseCopyList(fixture(name));
			expect(r.format).toBe('copy_list');
			expect(r.entries.length, name).toBeGreaterThan(0);
			expect(r.entries.every((e) => e.source === 'user-import')).toBe(true);
			expect(r.entries.every((e) => e.command.startsWith('Fixture'))).toBe(true);
		}
	});

	it('splits several shortcuts in one cell and keeps Ctrl+comma whole', () => {
		const r = parseCopyList(
			'Category\tCommand\tShortcut(s)\nA\tFixture Command One\tCtrl+B, Shift+F3; Ctrl+,\n'
		);
		expect(r.entries.map((e) => e.keys.join('+'))).toEqual(['Ctrl+B', 'Shift+F3', 'Ctrl+,']);
	});

	it('reads comma and semicolon lists, quoted cells and a header in any column order', () => {
		const comma = parseCopyList('Command,Category,Shortcut(s)\n"Fixture, Command",A,Ctrl+Q\n');
		expect(comma.entries).toEqual([
			{
				keys: ['Ctrl', 'Q'],
				doc_keys: 'Ctrl+Q',
				command: 'Fixture, Command',
				category: 'A',
				source: 'user-import',
			},
		]);
		const semi = parseCopyList('A;Fixture One;Alt+F1\nB;Fixture Two;Shift+Win+Z\n');
		expect(semi.entries.map((e) => e.keys)).toEqual([
			['Alt', 'F1'],
			['Shift', 'Win', 'Z'],
		]);
	});

	it('says when nothing was found', () => {
		expect(parseCopyList('').warnings).toEqual([
			'No shortcuts were found. Copy the list with Category set to All Commands.',
		]);
		expect(sldregResult().format).toBe('sldreg');
		expect(sldregResult().warnings[0]).toContain('use Copy List');
	});
});

describe('preview against the sourced data', () => {
	it('sorts the sample into added, changed and unchanged by the data captions', () => {
		const r = parseCopyList(sampleCopyList(DATA, false));
		const p = previewImport(r, DATA);
		expect(p.validated).toBe(false);
		expect(p.entries.length).toBe(r.entries.length);
		expect(p.added.length + p.changed.length + p.unchanged).toBe(r.entries.length);
		expect(p.unchanged).toBeGreaterThan(0);
		expect(p.added.length).toBeGreaterThan(0);
		expect(p.changed.length).toBe(1);
		for (const c of p.changed) expect(c.previous).toBe(captionFor(DATA, c.keys)?.caption);
		for (const a of p.added) expect(captionFor(DATA, a.keys)).toBeNull();
		// Every command name in the sample comes from the data file.
		const commands = new Set(DATA.shortcuts.map((s) => s.command));
		for (const e of r.entries) expect(commands.has(e.command)).toBe(true);
	});

	it('the warnings sample has each kind of warning', () => {
		const p = previewImport(parseCopyList(sampleCopyList(DATA, true)), DATA);
		const w = p.warnings.join('\n');
		expect(w).toContain('unknown key "Fn"');
		expect(w).toContain('expected 3 columns');
		expect(w).toContain('Lowercase letters');
		expect(w).toContain('is assigned to more than one command');
	});
});
