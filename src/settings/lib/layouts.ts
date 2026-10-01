/**
 * Names and descriptions of the keyboard layouts the keyboard visual draws
 * (src/lib/renderer/keyboard/layouts.ts holds the geometry).
 */
import type { KeyboardLayout } from '../../lib/protocol';

export const LAYOUT_INFO: Readonly<Record<KeyboardLayout, { name: string; description: string }>> =
	{
		full: { name: 'Full size', description: 'Every key, with the number pad.' },
		tkl: { name: 'Tenkeyless', description: 'Full size without the number pad.' },
		'75': { name: '75 percent', description: 'Compact, with the function row and arrows.' },
		'60': { name: '60 percent', description: 'No function row, no arrow keys.' },
		laptop: { name: 'Laptop', description: 'A short function row and half-height arrows.' },
		left: {
			name: 'Left-hand zone',
			description: 'The keys your left hand uses beside the mouse. Takes the least room.',
		},
	};
