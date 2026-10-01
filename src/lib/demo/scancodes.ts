/**
 * Physical key ids for the demo scripts, so the keyboard visual lights the
 * right keys. Wire format (ARCHITECTURE.md section 3): "sc:1E", or
 * "sc:E01D" for an extended key.
 *
 * Values are the "Scan 1 Make" column of Microsoft's scan code table:
 * https://learn.microsoft.com/en-us/windows/win32/inputdev/about-keyboard-input#scan-codes
 * (read 2026-10-01).
 */
import type { Modifier } from '../protocol';

export const DEMO_SCAN_CODES: Readonly<Record<string, string>> = {
	A: 'sc:1E',
	B: 'sc:30',
	C: 'sc:2E',
	D: 'sc:20',
	E: 'sc:12',
	F: 'sc:21',
	G: 'sc:22',
	H: 'sc:23',
	I: 'sc:17',
	J: 'sc:24',
	K: 'sc:25',
	L: 'sc:26',
	M: 'sc:32',
	N: 'sc:31',
	O: 'sc:18',
	P: 'sc:19',
	Q: 'sc:10',
	R: 'sc:13',
	S: 'sc:1F',
	T: 'sc:14',
	U: 'sc:16',
	V: 'sc:2F',
	W: 'sc:11',
	X: 'sc:2D',
	Y: 'sc:15',
	Z: 'sc:2C',
	'1': 'sc:02',
	'2': 'sc:03',
	'3': 'sc:04',
	'4': 'sc:05',
	'5': 'sc:06',
	'6': 'sc:07',
	'7': 'sc:08',
	'8': 'sc:09',
	'9': 'sc:0A',
	'0': 'sc:0B',
	'-': 'sc:0C',
	'=': 'sc:0D',
	'.': 'sc:34',
	',': 'sc:33',
	'/': 'sc:35',
	Enter: 'sc:1C',
	Esc: 'sc:01',
	Backspace: 'sc:0E',
	Tab: 'sc:0F',
	Space: 'sc:39',
	CapsLock: 'sc:3A',
	F1: 'sc:3B',
	F2: 'sc:3C',
	F3: 'sc:3D',
	F4: 'sc:3E',
	F5: 'sc:3F',
	F6: 'sc:40',
	F7: 'sc:41',
	F8: 'sc:42',
	F9: 'sc:43',
	F10: 'sc:44',
	F11: 'sc:57',
	F12: 'sc:58',
	Insert: 'sc:E052',
	Home: 'sc:E047',
	PageUp: 'sc:E049',
	Delete: 'sc:E053',
	End: 'sc:E04F',
	PageDown: 'sc:E051',
	Right: 'sc:E04D',
	Left: 'sc:E04B',
	Down: 'sc:E050',
	Up: 'sc:E048',
	Menu: 'sc:E05D',
	Ctrl: 'sc:1D',
	Shift: 'sc:2A',
	Alt: 'sc:38',
	Win: 'sc:E05B',
};

export const DEMO_MODIFIERS: Readonly<Record<string, Modifier>> = {
	Ctrl: 'ctrl',
	Shift: 'shift',
	Alt: 'alt',
	Win: 'win',
};

/** The canonical modifier order for a combo (ARCHITECTURE.md section 4). */
export const MODIFIER_ORDER: readonly string[] = ['Ctrl', 'Shift', 'Alt', 'AltGr', 'Win'];

export function physOf(key: string): string | null {
	return DEMO_SCAN_CODES[key] ?? null;
}
