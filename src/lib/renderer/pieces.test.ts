import { describe, expect, it } from 'vitest';
import { defaultAppState, defaultOverlayConfig } from '../protocol';
import { NO_PIECES, indicatorsFor, visiblePieces } from './pieces';

describe('visible pieces', () => {
	const config = defaultOverlayConfig();
	const state = defaultAppState();

	it('draws nothing in Host A while native_visible is false', () => {
		expect(visiblePieces('all', config, { ...state, native_visible: false }, 'native')).toEqual(
			NO_PIECES
		);
		expect(visiblePieces('all', config, { ...state, native_visible: false }, 'obs').callouts).toBe(
			true
		);
	});

	it('dedicated routes draw their piece whatever enabled says', () => {
		const off = { ...config, keyboard: { ...config.keyboard, enabled: false } };
		expect(visiblePieces('keyboard', off, state, 'obs').keyboard).toBe(true);
		expect(
			visiblePieces('keyboard', off, { ...state, keyboard_visible: false }, 'obs').keyboard
		).toBe(false);
		expect(visiblePieces('keys', config, state, 'obs')).toMatchObject({
			callouts: true,
			keyboard: false,
		});
		expect(visiblePieces('mouse', config, state, 'obs')).toMatchObject({
			mouse: true,
			callouts: false,
		});
		expect(visiblePieces('cursor', config, state, 'obs')).toMatchObject({
			cursor: true,
			indicators: false,
		});
	});

	it('the all route follows each enabled switch', () => {
		const all = visiblePieces('all', config, state, 'native');
		expect(all).toEqual({
			callouts: true,
			keyboard: false,
			mouse: false,
			cursor: false,
			indicators: true,
		});
		const on = {
			...config,
			keyboard: { ...config.keyboard, enabled: true },
			cursor: { ...config.cursor, enabled: true },
		};
		expect(visiblePieces('all', on, state, 'native')).toMatchObject({
			keyboard: true,
			cursor: true,
		});
	});

	it('shows Paused or Hidden only when the indicators allow it, never naming an app', () => {
		expect(indicatorsFor(config, state)).toEqual([]);
		expect(indicatorsFor(config, { ...state, paused: true })).toEqual([
			{ kind: 'paused', detail: null },
		]);
		expect(indicatorsFor(config, { ...state, suppressed: 'password' })).toEqual([
			{ kind: 'hidden', detail: 'Password field' },
		]);
		expect(indicatorsFor(config, { ...state, suppressed: 'excluded' })[0]?.detail).toBe(
			'Excluded app'
		);
		const quiet = { ...config, indicators: { paused: false, suppressed: false } };
		expect(indicatorsFor(quiet, { ...state, paused: true, suppressed: 'password' })).toEqual([]);
	});
});
