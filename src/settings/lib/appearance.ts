/**
 * The window's own look: IDEA (dark, the default) or Space White (light).
 * This is a preference of this window only, kept in this window's local
 * storage; it changes nothing the overlay shows. `?appearance=light` on the
 * page address overrides it (screenshots).
 */
export type Appearance = 'dark' | 'light';

export const APPEARANCE_KEY = 'keystage.settings.appearance';

interface StorageLike {
	getItem(key: string): string | null;
	setItem(key: string, value: string): void;
}

function safeStorage(): StorageLike | null {
	try {
		return typeof localStorage !== 'undefined' ? localStorage : null;
	} catch {
		return null;
	}
}

export function readAppearance(
	search: string,
	storage: StorageLike | null = safeStorage()
): Appearance {
	const q = new URLSearchParams(search).get('appearance');
	if (q === 'light' || q === 'dark') return q;
	try {
		const v = storage?.getItem(APPEARANCE_KEY);
		if (v === 'light' || v === 'dark') return v;
	} catch {
		/* storage blocked: use the default */
	}
	return 'dark';
}

export function saveAppearance(a: Appearance, storage: StorageLike | null = safeStorage()): void {
	try {
		storage?.setItem(APPEARANCE_KEY, a);
	} catch {
		/* storage blocked: the choice lasts until the window closes */
	}
}

/** Sets `data-plate` on the root element (plate.css and tokens.css read it). */
export function applyAppearance(a: Appearance, root: HTMLElement = document.documentElement): void {
	if (a === 'light') root.dataset.plate = 'space-white';
	else delete root.dataset.plate;
}
