/**
 * Saves the settings draft a moment after the last change, one save at a
 * time, and tells the owner what the app stored.
 *
 * * `push(value)` after every edit; nothing is sent while the value equals
 *   the last value the app confirmed (`baseline`).
 * * Edits during a save are kept and sent when it finishes.
 * * `onSaved(sent, stored)`: the app may correct a value (clamps, a hotkey
 *   it could not use); the owner adopts `stored` when the draft still
 *   equals `sent`, and calls `setBaseline(stored)` first so adopting it does
 *   not trigger another save.
 */

export type SaveState = 'idle' | 'pending' | 'saving' | 'saved' | 'error';

export interface SaveTimers {
	setTimeout(fn: () => void, ms: number): unknown;
	clearTimeout(handle: unknown): void;
}

export const systemSaveTimers: SaveTimers = {
	setTimeout: (fn, ms) => setTimeout(fn, ms),
	clearTimeout: (h) => clearTimeout(h as ReturnType<typeof setTimeout>),
};

export interface SaveQueueOptions<T> {
	delayMs: number;
	save: (value: T) => Promise<T>;
	equal: (a: T, b: T) => boolean;
	onSaved: (sent: T, stored: T) => void;
	onState: (state: SaveState, error: string | null) => void;
	describeError?: (e: unknown) => string;
	timers?: SaveTimers;
}

export class SaveQueue<T> {
	private readonly opts: SaveQueueOptions<T>;
	private readonly timers: SaveTimers;
	private baseline: T | null = null;
	private latest: T | null = null;
	private timer: unknown = null;
	private inFlight: Promise<void> | null = null;
	private state: SaveState = 'idle';

	constructor(opts: SaveQueueOptions<T>) {
		this.opts = opts;
		this.timers = opts.timers ?? systemSaveTimers;
	}

	/** The value the app last confirmed. Pending edits that equal it are dropped. */
	setBaseline(value: T): void {
		this.baseline = value;
		if (this.latest !== null && this.opts.equal(this.latest, value)) {
			this.latest = null;
			this.cancelTimer();
			if (!this.inFlight && (this.state === 'pending' || this.state === 'error')) {
				this.setState('saved', null);
			}
		}
	}

	get current(): SaveState {
		return this.state;
	}

	/** True while an edit has not been confirmed by the app. */
	get dirty(): boolean {
		return this.latest !== null || this.inFlight !== null;
	}

	push(value: T): void {
		if (this.baseline !== null && this.opts.equal(value, this.baseline) && !this.inFlight) {
			this.latest = null;
			this.cancelTimer();
			return;
		}
		this.latest = value;
		this.setState(this.inFlight ? 'saving' : 'pending', null);
		this.cancelTimer();
		this.timer = this.timers.setTimeout(() => {
			this.timer = null;
			void this.run();
		}, this.opts.delayMs);
	}

	/**
	 * Sends anything waiting now and resolves when the app has answered. After
	 * a failed save `current` is 'error' and the edit is kept.
	 */
	async flush(): Promise<void> {
		this.cancelTimer();
		if (this.inFlight) await this.inFlight;
		if (this.latest !== null) await this.run();
	}

	/** Drops a waiting edit (the window is closing, or the app replaced the settings). */
	cancel(): void {
		this.cancelTimer();
		this.latest = null;
	}

	private cancelTimer(): void {
		if (this.timer !== null) this.timers.clearTimeout(this.timer);
		this.timer = null;
	}

	private run(): Promise<void> {
		if (this.inFlight) return this.inFlight;
		const value = this.latest;
		if (value === null) return Promise.resolve();
		this.latest = null;
		if (this.baseline !== null && this.opts.equal(value, this.baseline)) {
			this.setState('saved', null);
			return Promise.resolve();
		}
		this.setState('saving', null);
		const job = (async () => {
			try {
				const stored = await this.opts.save(value);
				this.baseline = stored;
				this.opts.onSaved(value, stored);
				this.setState(this.latest !== null ? 'pending' : 'saved', null);
			} catch (e) {
				const msg = this.opts.describeError ? this.opts.describeError(e) : String(e);
				// Keep the edit so the next change (or a retry) sends it again.
				if (this.latest === null) this.latest = value;
				this.setState('error', msg);
				this.inFlight = null;
				return;
			}
			this.inFlight = null;
			if (this.latest !== null) {
				this.cancelTimer();
				await this.run();
			}
		})();
		this.inFlight = job;
		return job;
	}

	/** After an error: sends the kept edit again. */
	retry(): Promise<void> {
		return this.run();
	}

	private setState(state: SaveState, error: string | null): void {
		this.state = state;
		this.opts.onState(state, error);
	}
}
