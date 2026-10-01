/**
 * The SolidWorks page's table: rows built from the merged profile
 * (`profile_shortcuts`), searched and filtered on the client. Every caption,
 * context and source here comes from the profile data (or the user's own
 * import); this module only arranges it.
 */
import type {
	ImportChange,
	ProfileData,
	ProfileMouse,
	ProfileShortcut,
	ProfileSource,
	UserShortcut,
} from '../types';
import { DISPLAY_MOD_ORDER, isModifierName } from './accelerator';

export type RowKind = 'keyboard' | 'mouse';

export interface RowSource {
	id: string;
	version: string;
	title: string;
	url: string;
	screenshot: boolean;
}

export interface ShortcutRow {
	id: string;
	kind: RowKind;
	/** Keycap labels in display order: modifiers, then keys or the mouse action. */
	parts: string[];
	/** The keys as the source page wrote them (keyboard rows). */
	docKeys: string;
	/** More than one key after the modifiers (a chord or an Alt code): shown as the page wrote it. */
	sequence: boolean;
	command: string;
	context: string;
	description: string;
	group: string;
	versions: string[];
	sources: RowSource[];
	/** From the user's own SolidWorks import (Copy List). */
	imported: boolean;
	/** The entry was transcribed from a screenshot, not page text. */
	screenshot: boolean;
	/** Lowercased text the search matches against. */
	haystack: string;
}

export type KindFilter = 'all' | 'keyboard' | 'mouse' | 'imported';
export type VersionFilter = 'all' | '2025' | '2026';

export interface RowFilter {
	query: string;
	kind: KindFilter;
	version: VersionFilter;
}

/** Same keys regardless of order (modifier order differs between sources). */
export function sameKeySet(a: readonly string[], b: readonly string[]): boolean {
	if (a.length !== b.length) return false;
	const sa = [...a].sort();
	const sb = [...b].sort();
	return sa.every((v, i) => v === sb[i]);
}

/**
 * When one key combination is bound to several commands in an import, the
 * first in file order is used (keystage-core import.rs). For a previewed
 * change, the command that wins instead of it, or null when it is the one
 * used.
 */
export function shadowedBy(entries: readonly UserShortcut[], change: ImportChange): string | null {
	const first = entries.find((e) => sameKeySet(e.keys, change.keys));
	if (!first || first.command === change.command) return null;
	return first.command;
}

/** Modifiers first in display order, then the other keys in their given order. */
export function displayKeyParts(keys: readonly string[]): string[] {
	const mods = DISPLAY_MOD_ORDER.filter((m) => keys.includes(m));
	const rest = keys.filter((k) => !isModifierName(k));
	return [...mods, ...rest];
}

const BUTTON_WORDS: Readonly<Record<string, string>> = {
	left: 'Left',
	right: 'Right',
	middle: 'Middle',
	x1: 'Back button',
	x2: 'Forward button',
};

const ACTION_WORDS: Readonly<Record<string, string>> = {
	click: 'click',
	double_click: 'double click',
	drag: 'drag',
	press: 'press',
	gesture: 'gesture',
};

/** "Middle drag", "Drag", "Wheel", "Right gesture up-right". */
export function mouseActionWords(m: Pick<ProfileMouse, 'button' | 'action' | 'direction'>): string {
	const dir = m.direction ? ` ${m.direction.replace(/_/g, '-')}` : '';
	if (m.action === 'wheel') return `Wheel${dir}`;
	const action = ACTION_WORDS[m.action] ?? m.action.replace(/_/g, ' ');
	const button = BUTTON_WORDS[m.button];
	const text = button ? `${button} ${action}` : action;
	return `${text.charAt(0).toUpperCase()}${text.slice(1)}${dir}`;
}

function sourcesFor(ids: readonly string[], byId: ReadonlyMap<string, ProfileSource>): RowSource[] {
	const out: RowSource[] = [];
	for (const id of ids) {
		const s = byId.get(id);
		if (!s || typeof s.url !== 'string') continue;
		out.push({
			id: s.id,
			version: s.version,
			title: s.title,
			url: s.url,
			screenshot: s.evidence === 'screenshot',
		});
	}
	out.sort((a, b) => a.version.localeCompare(b.version));
	return out;
}

function keyboardRow(s: ProfileShortcut, byId: ReadonlyMap<string, ProfileSource>): ShortcutRow {
	const parts = displayKeyParts(s.keys);
	const imported = s.source === 'user-import';
	const row: ShortcutRow = {
		id: s.id || `user-${parts.join('+')}`,
		kind: 'keyboard',
		parts,
		docKeys: s.doc_keys ?? parts.join(' + '),
		sequence: parts.filter((k) => !isModifierName(k)).length > 1,
		command: (s.command ?? '').trim(),
		context: (s.context ?? '').trim(),
		description: (s.description ?? '').trim(),
		group: (s.group ?? '').trim(),
		versions: Array.isArray(s.versions) ? [...s.versions] : [],
		sources: sourcesFor(s.source_ids ?? [], byId),
		imported,
		screenshot: s.evidence === 'screenshot',
		haystack: '',
	};
	row.haystack = haystackOf(row);
	return row;
}

function mouseRow(m: ProfileMouse, byId: ReadonlyMap<string, ProfileSource>): ShortcutRow {
	const mods = DISPLAY_MOD_ORDER.filter((x) => m.modifiers.includes(x));
	const action = mouseActionWords(m);
	const row: ShortcutRow = {
		id: m.id,
		kind: 'mouse',
		parts: [...mods, action],
		docKeys: [...mods, action].join(' + '),
		sequence: false,
		command: (m.command ?? '').trim(),
		context: (m.context ?? '').trim(),
		description: (m.description ?? '').trim(),
		group: '',
		versions: Array.isArray(m.versions) ? [...m.versions] : [],
		sources: sourcesFor(m.source_ids ?? [], byId),
		imported: false,
		screenshot: false,
		haystack: '',
	};
	row.haystack = haystackOf(row);
	return row;
}

function haystackOf(r: ShortcutRow): string {
	return [r.parts.join(' + '), r.docKeys, r.command, r.context, r.description, r.group]
		.join('\n')
		.toLowerCase();
}

/** All rows: imported shortcuts first (they win), then the sourced keyboard entries, then mouse. */
export function buildRows(profile: ProfileData): ShortcutRow[] {
	const byId = new Map(profile.sources.map((s) => [s.id, s] as const));
	const keyboard = profile.shortcuts.map((s) => keyboardRow(s, byId));
	const imported = keyboard.filter((r) => r.imported);
	const sourced = keyboard.filter((r) => !r.imported);
	return [...imported, ...sourced, ...profile.mouse.map((m) => mouseRow(m, byId))];
}

/** Every word of the query must appear (in any field). */
export function filterRows(rows: readonly ShortcutRow[], f: RowFilter): ShortcutRow[] {
	const words = f.query
		.toLowerCase()
		.split(/\s+/)
		.map((w) => w.replace(/^\+|\+$/g, ''))
		.filter((w) => w.length > 0);
	return rows.filter((r) => {
		if (f.kind === 'keyboard' && r.kind !== 'keyboard') return false;
		if (f.kind === 'mouse' && r.kind !== 'mouse') return false;
		if (f.kind === 'imported' && !r.imported) return false;
		if (f.version !== 'all' && !r.imported && !r.versions.includes(f.version)) return false;
		return words.every((w) => r.haystack.includes(w));
	});
}

export interface ProfileCounts {
	keyboard: number;
	mouse: number;
	imported: number;
	sources: number;
	versions: string[];
	read: string | null;
}

export function profileCounts(profile: ProfileData): ProfileCounts {
	const imported = profile.shortcuts.filter((s) => s.source === 'user-import').length;
	const versions = new Set<string>();
	for (const s of profile.sources) if (s.version) versions.add(s.version);
	return {
		keyboard: profile.shortcuts.length - imported,
		mouse: profile.mouse.length,
		imported,
		sources: profile.sources.length,
		versions: [...versions].sort(),
		read: typeof profile.read === 'string' ? profile.read : null,
	};
}

/**
 * The caption the data gives for these keys, using the core's tie rule (an
 * entry with no context wins, otherwise the first in file order). Used to
 * caption the theme previews from the data, never from memory.
 */
export function captionFor(
	profile: ProfileData | null,
	keys: readonly string[]
): { caption: string; context: string | null } | null {
	if (!profile) return null;
	let first: ProfileShortcut | null = null;
	for (const s of profile.shortcuts) {
		if (!sameKeySet(s.keys, keys) || !s.command?.trim()) continue;
		if (!s.context?.trim()) return { caption: s.command.trim(), context: null };
		first ??= s;
	}
	return first ? { caption: first.command.trim(), context: first.context.trim() } : null;
}
