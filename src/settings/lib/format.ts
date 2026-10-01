/**
 * Small formatters for the settings window. Plain ASCII, American English,
 * sentence case.
 */

/** 2200 -> "2.2 s", 450 -> "450 ms", 15000 -> "15 s". */
export function formatMs(ms: number): string {
	if (!Number.isFinite(ms)) return '';
	const v = Math.max(0, Math.round(ms));
	if (v < 1000) return `${v} ms`;
	const s = v / 1000;
	const text = Number.isInteger(s)
		? String(s)
		: s
				.toFixed(s < 10 ? 2 : 1)
				.replace(/0+$/, '')
				.replace(/\.$/, '');
	return `${text} s`;
}

/** Milliseconds as a clock: "4:05", "1:02:03" (hours only when needed). */
export function formatClock(ms: number): string {
	const total = Math.max(0, Math.floor((Number.isFinite(ms) ? ms : 0) / 1000));
	const h = Math.floor(total / 3600);
	const m = Math.floor((total % 3600) / 60);
	const s = total % 60;
	const ss = String(s).padStart(2, '0');
	if (h > 0) return `${h}:${String(m).padStart(2, '0')}:${ss}`;
	return `${m}:${ss}`;
}

/** "26 min", "1 h 4 min", "45 s". */
export function formatDuration(ms: number): string {
	const total = Math.max(0, Math.round((Number.isFinite(ms) ? ms : 0) / 1000));
	if (total < 60) return `${total} s`;
	const minutes = Math.round(total / 60);
	if (minutes < 60) return `${minutes} min`;
	const h = Math.floor(minutes / 60);
	const m = minutes % 60;
	return m === 0 ? `${h} h` : `${h} h ${m} min`;
}

/**
 * OBS's `HH:MM:SS.mmm` record timecode shown without the milliseconds
 * ("00:12:41"). Anything else is shown as given, trimmed.
 */
export function formatTimecode(tc: string): string {
	const t = tc.trim();
	const m = /^(\d{1,3}):(\d{2}):(\d{2})(?:\.\d+)?$/.exec(t);
	if (!m) return t;
	return `${m[1]!.padStart(2, '0')}:${m[2]}:${m[3]}`;
}

const MONTHS = [
	'Jan',
	'Feb',
	'Mar',
	'Apr',
	'May',
	'Jun',
	'Jul',
	'Aug',
	'Sep',
	'Oct',
	'Nov',
	'Dec',
] as const;

/** ISO 8601 to "Sep 30, 2026, 2:05 PM" in local time; the input as given when unreadable. */
export function formatDateTime(iso: string): string {
	const d = new Date(iso);
	if (Number.isNaN(d.getTime())) return iso;
	const h24 = d.getHours();
	const h = h24 % 12 === 0 ? 12 : h24 % 12;
	const ampm = h24 < 12 ? 'AM' : 'PM';
	const min = String(d.getMinutes()).padStart(2, '0');
	return `${MONTHS[d.getMonth()]} ${d.getDate()}, ${d.getFullYear()}, ${h}:${min} ${ampm}`;
}

/** ISO 8601 to "2:05 PM". */
export function formatTime(iso: string): string {
	const d = new Date(iso);
	if (Number.isNaN(d.getTime())) return iso;
	const h24 = d.getHours();
	const h = h24 % 12 === 0 ? 12 : h24 % 12;
	return `${h}:${String(d.getMinutes()).padStart(2, '0')} ${h24 < 12 ? 'AM' : 'PM'}`;
}

/** Milliseconds since an ISO time, never negative; 0 when unreadable. */
export function elapsedSince(iso: string, now: number): number {
	const t = new Date(iso).getTime();
	return Number.isNaN(t) ? 0 : Math.max(0, now - t);
}

/** "1 marker", "3 markers". */
export function plural(n: number, one: string, many = `${one}s`): string {
	return `${n.toLocaleString('en-US')} ${n === 1 ? one : many}`;
}

/** 0.6 -> "60%". */
export function formatPercent(v: number): string {
	return `${Math.round((Number.isFinite(v) ? v : 0) * 100)}%`;
}

/** 1.25 -> "1.25x". */
export function formatScale(v: number): string {
	const r = Math.round(v * 100) / 100;
	return `${r}x`;
}

/** A file name safe on Windows: no reserved characters, no trailing dots or spaces. */
export function safeFileName(name: string): string {
	const cleaned = name
		.replace(/[<>:"/\\|?*\u0000-\u001f]/g, '-')
		.replace(/\s+/g, ' ')
		.trim()
		.replace(/[. ]+$/, '');
	return cleaned.length > 0 ? cleaned : 'keystage';
}
