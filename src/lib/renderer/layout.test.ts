import { describe, expect, it } from 'vitest';
import { anchorStyle, flexAlign, horizontalOf, verticalOf } from './layout';
import { ANCHORS } from '../protocol';

describe('anchor placement', () => {
	it('places each anchor with inward offsets scaled', () => {
		expect(anchorStyle('bottom', 0, 96, 1)).toBe(
			'left: 50%; bottom: 96px; transform: translate(calc(-50% + 0px), 0px)'
		);
		expect(anchorStyle('top-left', 10, 20, 2)).toBe('left: 20px; top: 40px');
		expect(anchorStyle('bottom-right', 32, 32, 1)).toBe('right: 32px; bottom: 32px');
		expect(anchorStyle('center', 5, -5, 1)).toBe(
			'left: 50%; top: 50%; transform: translate(calc(-50% + 5px), calc(-50% + -5px))'
		);
		expect(anchorStyle('left', 8, 0, 1)).toBe(
			'left: 8px; top: 50%; transform: translate(0px, calc(-50% + 0px))'
		);
		expect(anchorStyle('top-left', Number.NaN, 4, 0)).toBe('left: 0px; top: 4px');
	});

	it('aligns stacks to the anchored side', () => {
		const expected: Record<string, string> = {
			'top-left': 'flex-start',
			left: 'flex-start',
			'bottom-left': 'flex-start',
			top: 'center',
			center: 'center',
			bottom: 'center',
			'top-right': 'flex-end',
			right: 'flex-end',
			'bottom-right': 'flex-end',
		};
		for (const a of ANCHORS) expect(flexAlign(a)).toBe(expected[a]);
		expect(horizontalOf('right')).toBe('end');
		expect(verticalOf('top-right')).toBe('start');
		expect(verticalOf('left')).toBe('center');
	});
});
