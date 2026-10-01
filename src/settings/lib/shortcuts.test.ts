import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import type { ProfileData } from '../types';
import {
	buildRows,
	captionFor,
	displayKeyParts,
	filterRows,
	mouseActionWords,
	profileCounts,
	sameKeySet,
	shadowedBy,
} from './shortcuts';

/** The sourced data file, read the way the app ships it. */
const DATA = JSON.parse(
	readFileSync(resolve(__dirname, '../../../data/profiles/solidworks.json'), 'utf8')
) as ProfileData;

describe('rows from the SolidWorks data', () => {
	const rows = buildRows(DATA);

	it('has one row per shortcut and per mouse entry, with their sources', () => {
		expect(rows.length).toBe(DATA.shortcuts.length + DATA.mouse.length);
		const first = DATA.shortcuts[0]!;
		const row = rows.find((r) => r.id === first.id)!;
		expect(row.command).toBe(first.command.trim());
		expect(row.sources.map((s) => s.id).sort()).toEqual([...first.source_ids].sort());
		for (const s of row.sources) expect(s.url.startsWith('https://')).toBe(true);
		expect(row.sources.map((s) => s.version)).toEqual(
			[...row.sources.map((s) => s.version)].sort()
		);
	});

	it('marks the screenshot entries and shows chords as the page wrote them', () => {
		const shots = rows.filter((r) => r.screenshot);
		expect(shots.length).toBe(DATA.shortcuts.filter((s) => s.evidence === 'screenshot').length);
		const seq = rows.filter((r) => r.sequence);
		for (const r of seq) {
			const entry = DATA.shortcuts.find((s) => s.id === r.id)!;
			expect(r.docKeys).toBe(entry.doc_keys);
		}
	});

	it('puts imported shortcuts first', () => {
		const withImport: ProfileData = {
			...DATA,
			shortcuts: [
				...DATA.shortcuts,
				{
					id: 'user-001',
					keys: ['Ctrl', 'Alt', 'Q'],
					doc_keys: 'Ctrl+Alt+Q',
					command: 'Fixture Command One',
					description: '',
					group: 'Fixture',
					context: '',
					versions: [],
					source_ids: [],
					quote: '',
					source: 'user-import',
				},
			],
		};
		const r = buildRows(withImport);
		expect(r[0]!.imported).toBe(true);
		expect(r[0]!.parts).toEqual(['Ctrl', 'Alt', 'Q']);
		expect(filterRows(r, { query: '', kind: 'imported', version: 'all' }).map((x) => x.id)).toEqual(
			['user-001']
		);
		expect(
			filterRows(r, { query: 'fixture', kind: 'all', version: '2025' }).map((x) => x.id)
		).toEqual(['user-001']);
		expect(profileCounts(withImport).imported).toBe(1);
	});

	it('filters by every word, by kind and by version', () => {
		const all = filterRows(rows, { query: '', kind: 'all', version: 'all' });
		expect(all.length).toBe(rows.length);
		const mouse = filterRows(rows, { query: '', kind: 'mouse', version: 'all' });
		expect(mouse.length).toBe(DATA.mouse.length);
		const only2025 = filterRows(rows, { query: '', kind: 'keyboard', version: '2025' });
		expect(only2025.every((r) => r.versions.includes('2025'))).toBe(true);
		const ctrl = filterRows(rows, { query: 'ctrl', kind: 'keyboard', version: 'all' });
		expect(ctrl.length).toBeGreaterThan(10);
		expect(ctrl.every((r) => r.haystack.includes('ctrl'))).toBe(true);
		expect(filterRows(rows, { query: 'zzqqxx', kind: 'all', version: 'all' })).toEqual([]);
	});

	it('counts what the data holds', () => {
		const c = profileCounts(DATA);
		expect(c.keyboard).toBe(DATA.shortcuts.length);
		expect(c.mouse).toBe(DATA.mouse.length);
		expect(c.sources).toBe(DATA.sources.length);
		expect(c.versions).toEqual(['2025', '2026']);
		expect(c.read).toBe(DATA.read);
	});
});

describe('captionFor (the core tie rule)', () => {
	it('prefers an entry with no context, else the first in file order', () => {
		for (const s of DATA.shortcuts.slice(0, 120)) {
			const same = DATA.shortcuts.filter((x) => sameKeySet(x.keys, s.keys) && x.command.trim());
			const noContext = same.find((x) => !x.context.trim());
			const expected = noContext ?? same[0];
			expect(captionFor(DATA, s.keys)?.caption).toBe(expected?.command.trim());
		}
		expect(captionFor(DATA, ['Ctrl', 'Alt', 'Shift', 'F24'])).toBeNull();
		expect(captionFor(null, ['Ctrl', 'S'])).toBeNull();
	});
});

describe('labels', () => {
	it('orders modifiers for display and words mouse actions', () => {
		expect(displayKeyParts(['Shift', 'Ctrl', 'S'])).toEqual(['Ctrl', 'Shift', 'S']);
		expect(mouseActionWords({ button: 'middle', action: 'drag', direction: null })).toBe(
			'Middle drag'
		);
		expect(
			mouseActionWords({ button: 'unspecified', action: 'double_click', direction: null })
		).toBe('Double click');
		expect(mouseActionWords({ button: 'middle', action: 'wheel', direction: null })).toBe('Wheel');
		expect(mouseActionWords({ button: 'right', action: 'gesture', direction: 'up_right' })).toBe(
			'Right gesture up-right'
		);
		expect(sameKeySet(['Ctrl', 'B'], ['B', 'Ctrl'])).toBe(true);
		expect(sameKeySet(['Ctrl', 'B'], ['Ctrl'])).toBe(false);
	});
});

describe('shadowedBy (first in file order wins)', () => {
	const entry = (keys: string[], command: string) => ({
		keys,
		doc_keys: keys.join('+'),
		command,
		category: 'View',
		source: 'user-import',
	});
	const change = (keys: string[], command: string) => ({
		keys,
		doc_keys: keys.join('+'),
		command,
		previous: null,
		category: 'View',
	});
	const entries = [
		entry(['Ctrl', 'Alt', 'B'], 'First command'),
		entry(['Shift', 'F2'], 'Only command'),
		entry(['B', 'Alt', 'Ctrl'], 'Second command'),
	];

	it('names the winner for a later binding of the same keys', () => {
		expect(shadowedBy(entries, change(['Ctrl', 'Alt', 'B'], 'Second command'))).toBe(
			'First command'
		);
	});

	it('is null for the binding that is used and for unique keys', () => {
		expect(shadowedBy(entries, change(['Alt', 'Ctrl', 'B'], 'First command'))).toBeNull();
		expect(shadowedBy(entries, change(['Shift', 'F2'], 'Only command'))).toBeNull();
		expect(shadowedBy([], change(['Shift', 'F2'], 'Only command'))).toBeNull();
	});
});
