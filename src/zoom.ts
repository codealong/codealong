// Zooming as VS Code and a browser do: ⌘= (or ⌘+) bigger, ⌘- smaller, ⌘0 back to actual size.
// The engine keeps the zoom in config.toml (src-tauri/src/zoom.rs); this says which keys ask for
// it, and how a window draws at it.
//
// The board's windows zoom their whole page: the engine sets the webview's page zoom on them, so
// the page and its events agree on CSS pixels and devicePixelRatio has it in it; only a spot
// handed to the window in points needs it (api.ts `popupMenu`). The windows that float can't zoom
// their page, which is their window's size and shape (the notch's is the camera's), so a card in
// them, a session's or a PR's, zooms itself with CSS `zoom` (`cardZoom`).
//
// Under CSS zoom WebKit gives rects and the pointer on the screen's scale, and offsets, scroll
// positions and the lengths written in CSS on the element's own; a box sized by its parent (a
// percent, flex) fills it, with its contents laid out bigger. Checked in a WKWebView on
// 2026-10-06. So code inside a card that mixes the two converts with `scaleOf`.

import type { CSSProperties } from 'react';

/** Which way a key or View's menu zooms, as the engine's `zoom` command takes it. */
export type ZoomStep = 'in' | 'out' | 'actual';

/** The step a key asks for, or none. ⇧ is allowed for ⌘+ and ⌘_, which some keyboards need for
 *  the same key; by the character, so a keyboard whose + has a key of its own works too. */
export function zoomStep(e: Pick<KeyboardEvent, 'key' | 'metaKey' | 'ctrlKey' | 'altKey' | 'shiftKey'>): ZoomStep | null {
  if (!e.metaKey || e.ctrlKey || e.altKey) return null;
  if (e.key === '=' || e.key === '+') return 'in';
  if (e.key === '-' || e.key === '_') return 'out';
  if (e.key === '0' && !e.shiftKey) return 'actual';
  return null;
}

/** A zoom in percent as a factor; 1 for none. */
export function zoomFactor(percent: number | undefined): number {
  return percent && percent > 0 ? percent / 100 : 1;
}

/** The style that draws a card at the zoom, for the windows that float; none at actual size. The
 *  zoom is `--card-zoom` too, for a length that has to stay the same on the screen (a title bar's
 *  room for the window's buttons, SessionPanel.css). */
export function cardZoom(percent: number | undefined): CSSProperties | undefined {
  const z = zoomFactor(percent);
  return z === 1 ? undefined : ({ zoom: z, '--card-zoom': z } as CSSProperties);
}

/** How much bigger than its own CSS pixels an element draws: its card's zoom, 1 elsewhere. A
 *  distance between two rects divided by it is one in the element's CSS pixels. */
export function scaleOf(el: HTMLElement): number {
  const w = el.offsetWidth;
  return w > 0 ? el.getBoundingClientRect().width / w : 1;
}

let factor = 1;

/** The board's zoom, in percent, as this window draws it: the board windows set it as boards come. */
export function setPageZoom(percent: number | undefined): void {
  factor = zoomFactor(percent);
}

/** Points on the window per CSS pixel on the page; 1 in a window that isn't zoomed. */
export function pageZoom(): number {
  return factor;
}
