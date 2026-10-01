/**
 * The excluded apps list: process names, compared case-insensitively, with
 * or without `.exe` (keystage-core `PrivacySettings::is_excluded`). The
 * editor cleans an entry the same way core's validation does, so what the
 * user sees is what the app stores.
 */

/** The file name of a path (`C:\Tools\Vault.exe` -> `Vault.exe`). */
export function exeFileName(entry: string): string {
	const t = entry.trim().replace(/^"+|"+$/g, '');
	const parts = t.split(/[\\/]/);
	return (parts[parts.length - 1] ?? '').trim();
}

function stem(name: string): string {
	return name.toLowerCase().endsWith('.exe') ? name.slice(0, -4) : name;
}

/** Same app: equal names ignoring case, or equal without `.exe`. */
export function sameApp(a: string, b: string): boolean {
	const x = a.trim().toLowerCase();
	const y = b.trim().toLowerCase();
	return x === y || stem(x) === stem(y);
}

export type AddResult =
	{ ok: true; list: string[]; added: string; note: string | null } | { ok: false; error: string };

const INVALID = /[<>:"|?*\u0000-\u001f]/;

/** Adds one entry to the list, or says why it cannot be added. */
export function addExcludedApp(list: readonly string[], raw: string): AddResult {
	const trimmed = raw.trim();
	if (trimmed.length === 0)
		return { ok: false, error: 'Type a process name, for example KeePass.exe.' };
	const name = exeFileName(trimmed);
	if (name.length === 0) return { ok: false, error: 'That path has no file name.' };
	if (INVALID.test(name))
		return { ok: false, error: 'A process name cannot contain < > : " | ? or *.' };
	if (list.some((e) => sameApp(e, name)))
		return { ok: false, error: `${name} is already in the list.` };
	const note = name !== trimmed ? `Saved as ${name}: only the file name is used.` : null;
	return { ok: true, list: [...list, name], added: name, note };
}

export function removeExcludedApp(list: readonly string[], name: string): string[] {
	return list.filter((e) => e !== name);
}

/** Core's cleanup: trimmed, paths shortened to file names, duplicates dropped. */
export function normalizeExcludedApps(list: readonly string[]): string[] {
	const out: string[] = [];
	for (const raw of list) {
		const name = exeFileName(raw);
		if (name.length === 0) continue;
		if (!out.some((e) => e.toLowerCase() === name.toLowerCase())) out.push(name);
	}
	return out;
}
