// In-page latency probe for scripts/perf.mjs, installed with
// page.addInitScript before any page script runs (the overlay CSP does not
// apply to init scripts). It must stay self-contained: Playwright sends the
// function's source text to the page, so it may not use anything from this
// module's scope.
//
// What it measures, per callout message the demo server sends with --perf:
//   sent    the server's Unix time in ms, from the {"t":"perf"} stamp sent
//           right before the callout message (same id and count)
//   recv    when the page's WebSocket delivered the callout message
//   dom     when a MutationObserver saw the callout in the DOM: a new
//           .ks-callout element, or an aria-label change on one, whose label
//           starts with the callout's text form (the renderer's calloutText)
//   raf1    the first requestAnimationFrame callback after `dom`: the start
//           of the rendering update that includes the callout
//   raf2    the next requestAnimationFrame callback: by then the frame that
//           includes the callout has been produced. A paint approximation,
//           not the moment pixels reach a display.
// Page times are performance.timeOrigin + performance.now(), the same Unix
// clock as `sent` on one machine.
//
// Privacy: the probe keeps labels in memory to match DOM nodes; the harness
// reports only counts and timings.

export function installLatencyProbe() {
	const w = window;
	if (w.__ksProbe) return;
	const clock = () => performance.timeOrigin + performance.now();
	const probe = {
		stamps: 0,
		callouts: 0,
		unpaired: 0,
		records: [],
		pending: [],
		longTasks: { count: 0, totalMs: 0, maxMs: 0 },
		installedAt: clock(),
		dateSkewMs: Date.now() - clock(),
	};
	w.__ksProbe = probe;

	const textForm = (c) => {
		const steps = c.steps.map((s) => s.map((chip) => chip.label).join(' + ')).filter((s) => s);
		let out;
		if (c.kind === 'text' || c.kind === 'masked') {
			const base = c.kind === 'text' ? (c.text == null ? '' : c.text) : 'Typing';
			out = [base]
				.concat(steps)
				.filter((s) => s.length > 0)
				.join(' ');
		} else {
			out = steps.join(', ');
		}
		if (c.count > 1) out += ' x' + c.count;
		return out;
	};
	const labelMatches = (aria, label) => aria === label || aria.indexOf(label + '. ') === 0;

	const knownIds = new Set();
	let stamp = null;
	const onMessage = (ev) => {
		const recv = clock();
		let msg;
		try {
			msg = JSON.parse(typeof ev.data === 'string' ? ev.data : '');
		} catch (e) {
			return;
		}
		if (!msg || typeof msg !== 'object') return;
		if (msg.t === 'perf') {
			probe.stamps++;
			stamp = { seq: msg.seq, sent: msg.sent_ms, id: msg.id, count: msg.count };
			return;
		}
		if (msg.t !== 'callout' || !msg.callout) return;
		const c = msg.callout;
		probe.callouts++;
		const paired = stamp && stamp.id === c.id && stamp.count === c.count ? stamp : null;
		if (!paired) probe.unpaired++;
		stamp = null;
		probe.pending.push({
			seq: paired ? paired.seq : null,
			sent: paired ? paired.sent : null,
			recv: recv,
			dom: null,
			raf1: null,
			raf2: null,
			update: knownIds.has(c.id),
			label: textForm(c),
		});
		knownIds.add(c.id);
	};

	const Native = w.WebSocket;
	if (typeof Native === 'function') {
		const Probed = function (url, protocols) {
			const socket = protocols === undefined ? new Native(url) : new Native(url, protocols);
			socket.addEventListener('message', onMessage);
			return socket;
		};
		Probed.prototype = Native.prototype;
		for (const k of ['CONNECTING', 'OPEN', 'CLOSING', 'CLOSED']) Probed[k] = Native[k];
		w.WebSocket = Probed;
	}

	const frame = (entries) => {
		w.requestAnimationFrame(() => {
			const t1 = clock();
			for (const r of entries) r.raf1 = t1;
			w.requestAnimationFrame(() => {
				const t2 = clock();
				for (const r of entries) r.raf2 = t2;
			});
		});
	};

	const isCallout = (n) => n.nodeType === 1 && n.matches && n.matches('.ks-callout');
	const observer = new w.MutationObserver((list) => {
		const t = clock();
		const added = [];
		const changed = [];
		for (const r of list) {
			if (r.type === 'childList') {
				r.addedNodes.forEach((n) => {
					if (n.nodeType !== 1) return;
					if (isCallout(n)) added.push(n);
					n.querySelectorAll('.ks-callout').forEach((x) => added.push(x));
				});
			} else if (r.type === 'attributes' && isCallout(r.target)) {
				changed.push(r.target);
			}
		}
		if (added.length === 0 && changed.length === 0) return;
		const addedLabels = added.map((n) => n.getAttribute('aria-label') || '');
		const changedLabels = changed.map((n) => n.getAttribute('aria-label') || '');
		const matched = [];
		probe.pending = probe.pending.filter((p) => {
			let i = addedLabels.findIndex((a) => labelMatches(a, p.label));
			if (i >= 0) {
				addedLabels.splice(i, 1);
			} else if (p.update) {
				i = changedLabels.findIndex((a) => labelMatches(a, p.label));
				if (i >= 0) changedLabels.splice(i, 1);
			}
			if (i < 0) return true;
			p.dom = t;
			matched.push(p);
			return false;
		});
		for (const p of matched) {
			delete p.label;
			probe.records.push(p);
		}
		if (matched.length) frame(matched);
	});
	observer.observe(document, {
		subtree: true,
		childList: true,
		attributes: true,
		attributeFilter: ['aria-label'],
	});

	try {
		const po = new w.PerformanceObserver((list) => {
			for (const e of list.getEntries()) {
				probe.longTasks.count++;
				probe.longTasks.totalMs += e.duration;
				probe.longTasks.maxMs = Math.max(probe.longTasks.maxMs, e.duration);
			}
		});
		po.observe({ type: 'longtask', buffered: true });
	} catch (e) {
		probe.longTasks = null;
	}
}

/** Latencies in ms from a finished probe, per record (null where unknown). */
export function probeLatencies(records) {
	const d = (a, b) => (typeof a === 'number' && typeof b === 'number' ? b - a : null);
	return records.map((r) => ({
		seq: r.seq,
		transport: d(r.sent, r.recv),
		dom: d(r.sent, r.dom),
		frame: d(r.sent, r.raf1),
		nextFrame: d(r.sent, r.raf2),
		pageToFrame: d(r.recv, r.raf1),
	}));
}
