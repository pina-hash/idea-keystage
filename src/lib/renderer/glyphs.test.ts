import { describe, expect, it } from 'vitest';
import type { Callout, Chip } from '../protocol';
import {
	calloutText,
	glyphFor,
	isShortLabel,
	mouseChipInfo,
	repeatText,
	textRunDisplay,
} from './glyphs';

const chip = (key: string, label = key, kind: Chip['kind'] = 'mouse'): Chip => ({
	key,
	label,
	kind,
});

describe('glyphs', () => {
	it('maps arrows, Enter, Tab and Backspace, keeping Num on numpad keys', () => {
		expect(glyphFor('Up')?.id).toBe('up');
		expect(glyphFor('Enter')).toEqual({ id: 'enter', prefix: null, words: 'Enter' });
		expect(glyphFor('NumEnter')).toEqual({ id: 'enter', prefix: 'Num', words: 'Num Enter' });
		expect(glyphFor('NumLeft')?.prefix).toBe('Num');
		expect(glyphFor('NumTab')).toBeNull();
		expect(glyphFor('A')).toBeNull();
		expect(glyphFor('Num5')).toBeNull();
	});

	it('reads mouse chips by their canonical id or label', () => {
		expect(mouseChipInfo(chip('MiddleDrag', 'Middle drag'))).toEqual({
			part: 'middle',
			hint: null,
		});
		expect(mouseChipInfo(chip('WheelUp', 'Wheel up'))).toEqual({ part: 'wheel', hint: 'up' });
		expect(mouseChipInfo(chip('WheelDown', 'Wheel down'))).toEqual({ part: 'wheel', hint: 'down' });
		expect(mouseChipInfo(chip('LeftDoubleClick', 'Left double click')).part).toBe('left');
		expect(mouseChipInfo(chip('RightGestureLeft', 'Right gesture left'))).toEqual({
			part: 'right',
			hint: 'left',
		});
		expect(mouseChipInfo(chip('X1Click', 'X1 click')).part).toBe('x1');
		expect(mouseChipInfo(chip('Thing', 'Thing'))).toEqual({ part: null, hint: null });
	});

	it('formats badges, long text runs and short labels', () => {
		expect(repeatText(1)).toBeNull();
		expect(repeatText(4)).toBe('×4');
		expect(textRunDisplay('25.4')).toBe('25.4');
		const long = 'x'.repeat(60);
		const shown = textRunDisplay(long, 10);
		expect(shown).toBe('…' + 'x'.repeat(9));
		expect(isShortLabel('A')).toBe(true);
		expect(isShortLabel('Ctrl')).toBe(false);
	});

	it('writes the plain text form like keystage-core', () => {
		const base: Callout = {
			id: 1,
			kind: 'combo',
			steps: [[chip('Ctrl', 'Ctrl', 'modifier'), chip('Z', 'Z', 'key')]],
			text: null,
			count: 4,
			caption: null,
			context: null,
			profile: null,
			t_ms: 0,
			updated_ms: 0,
		};
		expect(calloutText(base)).toBe('Ctrl + Z x4');
		expect(
			calloutText({
				...base,
				kind: 'chord',
				count: 1,
				steps: [[chip('Alt', 'Alt', 'modifier')], [chip('F', 'F', 'key')], [chip('C', 'C', 'key')]],
			})
		).toBe('Alt, F, C');
		expect(
			calloutText({
				...base,
				kind: 'text',
				count: 1,
				text: '25.4',
				steps: [[chip('Enter', 'Enter', 'key')]],
			})
		).toBe('25.4 Enter');
		expect(calloutText({ ...base, kind: 'masked', count: 1, steps: [[]] })).toBe('Typing');
	});
});
