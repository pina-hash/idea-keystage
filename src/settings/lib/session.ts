/**
 * Reading a saved session (`session_get`, keystage.session/1 as written by
 * crates/keystage-session) for the Sessions page: its markers and counts.
 * Marker times are on the session's monotonic clock (`t_ms`, with the
 * session's own `start_ms`), so a marker's place in the session is
 * `t_ms - start_ms`. The reader is tolerant of a missing field: it needs
 * only `id`, `started` and `markers[].name`.
 */
import { isRecord } from '../../lib/protocol';
import type { ExportFormat } from '../types';
import { formatClock, safeFileName } from './format';

export interface SessionMarker {
	index: number;
	name: string;
	/** Milliseconds from the session start. */
	offsetMs: number;
	/** The OBS recording time of the marker, when the session had one. */
	timecode: string | null;
}

export interface SessionDetail {
	id: string;
	started: string;
	durationMs: number | null;
	callouts: number;
	markers: SessionMarker[];
}

function num(v: unknown): number | null {
	return typeof v === 'number' && Number.isFinite(v) ? v : null;
}

export function readSession(raw: unknown, fallbackId = ''): SessionDetail | null {
	if (!isRecord(raw)) return null;
	const startedMs = num(raw.start_ms) ?? num(raw.started_ms);
	const endMs = num(raw.end_ms);
	const startedUnix = num(raw.started_unix_ms);
	const endedUnix = num(raw.ended_unix_ms);
	const list = Array.isArray(raw.markers) ? raw.markers : [];
	const markers: SessionMarker[] = [];
	list.forEach((m, index) => {
		if (!isRecord(m)) return;
		const offset = num(m.offset_ms);
		const t = num(m.t_ms);
		let offsetMs = 0;
		if (offset !== null) offsetMs = offset;
		else if (t !== null && startedMs !== null && t >= startedMs) offsetMs = t - startedMs;
		else if (t !== null) offsetMs = t;
		markers.push({
			index,
			name: typeof m.name === 'string' ? m.name : '',
			offsetMs: Math.max(0, offsetMs),
			timecode: typeof m.timecode === 'string' && m.timecode.trim() ? m.timecode.trim() : null,
		});
	});
	const callouts = Array.isArray(raw.callouts) ? raw.callouts.length : 0;
	let durationMs = num(raw.duration_ms);
	if (durationMs === null && startedMs !== null && endMs !== null)
		durationMs = Math.max(0, endMs - startedMs);
	if (durationMs === null && startedUnix !== null && endedUnix !== null) {
		durationMs = Math.max(0, endedUnix - startedUnix);
	}
	return {
		id: typeof raw.id === 'string' ? raw.id : fallbackId,
		started: typeof raw.started === 'string' ? raw.started : '',
		durationMs,
		callouts,
		markers,
	};
}

export function markerTime(m: SessionMarker): string {
	return m.timecode ?? formatClock(m.offsetMs);
}

export interface ExportKind {
	format: ExportFormat;
	label: string;
	extension: string;
	filterName: string;
	help: string;
}

export const EXPORT_KINDS: readonly ExportKind[] = [
	{
		format: 'srt',
		label: 'Subtitles (.srt)',
		extension: 'srt',
		filterName: 'SubRip subtitles',
		help: 'One caption per callout, timed to the recording.',
	},
	{
		format: 'md',
		label: 'Shortcut list (.md)',
		extension: 'md',
		filterName: 'Markdown',
		help: 'The shortcuts used in the video, ready for a description.',
	},
	{
		format: 'chapters',
		label: 'YouTube chapters (.txt)',
		extension: 'txt',
		filterName: 'Text',
		help: 'Chapter lines from your markers, starting at 00:00.',
	},
	{
		format: 'json',
		label: 'Session data (.json)',
		extension: 'json',
		filterName: 'JSON',
		help: 'Everything the session log kept, as keystage.session/1.',
	},
];

/** "keystage-2026-09-30-1405-chapters.txt" from the session start time. */
export function exportFileName(started: string, kind: ExportKind): string {
	const d = new Date(started);
	const stamp = Number.isNaN(d.getTime())
		? 'session'
		: [
				d.getFullYear(),
				String(d.getMonth() + 1).padStart(2, '0'),
				String(d.getDate()).padStart(2, '0'),
				`${String(d.getHours()).padStart(2, '0')}${String(d.getMinutes()).padStart(2, '0')}`,
			].join('-');
	const suffix = kind.format === 'chapters' ? '-chapters' : '';
	return safeFileName(`keystage-${stamp}${suffix}.${kind.extension}`);
}

/** A marker name the app will store: trimmed, at most 120 characters. */
export function cleanMarkerName(name: string): string {
	return name.replace(/\s+/g, ' ').trim().slice(0, 120);
}
