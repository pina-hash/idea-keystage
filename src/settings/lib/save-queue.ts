/**
 * Saves the settings draft a moment after the last change, one save at a
 * time, and tells the owner what the app stored.
 *
 * * `push(value)` after every edit; nothing is sent while the value equals
 *   the last value the app confirmed (`baseline`).
 * * Edits during a save are kept and sent when it finishes.
 * * `onSaved(edit, stored, sent)`: the app may correct a value (clamps, a
 *   hotkey it could not use); the queue has already made `stored` its
 *   baseline, so the owner can adopt it without another save.
 *
 * The app also changes the stored value itself (the tray, a hotkey). With
 * the optional `fetch` and `merge`, the queue never sends a stale copy over
 * such a change:
 *
 * * A save reads what the app holds now and sends it with only the values
 *   the draft changed from the baseline (`sent = merge(baseline, edit,
 *   now)`), so a value changed elsewhere is never put back.
 * * `refresh()` reads what the app holds and, when it changed, makes it the
 *   baseline, carries a waiting edit over to it, and tells the owner
 *   (`onChanged(before, now)`) so the draft can do the same.
 *
 * Saves and refreshes run one at a time, in order, so a read never lands
 * on top of a newer save.
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
	/**
	 * After a save: `edit` is the draft that was pushed, `sent` what went to
	 * the app (the edit carried over to the app's current value), `stored`
	 * the app's answer, already the new baseline.
	 */
	onSaved: (edit: T, stored: T, sent: T) => void;
	onState: (state: SaveState, error: string | null) => void;
	describeError?: (e: unknown) => string;
	timers?: SaveTimers;
	/** Reads the value the app holds now. Needs `merge`. */
	fetch?: () => Promise<T>;
	/** Three-way merge: `theirs` with every value `mine` changed from `base`. */
	merge?: (base: T, mine: T, theirs: T) => T;
	/** `refresh()` found the app's value changed (`before` was the baseline). */
	onChanged?: (before: T, now: T) => void;
}

export class SaveQueue<T> {
	private readonly opts: SaveQueueOptions<T>;
	private readonly timers: SaveTimers;
	private baseline: T | null = null;
	/** Bumped by setBaseline, so a read that started before it is dropped. */
	private epoch = 0;
	private latest: T | null = null;
	private timer: unknown = null;
	private inFlight: Promise<void> | null = null;
	/** True only while a save is out (a refresh may also be in flight). */
	private saving = false;
	private wantSave = false;
	private wantRefresh = false;
	private state: SaveState = 'idle';

	constructor(opts: SaveQueueOptions<T>) {
		this.opts = opts;
		this.timers = opts.timers ?? systemSaveTimers;
	}

	/** The value the app confirmed (replaces the baseline; pending edits that equal it are dropped). */
	setBaseline(value: T): void {
		this.baseline = value;
		this.epoch += 1;
		if (this.latest !== null && this.opts.equal(this.latest, value)) {
			this.latest = null;
			this.cancelTimer();
			if (!this.saving && (this.state === 'pending' || this.state === 'error')) {
				this.setState('saved', null);
			}
		}
	}

	/** The value the app last confirmed. */
	get confirmed(): T | null {
		return this.baseline;
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
		this.setState(this.saving ? 'saving' : 'pending', null);
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

	/** After an error: sends the kept edit again. */
	retry(): Promise<void> {
		return this.run();
	}

	/**
	 * Reads what the app holds now (after any save in progress) and takes in
	 * a change made elsewhere. Several calls while one waits are one read.
	 */
	refresh(): Promise<void> {
		if (!this.opts.fetch || !this.opts.merge) return Promise.resolve();
		this.wantRefresh = true;
		return this.kick();
	}

	private cancelTimer(): void {
		if (this.timer !== null) this.timers.clearTimeout(this.timer);
		this.timer = null;
	}

	private run(): Promise<void> {
		this.wantSave = true;
		return this.kick();
	}

	/** Starts the worker unless it runs; it does every job asked for, one at a time. */
	private kick(): Promise<void> {
		if (this.inFlight) return this.inFlight;
		const job = (async () => {
			// Lets `inFlight` be set before the first job decides anything.
			await null;
			try {
				for (;;) {
					if (this.wantSave && this.latest !== null) {
						this.wantSave = false;
						if (!(await this.send())) break;
						if (this.latest !== null) {
							// An edit arrived during the save: send it now.
							this.cancelTimer();
							this.wantSave = true;
						}
						continue;
					}
					this.wantSave = false;
					if (this.wantRefresh) {
						this.wantRefresh = false;
						await this.pull();
						continue;
					}
					break;
				}
			} finally {
				this.wantSave = false;
				this.inFlight = null;
			}
		})();
		this.inFlight = job;
		return job;
	}

	/** One save. False when it failed (the edit is kept for a retry). */
	private async send(): Promise<boolean> {
		const edit = this.latest as T;
		this.latest = null;
		const base = this.baseline;
		if (base !== null && this.opts.equal(edit, base)) {
			this.setState('saved', null);
			return true;
		}
		this.setState('saving', null);
		this.saving = true;
		let sent = edit;
		let stored: T;
		try {
			if (base !== null && this.opts.fetch && this.opts.merge) {
				const now = await this.opts.fetch();
				sent = this.opts.merge(base, edit, now);
				// The app already holds exactly this (the same change came from elsewhere).
				stored = this.opts.equal(sent, now) ? now : await this.opts.save(sent);
			} else {
				stored = await this.opts.save(edit);
			}
		} catch (e) {
			this.saving = false;
			const msg = this.opts.describeError ? this.opts.describeError(e) : String(e);
			// Keep the edit so the next change (or a retry) sends it again.
			if (this.latest === null) this.latest = edit;
			this.setState('error', msg);
			return false;
		}
		this.saving = false;
		this.baseline = stored;
		if (this.latest !== null && this.opts.merge) {
			// An edit made during the save keeps only what it changed since `edit`.
			this.latest = this.opts.merge(edit, this.latest, stored);
		}
		this.opts.onSaved(edit, stored, sent);
		this.setState(this.latest !== null ? 'pending' : 'saved', null);
		return true;
	}

	/** One refresh. A failed read changes nothing; the next save reads again. */
	private async pull(): Promise<void> {
		const fetch = this.opts.fetch;
		const merge = this.opts.merge;
		const before = this.baseline;
		if (!fetch || !merge || before === null) return;
		const epoch = this.epoch;
		let now: T;
		try {
			now = await fetch();
		} catch {
			return;
		}
		// The owner replaced the settings meanwhile (import, reset): that is newer.
		if (epoch !== this.epoch || this.baseline !== before) return;
		if (this.opts.equal(before, now)) return;
		this.baseline = now;
		if (this.latest !== null) {
			this.latest = merge(before, this.latest, now);
			if (this.opts.equal(this.latest, now)) {
				// The app already holds the waiting edit (the same change came from elsewhere).
				this.latest = null;
				this.cancelTimer();
				if (this.state === 'pending' || this.state === 'error') this.setState('saved', null);
			}
		}
		this.opts.onChanged?.(before, now);
	}

	private setState(state: SaveState, error: string | null): void {
		this.state = state;
		this.opts.onState(state, error);
	}
}
