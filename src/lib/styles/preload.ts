/**
 * Loads the bundled font faces (fonts.css) up front.
 *
 * The browser fetches an @font-face only when some text first needs it. On
 * a WebSocket page that is too late for anything first drawn after the app
 * has gone away (the waiting card's Rajdhani hint): the page's server is
 * gone, the request is refused and the face stays in the error state, so
 * the text falls back to a system font. The page itself was served by the
 * app, so at page load every face can be fetched; loading them then also
 * keeps the first callout of a session from flashing in a fallback font.
 *
 * `FontFaceSet.load` with no sample text loads the faces that cover a space,
 * so only the Latin subsets are fetched; the Latin Extended faces of
 * Rajdhani still load on demand.
 */

/** One CSS `font` shorthand per bundled face (keep in step with fonts.css). */
export const BUNDLED_FONTS: readonly string[] = [
	"500 1em 'Rajdhani'",
	"600 1em 'Rajdhani'",
	"700 1em 'Rajdhani'",
	"400 1em 'Share Tech Mono'",
	"500 1em 'Orbitron'",
	"700 1em 'Orbitron'",
];

export interface FontLoader {
	load(font: string): Promise<unknown>;
}

/** Starts loading every bundled face; resolves when all have settled, never rejects. */
export function preloadFonts(fonts: FontLoader | null | undefined): Promise<void> {
	if (!fonts || typeof fonts.load !== 'function') return Promise.resolve();
	return Promise.all(
		BUNDLED_FONTS.map((f) => {
			try {
				return fonts.load(f).catch(() => undefined);
			} catch {
				return undefined;
			}
		})
	).then(() => undefined);
}
