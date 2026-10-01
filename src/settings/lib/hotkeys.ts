/**
 * The five global hotkeys: what each does, and what is wrong with a
 * candidate shortcut before it is saved. The rules mirror keystage-core
 * `Settings::validate_hotkeys` (a command modifier or a function key, a key
 * the global-shortcut plugin can register, no two hotkeys alike); the app
 * also reports shortcuts Windows refused (`hotkeys_status`), and a shortcut
 * the SolidWorks data lists is flagged from that data.
 */
import {
	HOTKEY_NAMES,
	type HotkeyName,
	type HotkeyState,
	type Hotkeys,
	type ProfileData,
} from '../types';
import {
	formatAccelerator,
	hasCommandModifier,
	isFunctionKey,
	isRegistrable,
	parseAccelerator,
	type Accelerator,
} from './accelerator';
import { sameKeySet } from './shortcuts';

export const HOTKEY_INFO: Readonly<Record<HotkeyName, { title: string; help: string }>> = {
	pause: {
		title: 'Pause and resume',
		help: 'Stops showing input and removes the input hooks until you press it again.',
	},
	toggle_keyboard: {
		title: 'Keyboard visual',
		help: 'Shows or hides the keyboard visual everywhere.',
	},
	toggle_solidworks: {
		title: 'SolidWorks mode',
		help: 'Steps through Auto, On and Off.',
	},
	chapter_marker: {
		title: 'Chapter marker',
		help: 'Adds a chapter marker to the session log that is running.',
	},
	session_toggle: {
		title: 'Session log',
		help: 'Starts or stops a session log.',
	},
};

export interface HotkeyProblem {
	level: 'error' | 'warning';
	message: string;
}

export interface HotkeyCheckContext {
	hotkeys: Hotkeys;
	profile?: ProfileData | null;
	states?: readonly HotkeyState[];
}

/** Problems that stop a shortcut from being saved; empty when it can be used. */
export function blockingProblems(
	name: HotkeyName,
	accel: Accelerator,
	hotkeys: Hotkeys
): HotkeyProblem[] {
	const out: HotkeyProblem[] = [];
	if (!hasCommandModifier(accel) && !isFunctionKey(accel.key)) {
		out.push({
			level: 'error',
			message: 'Add Ctrl, Alt or Win, or use a function key (F1 to F24).',
		});
	} else if (!isRegistrable(accel)) {
		out.push({
			level: 'error',
			message: accel.mods.includes('AltGr')
				? 'AltGr cannot be part of a global hotkey.'
				: `${accel.key} cannot be used in a global hotkey.`,
		});
	}
	const text = formatAccelerator(accel);
	for (const other of HOTKEY_NAMES) {
		if (other === name) continue;
		const theirs = parseAccelerator(hotkeys[other]);
		if (theirs.ok && formatAccelerator(theirs.accel) === text) {
			out.push({ level: 'error', message: `Already used by ${HOTKEY_INFO[other].title}.` });
		}
	}
	return out;
}

/** Everything worth saying about the hotkey as it is stored now. */
export function hotkeyProblems(name: HotkeyName, ctx: HotkeyCheckContext): HotkeyProblem[] {
	const value = ctx.hotkeys[name];
	const parsed = parseAccelerator(value);
	if (!parsed.ok) return [{ level: 'error', message: parsed.error }];
	const out = blockingProblems(name, parsed.accel, ctx.hotkeys);
	const state = ctx.states?.find((s) => s.name === name);
	if (state && !state.registered) {
		out.push({
			level: 'error',
			message: state.error
				? `Windows did not accept it: ${state.error}`
				: 'Windows did not accept it. Another program may already use it.',
		});
	}
	const conflict = solidworksConflict(parsed.accel, ctx.profile ?? null);
	if (conflict) out.push({ level: 'warning', message: conflict });
	return out;
}

/**
 * A warning when the SolidWorks data lists the same keys: while the hotkey
 * is registered, SolidWorks never receives that shortcut. The caption is
 * the data file's command name.
 */
export function solidworksConflict(accel: Accelerator, profile: ProfileData | null): string | null {
	if (!profile) return null;
	const keys = [...accel.mods, accel.key];
	const hit = profile.shortcuts.find(
		(s) => s.keys.length === keys.length && sameKeySet(s.keys, keys)
	);
	if (!hit) return null;
	return `SolidWorks uses this for "${hit.command.trim()}". While it is a hotkey, SolidWorks will not receive it.`;
}
