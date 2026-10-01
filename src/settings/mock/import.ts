/**
 * The mock backend's shortcut import, a TypeScript port of keystage-core
 * import.rs (`parse_copy_list`, `preview`) so the import panel can be shown
 * and screenshotted in a plain browser with the same results and the same
 * warning text. The app itself always uses the Rust importer.
 */
import { DISPLAY_MOD_ORDER, modFromAlias, parseKeyName, type ModName } from '../lib/accelerator';
import { captionFor } from '../lib/shortcuts';
import type {
	ImportChange,
	ImportFormat,
	ImportPreview,
	ProfileData,
	UserShortcut,
} from '../types';

export interface MockImportResult {
	format: ImportFormat;
	entries: UserShortcut[];
	warnings: string[];
	unparsed: number;
}

type Delim = '\t' | ';' | ',';

function splitLines(text: string): string[] {
	return text
		.split('\n')
		.map((l) => (l.endsWith('\r') ? l.slice(0, -1) : l))
		.flatMap((l) => l.split('\r'));
}

function detectDelimiter(lines: readonly string[]): Delim {
	const rows = lines.filter((l) => l.trim().length > 0).slice(0, 50);
	const rowsWith = (c: string) => rows.filter((l) => l.split(c).length - 1 >= 2).length;
	if (rows.some((l) => l.includes('\t'))) return '\t';
	const semi = rowsWith(';');
	const comma = rowsWith(',');
	if (semi > 0 && semi * 2 >= rows.length) return ';';
	if (comma > 0) return ',';
	if (semi > 0) return ';';
	return '\t';
}

function splitCells(line: string, d: Delim): string[] {
	const cells: string[] = [];
	let cur = '';
	let inQuotes = false;
	let atStart = true;
	for (let i = 0; i < line.length; i++) {
		const c = line[i]!;
		if (inQuotes) {
			if (c === '"') {
				if (line[i + 1] === '"') {
					cur += '"';
					i++;
				} else inQuotes = false;
			} else cur += c;
			continue;
		}
		if (c === '"' && atStart && cur.trim().length === 0) {
			cur = '';
			inQuotes = true;
			atStart = false;
			continue;
		}
		if (c === d) {
			cells.push(cur);
			cur = '';
			atStart = true;
			continue;
		}
		atStart = false;
		cur += c;
	}
	cells.push(cur);
	return cells;
}

interface Columns {
	category: number;
	command: number;
	shortcut: number;
}

function headerColumns(cells: readonly string[]): Columns | null {
	const norm = cells.map((c) => c.trim().toLowerCase());
	const category = norm.findIndex((c) => c === 'category');
	const command = norm.findIndex((c) => c === 'command' || c === 'commands');
	const shortcut = norm.findIndex((c) => c.startsWith('shortcut'));
	if (category < 0 || command < 0 || shortcut < 0) return null;
	return { category, command, shortcut };
}

function splitShortcuts(cell: string): string[] {
	const out: string[] = [];
	let cur = '';
	for (const c of cell) {
		if ((c === ',' || c === ';') && cur.trim().length > 0 && !cur.trimEnd().endsWith('+')) {
			out.push(cur.trim());
			cur = '';
		} else cur += c;
	}
	if (cur.trim().length > 0) out.push(cur.trim());
	return out;
}

type Parsed = { ok: true; keys: string[]; lower: boolean } | { ok: false; bad: string };

function parseShortcut(s: string): Parsed {
	const tokens: string[] = [];
	let cur = '';
	for (const c of s) {
		if (c === '+') {
			if (cur.trim().length === 0) return { ok: false, bad: '+' };
			tokens.push(cur.trim());
			cur = '';
		} else cur += c;
	}
	if (cur.trim().length === 0) return { ok: false, bad: tokens.length === 0 ? s : '+' };
	tokens.push(cur.trim());
	const mods = new Set<ModName>();
	let key: string | null = null;
	let lower = false;
	for (const t of tokens) {
		const m = modFromAlias(t.replace(/\s/g, '').toLowerCase());
		if (m) {
			mods.add(m);
			continue;
		}
		if (key !== null) return { ok: false, bad: t };
		if ([...t].length === 1 && /^[a-z]$/.test(t)) lower = true;
		const name = parseKeyName(t);
		if (!name) return { ok: false, bad: t };
		key = name;
	}
	const keys: string[] = DISPLAY_MOD_ORDER.filter((m) => mods.has(m));
	if (key !== null) keys.push(key);
	return { ok: true, keys, lower };
}

/** keystage-core `parse_copy_list`. */
export function parseCopyList(text: string): MockImportResult {
	const r: MockImportResult = { format: 'copy_list', entries: [], warnings: [], unparsed: 0 };
	const lines = splitLines(text.replace(/^﻿/, ''));
	const delim = detectDelimiter(lines);
	let columns: Columns = { category: 0, command: 1, shortcut: 2 };
	let seenHeader = false;
	let seenRows = 0;
	const lowerRows: number[] = [];
	const bindings = new Map<string, string[]>();
	lines.forEach((raw, i) => {
		const lineNo = i + 1;
		if (raw.trim().length === 0) return;
		let cells = splitCells(raw, delim);
		if (!seenHeader && seenRows === 0) {
			const cols = headerColumns(cells);
			if (cols) {
				columns = cols;
				seenHeader = true;
				return;
			}
		}
		seenRows++;
		const needed = Math.max(columns.category, columns.command, columns.shortcut) + 1;
		if (delim === ',' && cells.length > needed && columns.shortcut + 1 === needed) {
			cells = [...cells.slice(0, columns.shortcut), cells.slice(columns.shortcut).join(',')];
		}
		if (cells.length < needed) {
			r.warnings.push(
				`Line ${lineNo}: expected ${needed} columns, found ${cells.length}; skipped.`
			);
			r.unparsed++;
			return;
		}
		const category = (cells[columns.category] ?? '').trim();
		const command = (cells[columns.command] ?? '').trim();
		const cell = (cells[columns.shortcut] ?? '').trim();
		if (cell.length === 0) return;
		if (command.length === 0) {
			r.warnings.push(`Line ${lineNo}: the Command cell is empty; skipped.`);
			r.unparsed++;
			return;
		}
		for (const piece of splitShortcuts(cell)) {
			const p = parseShortcut(piece);
			if (!p.ok) {
				r.warnings.push(
					`Line ${lineNo}: "${piece}" has an unknown key "${p.bad}"; kept as written, not shown.`
				);
				r.unparsed++;
				continue;
			}
			if (p.lower) lowerRows.push(lineNo);
			const binding = p.keys.join('+');
			const users = bindings.get(binding) ?? [];
			if (!users.includes(command)) users.push(command);
			bindings.set(binding, users);
			const entry: UserShortcut = {
				keys: p.keys,
				doc_keys: piece,
				command,
				category,
				source: 'user-import',
			};
			if (!r.entries.some((e) => JSON.stringify(e) === JSON.stringify(entry)))
				r.entries.push(entry);
		}
	});
	if (lowerRows.length > 0) {
		r.warnings.push(`Lowercase letters were read as uppercase (line ${lowerRows.join(', ')}).`);
	}
	for (const [binding, commands] of [...bindings.entries()].sort(([a], [b]) =>
		a < b ? -1 : a > b ? 1 : 0
	)) {
		if (commands.length > 1) {
			r.warnings.push(
				`${binding.replace(/\+/g, ' + ')} is assigned to more than one command (${commands.join('; ')}); the first one is used.`
			);
		}
	}
	if (r.entries.length === 0 && r.unparsed === 0) {
		r.warnings.push('No shortcuts were found. Copy the list with Category set to All Commands.');
	}
	return r;
}

/** keystage-core's `.sldreg` result for a file with no documented keyboard section. */
export function sldregResult(): MockImportResult {
	return {
		format: 'sldreg',
		entries: [],
		unparsed: 0,
		warnings: [
			'Could not read the keyboard section; use Copy List from the Keyboard tab instead.',
			'The file holds 214 registry key(s) and 1873 value(s); none is a documented keyboard section.',
		],
	};
}

/** keystage-core `preview`: added, changed and unchanged against the sourced captions. */
export function previewImport(
	result: MockImportResult,
	sourced: ProfileData | null
): ImportPreview {
	const added: ImportChange[] = [];
	const changed: ImportChange[] = [];
	let unchanged = 0;
	for (const e of result.entries) {
		const change = (previous: string | null): ImportChange => ({
			keys: [...e.keys],
			doc_keys: e.doc_keys,
			command: e.command,
			previous,
			category: e.category,
		});
		const existing = captionFor(sourced, e.keys);
		if (!existing) added.push(change(null));
		else if (existing.caption.trim().toLowerCase() === e.command.trim().toLowerCase()) unchanged++;
		else changed.push(change(existing.caption));
	}
	return {
		format: result.format,
		entries: result.entries.map((e) => ({ ...e, keys: [...e.keys] })),
		added,
		changed,
		unchanged,
		warnings: [...result.warnings],
		validated: false,
	};
}
