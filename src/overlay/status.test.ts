import { describe, expect, it } from 'vitest';
import { drawsPieces, showWaitingCard, waitingText } from './status';

describe('connection card', () => {
	it('never shows inside OBS unless status=1, and never once live', () => {
		const base = {
			connection: 'retrying' as const,
			transport: 'ws' as const,
			inObs: false,
			status: false,
		};
		expect(showWaitingCard(base)).toBe(true);
		expect(showWaitingCard({ ...base, inObs: true })).toBe(false);
		expect(showWaitingCard({ ...base, inObs: true, status: true })).toBe(true);
		expect(showWaitingCard({ ...base, connection: 'live' })).toBe(false);
		expect(showWaitingCard({ ...base, transport: 'tauri' })).toBe(false);
		expect(showWaitingCard({ ...base, transport: 'demo' })).toBe(false);
	});

	it('draws pieces on a WebSocket page only while live', () => {
		for (const c of ['idle', 'connecting', 'open', 'retrying', 'failed'] as const) {
			expect(drawsPieces({ connection: c, transport: 'ws' })).toBe(false);
			expect(drawsPieces({ connection: c, transport: 'tauri' })).toBe(true);
			expect(drawsPieces({ connection: c, transport: 'demo' })).toBe(true);
		}
		expect(drawsPieces({ connection: 'live', transport: 'ws' })).toBe(true);
	});

	it('names the address it is trying, in plain words', () => {
		const t = waitingText('connecting', '127.0.0.1', '47820');
		expect(t.title).toBe('Waiting for IDEA Keystage');
		expect(t.line).toBe('Connecting to 127.0.0.1:47820');
		expect(waitingText('retrying', '', '47821').line).toBe('Trying 127.0.0.1:47821 again');
		expect(waitingText('open', 'localhost', '47820').line).toBe('Connected to localhost:47820');
		for (const s of ['connecting', 'retrying', 'open'] as const) {
			const all = Object.values(waitingText(s, 'h', '1')).join(' ');
			expect(all).not.toMatch(/\u2014/);
		}
	});
});
