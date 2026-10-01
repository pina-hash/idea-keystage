/**
 * The first-run wizard's steps and the output choice it writes into the
 * settings (host.native_overlay and host.hide_native_when_obs).
 */
import type { Settings } from '../types';

export const WIZARD_STEPS = [
	{ id: 'monitor', title: 'Monitor' },
	{ id: 'look', title: 'Look' },
	{ id: 'output', title: 'Output' },
	{ id: 'done', title: 'Done' },
] as const;
export type WizardStepId = (typeof WIZARD_STEPS)[number]['id'];

export type OutputChoice = 'obs' | 'native' | 'both';

export const OUTPUT_INFO: Readonly<Record<OutputChoice, { title: string; description: string }>> = {
	obs: {
		title: 'OBS Browser Sources',
		description: 'OBS draws the overlay into the recording. Nothing extra on your screen.',
	},
	native: {
		title: 'Native overlay',
		description: 'A transparent window over your screen. Any recorder or screen share picks it up.',
	},
	both: {
		title: 'Both',
		description:
			'OBS sources while OBS is connected; the native overlay steps aside then and returns after.',
	},
};

/** The output choice the current settings describe. */
export function outputOf(s: Pick<Settings, 'host'>): OutputChoice {
	if (!s.host.native_overlay) return 'obs';
	return s.host.hide_native_when_obs ? 'both' : 'native';
}

/** Writes an output choice into the settings. */
export function applyOutput(s: Pick<Settings, 'host'>, choice: OutputChoice): void {
	switch (choice) {
		case 'obs':
			s.host.native_overlay = false;
			s.host.hide_native_when_obs = true;
			break;
		case 'native':
			s.host.native_overlay = true;
			s.host.hide_native_when_obs = false;
			break;
		case 'both':
			s.host.native_overlay = true;
			s.host.hide_native_when_obs = true;
			break;
	}
}

export function stepIndex(id: WizardStepId): number {
	return WIZARD_STEPS.findIndex((s) => s.id === id);
}
