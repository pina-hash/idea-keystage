/**
 * When the overlay page shows its connection card.
 *
 * Inside OBS (window.obsstudio exists) a source that is not connected is
 * fully transparent (no pieces, see drawsPieces, and no card), so a status
 * banner or a stale keyboard is never recorded, unless the URL
 * asks for status=1. Host A and the settings preview never show the card
 * (they are not WebSocket pages). The demo never needs it. In an ordinary
 * browser the card shows until the first hello arrives, after a short grace
 * period handled by the card's own enter delay.
 */
import type { ConnectionStatus } from '../lib/renderer/engine';
import type { TransportKind } from '../lib/renderer/options';

export interface CardInput {
	connection: ConnectionStatus;
	transport: TransportKind;
	inObs: boolean;
	status: boolean;
}

/**
 * Whether the page draws its pieces (keyboard, mouse, callouts, cursor).
 *
 * A WebSocket page draws only while live: before the first hello it has no
 * user config yet (it would draw the built-in defaults, then jump), and after
 * the app quits or restarts its last config is stale. So a disconnected OBS
 * source is fully transparent (ARCHITECTURE.md section 6), and in a browser
 * the waiting card shows over an empty stage. Host A, the settings preview
 * and the demo are not WebSocket pages and always draw.
 */
export function drawsPieces(i: Pick<CardInput, 'connection' | 'transport'>): boolean {
	return i.transport !== 'ws' || i.connection === 'live';
}

export function showWaitingCard(i: CardInput): boolean {
	if (i.transport !== 'ws') return false;
	if (i.connection === 'live') return false;
	if (i.inObs && !i.status) return false;
	return true;
}

export interface CardText {
	title: string;
	line: string;
	hint: string;
}

/** The card's words for each connection state (sentence case, short, plain). */
export function waitingText(connection: ConnectionStatus, host: string, port: string): CardText {
	const where = `${host || '127.0.0.1'}:${port}`;
	const title = 'Waiting for IDEA Keystage';
	if (connection === 'open') {
		return { title, line: `Connected to ${where}`, hint: 'Loading your overlay settings.' };
	}
	if (connection === 'retrying') {
		return {
			title,
			line: `Trying ${where} again`,
			hint: 'Start IDEA Keystage on this computer. This page connects on its own.',
		};
	}
	return {
		title,
		line: `Connecting to ${where}`,
		hint: 'Start IDEA Keystage on this computer. This page connects on its own.',
	};
}
