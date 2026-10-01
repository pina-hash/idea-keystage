/**
 * The demo scripts. Each segment plays input the way keystage-core would
 * report it, through the same protocol messages:
 *
 *   SolidWorks segment: captions come only from the profile data file
 *     (data/profiles/solidworks.json), looked up with the core's matching
 *     rules (profile-data.ts). A gesture with no sourced entry is shown with
 *     no caption, exactly as the app would.
 *   General segment: the shortcuts-only behavior of the general profile:
 *     combos with no captions, a masked "Typing" tag with no key identity and
 *     no key messages, a chord, repeated arrows.
 *   States segment: the Paused and Hidden pills.
 */
import type { AppState, Chip, ServerMsg } from '../protocol';
import { defaultAppState } from '../protocol';
import { captionForKeys, captionForMouse, type ProfileData } from './profile-data';
import { Timeline, chipOf } from './timeline';

const SOLIDWORKS_REF = { id: 'solidworks', name: 'SolidWorks' } as const;

function mouseChip(key: string, label: string): Chip {
	return { key, label, kind: 'mouse' };
}

export function stateMsg(changes: Partial<AppState>): ServerMsg {
	return { t: 'state', state: { ...defaultAppState(), ...changes } };
}

/** Typed characters for the typed-value example (a length in a dimension box). */
const TYPED_VALUE = ['2', '5', '.', '4'];

export function solidworksSegment(t: Timeline, profile: ProfileData): void {
	const pid = profile.id || SOLIDWORKS_REF.id;
	const keys = (k: string[]) => ({ caption: captionForKeys(profile, k), profile: pid });

	t.emit(stateMsg({ profile: { id: pid, name: profile.name || SOLIDWORKS_REF.name } }));
	t.glide(0.46, 0.44, 400);

	// A single-key shortcut.
	t.combo(['F'], keys(['F']));
	t.glide(0.52, 0.48, 900);

	// Middle drag.
	t.button('middle', true, t.x, t.y).wait(40);
	t.glide(t.x + 0.02, t.y - 0.01, 160, 0);
	t.callout(
		t.newCallout('mouse', [[mouseChip('MiddleDrag', 'Middle drag')]], {
			caption: captionForMouse(profile, 'middle', 'drag', []),
			profile: pid,
		})
	);
	t.glide(t.x + 0.1, t.y + 0.06, 760, 0.12);
	t.button('middle', false, t.x, t.y);
	t.glide(0.5, 0.52, 700);

	// Ctrl + middle drag.
	t.key('Ctrl', true).wait(80);
	t.button('middle', true, t.x, t.y).wait(40);
	t.glide(t.x - 0.02, t.y, 160, 0);
	t.callout(
		t.newCallout('mouse', [[chipOf('Ctrl'), mouseChip('MiddleDrag', 'Middle drag')]], {
			caption: captionForMouse(profile, 'middle', 'drag', ['Ctrl']),
			profile: pid,
		})
	);
	t.glide(t.x - 0.12, t.y + 0.04, 700, 0.05);
	t.button('middle', false, t.x, t.y).wait(40);
	t.key('Ctrl', false);
	t.glide(0.42, 0.5, 600);

	// The wheel, three notches.
	let wheel = t.newCallout('mouse', [[mouseChip('WheelUp', 'Wheel up')]], {
		caption: captionForMouse(profile, 'middle', 'wheel', [], 'up'),
		profile: pid,
	});
	t.wheel(120, t.x, t.y);
	t.callout(wheel);
	for (let i = 0; i < 2; i++) {
		t.wait(140);
		t.wheel(120, t.x, t.y);
		wheel = t.update(wheel, { count: wheel.count + 1 });
	}
	t.wait(1300);

	// A combo with Shift.
	t.combo(['Shift', 'Z'], keys(['Shift', 'Z']));
	t.wait(1500);

	// A typed value: a text run closed by Enter.
	let run = t.newCallout('text', [[]], { profile: pid, text: '' });
	TYPED_VALUE.forEach((ch, i) => {
		t.key(ch, true);
		const text = TYPED_VALUE.slice(0, i + 1).join('');
		if (i === 0) {
			run = { ...run, text, updated_ms: Math.round(t.t) };
			t.callout(run);
		} else {
			run = t.update(run, { text });
		}
		t.wait(90);
		t.key(ch, false);
		t.wait(130);
	});
	t.wait(200);
	t.key('Enter', true);
	run = t.update(run, { steps: [[chipOf('Enter')]] });
	t.wait(110);
	t.key('Enter', false);
	t.wait(1400);

	// Undo, three times.
	let undo = t.combo(['Ctrl', 'Z'], keys(['Ctrl', 'Z']));
	t.wait(260);
	undo = t.repeat(undo, ['Ctrl', 'Z']);
	t.wait(260);
	undo = t.repeat(undo, ['Ctrl', 'Z']);
	t.wait(1400);

	// Save.
	t.combo(['Ctrl', 'S'], keys(['Ctrl', 'S']));
	t.glide(0.6, 0.42, 1400);

	// Space.
	t.combo(['Space'], keys(['Space']));
	t.wait(2200);
}

export function generalSegment(t: Timeline): void {
	t.emit(stateMsg({ profile: null }));
	t.glide(0.36, 0.4, 500);

	// A click (the general profile shows no mouse callouts; the cursor ripples).
	t.button('left', true, t.x, t.y).wait(90).button('left', false, t.x, t.y);
	t.wait(300);

	t.combo(['Ctrl', 'C'], {});
	t.wait(700);
	t.glide(0.44, 0.56, 500);
	t.button('left', true, t.x, t.y).wait(90).button('left', false, t.x, t.y);
	t.wait(200);
	t.combo(['Ctrl', 'V'], {});
	t.wait(1200);

	// Typing is masked: one tag, no key identity, no key messages.
	let typing = t.newCallout('masked', [[]]);
	t.callout(typing);
	t.wait(1500);
	t.key('Enter', true);
	typing = t.update(typing, { steps: [[chipOf('Enter')]] });
	t.wait(110);
	t.key('Enter', false);
	t.wait(1300);

	// An Alt chord: menu access keys.
	t.key('Alt', true).wait(90).key('Alt', false);
	let chord = t.newCallout('chord', [[chipOf('Alt')]]);
	t.callout(chord);
	t.wait(320);
	t.key('F', true);
	chord = t.update(chord, { steps: [[chipOf('Alt')], [chipOf('F')]] });
	t.wait(90).key('F', false).wait(300);
	t.key('S', true);
	chord = t.update(chord, { steps: [[chipOf('Alt')], [chipOf('F')], [chipOf('S')]] });
	t.wait(90).key('S', false);
	t.wait(1400);

	// Arrow keys, repeated.
	let up = t.combo(['Up'], {});
	t.wait(200);
	up = t.repeat(up, ['Up']);
	t.wait(200);
	up = t.repeat(up, ['Up']);
	t.wait(1200);

	t.combo(['Ctrl', 'Shift', 'S'], {});
	t.wait(1500);
	t.combo(['Backspace'], {});
	t.wait(2200);
}

export function statesSegment(t: Timeline): void {
	t.emit(stateMsg({ profile: null }));
	t.combo(['Ctrl', 'C'], {});
	t.wait(1200);
	t.emit(stateMsg({ paused: true }));
	t.wait(3000);
	t.emit(stateMsg({ paused: false, suppressed: 'password' }));
	t.wait(3000);
	t.emit(stateMsg({ suppressed: 'excluded' }));
	t.wait(3000);
	t.emit(stateMsg({ suppressed: null }));
	t.combo(['Ctrl', 'V'], {});
	t.wait(2400);
}
