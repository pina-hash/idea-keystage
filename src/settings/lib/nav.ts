/**
 * The window's pages, the preview route each one shows by default, and the
 * plain words for the app's status.
 */
import type { Route } from '../../lib/protocol';
import type { ObsState, Status } from '../types';
import type { IconName } from '../ui/icons';
import { formatTimecode } from './format';

export const PAGES = [
	'home',
	'keystrokes',
	'keyboard',
	'mouse',
	'display',
	'obs',
	'solidworks',
	'sessions',
	'privacy',
	'hotkeys',
	'sound',
	'general',
] as const;
export type PageId = (typeof PAGES)[number];

export interface PageInfo {
	id: PageId;
	title: string;
	icon: IconName;
	/** What the live preview shows on this page when it follows the page. */
	previewRoute: Route;
}

export const PAGE_INFO: Readonly<Record<PageId, PageInfo>> = {
	home: { id: 'home', title: 'Home', icon: 'home', previewRoute: 'all' },
	keystrokes: { id: 'keystrokes', title: 'Keystrokes', icon: 'keys', previewRoute: 'keys' },
	keyboard: { id: 'keyboard', title: 'Keyboard', icon: 'keyboard', previewRoute: 'keyboard' },
	mouse: { id: 'mouse', title: 'Mouse', icon: 'mouse', previewRoute: 'cursor' },
	display: { id: 'display', title: 'Display', icon: 'display', previewRoute: 'all' },
	obs: { id: 'obs', title: 'OBS', icon: 'broadcast', previewRoute: 'all' },
	solidworks: { id: 'solidworks', title: 'SolidWorks', icon: 'cube', previewRoute: 'keys' },
	sessions: { id: 'sessions', title: 'Sessions', icon: 'sessions', previewRoute: 'all' },
	privacy: { id: 'privacy', title: 'Privacy', icon: 'shield', previewRoute: 'all' },
	hotkeys: { id: 'hotkeys', title: 'Hotkeys', icon: 'command', previewRoute: 'all' },
	sound: { id: 'sound', title: 'Sound', icon: 'speaker', previewRoute: 'keys' },
	general: { id: 'general', title: 'General', icon: 'gear', previewRoute: 'all' },
};

export function isPageId(v: unknown): v is PageId {
	return typeof v === 'string' && (PAGES as readonly string[]).includes(v);
}

/** The page named by a location hash (`#obs`) or a `page` query, else Home. */
export function pageFromLocation(hash: string, search: string): PageId {
	const h = hash.replace(/^#/, '');
	if (isPageId(h)) return h;
	const q = new URLSearchParams(search).get('page');
	return isPageId(q) ? q : 'home';
}

export type Tone = 'ok' | 'warn' | 'danger' | 'off' | 'info';

export interface StatusWord {
	word: string;
	detail: string;
	tone: Tone;
}

/** Active, Paused, or Hidden (with the reason, never an app name). */
export function activityWord(s: Status): StatusWord {
	if (s.paused)
		return { word: 'Paused', detail: 'Input hooks removed. Nothing is shown.', tone: 'warn' };
	if (s.suppressed === 'password')
		return { word: 'Hidden', detail: 'A password field has focus.', tone: 'info' };
	if (s.suppressed === 'excluded')
		return { word: 'Hidden', detail: 'An excluded app is in front.', tone: 'info' };
	if (!s.hook.installed)
		return { word: 'Starting', detail: 'Waiting for the input hooks.', tone: 'warn' };
	return { word: 'Active', detail: 'Showing your input.', tone: 'ok' };
}

const OBS_WORDS: Readonly<Record<ObsState, { word: string; tone: Tone }>> = {
	off: { word: 'Off', tone: 'off' },
	connecting: { word: 'Connecting', tone: 'warn' },
	connected: { word: 'Connected', tone: 'ok' },
	auth_failed: { word: 'Sign-in failed', tone: 'danger' },
	error: { word: 'Not connected', tone: 'danger' },
};

export function obsWord(s: Status): StatusWord {
	if (!s.obs.enabled) {
		return { word: 'Off', detail: 'obs-websocket is not turned on in IDEA Keystage.', tone: 'off' };
	}
	const w = OBS_WORDS[s.obs.state];
	let detail = '';
	switch (s.obs.state) {
		case 'connected':
			detail = recordingText(s) ?? 'Not recording.';
			break;
		case 'connecting':
			detail = 'Trying to reach OBS.';
			break;
		case 'auth_failed':
			detail = s.obs.error ?? 'OBS did not accept the password.';
			break;
		case 'error':
			detail = s.obs.error ?? 'OBS did not answer.';
			break;
		case 'off':
			detail = 'Not connected.';
			break;
	}
	return { word: w.word, detail, tone: w.tone };
}

/** "Recording 00:12:41", "Recording paused 00:12:41", or null. */
export function recordingText(s: Status): string | null {
	const r = s.obs.recording;
	if (!r || !r.active) return null;
	const tc = formatTimecode(r.timecode);
	return r.paused ? `Recording paused ${tc}` : `Recording ${tc}`;
}

export function serverWord(s: Status): StatusWord {
	if (s.server.running) {
		const n = s.server.clients;
		return {
			word: 'Running',
			detail: `Port ${s.server.port}, ${n} ${n === 1 ? 'source' : 'sources'} connected.`,
			tone: 'ok',
		};
	}
	return {
		word: 'Stopped',
		detail: s.server.error ?? `Port ${s.server.port} is not serving.`,
		tone: 'danger',
	};
}
