/**
 * Three-way merge of JSON values, for settings that change in two places at
 * once: this window edits a copy, while the app changes the stored settings
 * itself (the tray's SolidWorks mode and Native overlay items, the
 * SolidWorks mode hotkey, the OBS password).
 *
 * `merge3(base, mine, theirs)`: `mine` is `base` with this window's edits,
 * `theirs` is what the app holds now. Objects are merged key by key; any
 * other value (a string, a number, null, an array such as the excluded apps)
 * is one value. A value this window changed from `base` keeps its edit;
 * every other value is taken from `theirs`. So an edit is never lost, and a
 * change made elsewhere is never overwritten by a stale copy.
 */
import { sameJson } from '../defaults';

type Rec = Record<string, unknown>;

function isPlainObject(v: unknown): v is Rec {
	return typeof v === 'object' && v !== null && !Array.isArray(v);
}

function copy(v: unknown): unknown {
	return v === undefined ? undefined : (JSON.parse(JSON.stringify(v)) as unknown);
}

function mergeValue(base: unknown, mine: unknown, theirs: unknown): unknown {
	if (isPlainObject(base) && isPlainObject(mine) && isPlainObject(theirs)) {
		const out: Rec = {};
		for (const k of new Set([...Object.keys(theirs), ...Object.keys(mine)])) {
			const v = mergeValue(base[k], mine[k], theirs[k]);
			if (v !== undefined) out[k] = v;
		}
		return out;
	}
	return copy(sameJson(mine, base) ? theirs : mine);
}

/** `theirs` with every value `mine` changed from `base` (a fresh copy). */
export function merge3<T>(base: T, mine: T, theirs: T): T {
	return mergeValue(base, mine, theirs) as T;
}
