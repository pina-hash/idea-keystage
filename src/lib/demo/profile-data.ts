/**
 * Reading captions out of a profile data file (data/profiles/*.json) for the
 * demo scripts, with the same matching and tie-break rules keystage-core
 * applies (crates/keystage-core/src/profile.rs, ARCHITECTURE.md section 9):
 *   - keys: the same modifiers and the same key; an entry with an empty
 *     context wins, otherwise the first entry in file order.
 *   - mouse: modifiers must match exactly; an "unspecified" button matches
 *     the left button for clicks, double clicks and drags; wheel entries with
 *     no direction match both directions; gesture entries need their
 *     direction; click and drag entries with a direction never match.
 * Nothing here holds a caption of its own.
 */
import { MODIFIER_ORDER } from './scancodes';

export interface ProfileShortcut {
	id: string;
	keys: string[];
	command: string;
	context: string;
}

export interface ProfileMouse {
	id: string;
	button: string;
	action: string;
	modifiers: string[];
	direction: string | null;
	command: string;
	context: string;
}

export interface ProfileData {
	id: string;
	name: string;
	shortcuts: ProfileShortcut[];
	mouse: ProfileMouse[];
}

export interface CaptionHit {
	/** The entry id in the data file, for tests and notes. */
	id: string;
	caption: string;
	context: string | null;
}

function isModifierName(k: string): boolean {
	return MODIFIER_ORDER.includes(k);
}

function sameSet(a: readonly string[], b: readonly string[]): boolean {
	if (a.length !== b.length) return false;
	const sa = [...a].sort();
	const sb = [...b].sort();
	return sa.every((v, i) => v === sb[i]);
}

/** Splits a key list into its modifiers and its one main key, or null. */
export function splitCombo(keys: readonly string[]): { mods: string[]; key: string } | null {
	const mods = keys.filter(isModifierName);
	const rest = keys.filter((k) => !isModifierName(k));
	if (rest.length === 1 && rest[0] !== undefined) return { mods, key: rest[0] };
	if (rest.length === 0 && mods.length === 1 && mods[0] !== undefined)
		return { mods: [], key: mods[0] };
	return null;
}

function pick<T extends { command: string; context: string; id: string }>(
	entries: readonly T[]
): CaptionHit | null {
	let first: T | null = null;
	for (const e of entries) {
		if (!e.command.trim()) continue;
		if (!e.context.trim()) return { id: e.id, caption: e.command.trim(), context: null };
		if (!first) first = e;
	}
	return first
		? { id: first.id, caption: first.command.trim(), context: first.context.trim() }
		: null;
}

export function captionForKeys(profile: ProfileData, keys: readonly string[]): CaptionHit | null {
	const want = splitCombo(keys);
	if (!want) return null;
	const matches = profile.shortcuts.filter((s) => {
		const got = splitCombo(s.keys);
		return got !== null && got.key === want.key && sameSet(got.mods, want.mods);
	});
	return pick(matches);
}

export type DemoMouseAction = 'click' | 'double_click' | 'drag' | 'wheel' | 'gesture';

export function captionForMouse(
	profile: ProfileData,
	button: string,
	action: DemoMouseAction,
	modifiers: readonly string[],
	direction: string | null = null
): CaptionHit | null {
	const matches = profile.mouse.filter((m) => {
		if (m.action !== action || !sameSet(m.modifiers, modifiers)) return false;
		let buttonOk: boolean;
		if (action === 'wheel') buttonOk = true;
		else if (m.button === 'unspecified')
			buttonOk =
				button === 'left' && (action === 'click' || action === 'double_click' || action === 'drag');
		else buttonOk = m.button === button;
		if (!buttonOk) return false;
		if (action === 'gesture') return m.direction !== null && m.direction === direction;
		if (action === 'wheel') return m.direction === null || m.direction === direction;
		return m.direction === null;
	});
	return pick(matches);
}

/** Narrows an imported JSON value to ProfileData, or throws. */
export function asProfileData(v: unknown): ProfileData {
	const o = v as Partial<ProfileData> | null;
	if (!o || typeof o !== 'object' || !Array.isArray(o.shortcuts) || !Array.isArray(o.mouse)) {
		throw new Error('profile data has no shortcuts or mouse table');
	}
	return {
		id: String(o.id ?? ''),
		name: String(o.name ?? ''),
		shortcuts: o.shortcuts.filter(
			(s): s is ProfileShortcut =>
				!!s &&
				Array.isArray(s.keys) &&
				typeof s.command === 'string' &&
				typeof s.context === 'string'
		),
		mouse: o.mouse.filter(
			(m): m is ProfileMouse =>
				!!m &&
				typeof m.button === 'string' &&
				typeof m.action === 'string' &&
				Array.isArray(m.modifiers) &&
				typeof m.command === 'string' &&
				typeof m.context === 'string'
		),
	};
}
