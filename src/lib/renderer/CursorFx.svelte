<!--
	Cursor effects: one full-page canvas drawn at the device pixel ratio
	(crisp at 4K and at any OBS scale) with the pointer highlight (ring,
	spotlight or dot), click ripples whose shape says which button was
	pressed, a fading drag trail and a wheel indicator.

	Positions are normalized to the chosen monitor, so on a page the size of
	that monitor (Host A, or an OBS source laid over a Display Capture of it)
	the effects sit exactly on the captured cursor.

	It draws only when something changes, and runs animation frames only while
	a ripple, a wheel indicator, a trail or a press is still moving: an idle
	cursor costs no frames at all. Logic and drawing:
	src/lib/renderer/keyboard/cursor-fx.ts.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import type { CursorFxProps } from './visual-props';
	import {
		Trail,
		anyButtonDown,
		drawScene,
		fxStyle,
		monitorRect,
		type Ctx,
	} from './keyboard/cursor-fx';

	let { cursor, mouse, aspect, scale }: CursorFxProps = $props();

	let canvas = $state<HTMLCanvasElement | null>(null);
	/** Canvas size in CSS px and in device px. */
	let cssW = 0;
	let cssH = 0;
	let devW = 0;
	let devH = 0;
	let frame = 0;
	let mounted = false;
	const trail = new Trail();

	function clock(): number {
		return typeof performance !== 'undefined' ? performance.now() : Date.now();
	}

	function invalidate(): void {
		if (!mounted || frame !== 0 || typeof requestAnimationFrame === 'undefined') return;
		frame = requestAnimationFrame(render);
	}

	function measure(c: HTMLCanvasElement): void {
		const r = c.getBoundingClientRect();
		const dpr =
			typeof window !== 'undefined' && window.devicePixelRatio > 0 ? window.devicePixelRatio : 1;
		if (cssW !== r.width || cssH !== r.height || devW === 0) {
			cssW = r.width;
			cssH = r.height;
			devW = Math.max(1, Math.round(r.width * dpr));
			devH = Math.max(1, Math.round(r.height * dpr));
		}
	}

	function render(): void {
		frame = 0;
		const c = canvas;
		if (!c) return;
		if (cssW === 0 || cssH === 0) measure(c);
		if (c.width !== devW) c.width = devW;
		if (c.height !== devH) c.height = devH;
		const ctx = c.getContext('2d');
		if (!ctx || cssW <= 0 || cssH <= 0) return;
		const now = clock();
		trail.prune(now);
		ctx.setTransform(1, 0, 0, 1, 0, 0);
		ctx.clearRect(0, 0, c.width, c.height);
		ctx.setTransform(devW / cssW, 0, 0, devH / cssH, 0, 0);
		const moving = drawScene(
			ctx as unknown as Ctx,
			{
				cursor,
				style: fxStyle(cursor, scale),
				width: cssW,
				height: cssH,
				rect: monitorRect(cssW, cssH, aspect),
				pointer: mouse.pointer,
				buttons: mouse.buttons,
				clicks: mouse.clicks,
				wheels: mouse.wheels,
				trail: trail.points,
			},
			now
		);
		if (moving) invalidate();
	}

	// Any change of input or settings draws one frame (more only while moving).
	$effect(() => {
		void cursor.style;
		void cursor.size;
		void cursor.color;
		void cursor.opacity;
		void cursor.ripples;
		void cursor.wheel;
		void aspect;
		void scale;
		void mouse.clicks;
		void mouse.wheels;
		void mouse.buttons;
		invalidate();
	});

	// The drag trail: positions while a button is held.
	$effect(() => {
		const p = mouse.pointer;
		const held = anyButtonDown(mouse.buttons);
		if (cursor.trail && p.known) {
			trail.record(p.x, p.y, p.movedAt > 0 ? p.movedAt : clock(), held);
		} else {
			trail.record(p.x, p.y, clock(), false);
			if (!cursor.trail) trail.clear();
		}
		invalidate();
	});

	onMount(() => {
		mounted = true;
		const c = canvas;
		let ro: ResizeObserver | null = null;
		if (c && typeof ResizeObserver !== 'undefined') {
			ro = new ResizeObserver((entries) => {
				for (const e of entries) {
					cssW = e.contentRect.width;
					cssH = e.contentRect.height;
					const dp = e.devicePixelContentBoxSize?.[0];
					const dpr = window.devicePixelRatio > 0 ? window.devicePixelRatio : 1;
					devW = Math.max(1, dp ? dp.inlineSize : Math.round(cssW * dpr));
					devH = Math.max(1, dp ? dp.blockSize : Math.round(cssH * dpr));
				}
				invalidate();
			});
			try {
				ro.observe(c, { box: 'device-pixel-content-box' });
			} catch {
				ro.observe(c);
			}
		} else if (c) {
			measure(c);
		}
		invalidate();
		return () => {
			mounted = false;
			ro?.disconnect();
			if (frame !== 0) cancelAnimationFrame(frame);
			frame = 0;
		};
	});
</script>

<canvas class="ks-cursor-fx" bind:this={canvas} aria-hidden="true"></canvas>

<style>
	.ks-cursor-fx {
		position: absolute;
		top: 0;
		left: 0;
		display: block;
		width: 100%;
		height: 100%;
		pointer-events: none;
	}
</style>
