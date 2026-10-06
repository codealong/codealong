// Zooming the board's windows as VS Code and a browser do: ⌘= (or ⌘+) bigger, ⌘- smaller, ⌘0
// back to actual size. The engine keeps the zoom in config.toml and sets it on every board window
// (src-tauri/src/zoom.rs); this says which keys ask for it and what the page is zoomed to now.
//
// It's the webview's own page zoom, so the page and its events agree on CSS pixels, and
// devicePixelRatio has it in it; only a spot handed to the window in points needs it (api.ts
// `popupMenu`, a menu dropped from a button).

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

let factor = 1;

/** The board's zoom, in percent, as this window draws it: the board windows set it as boards come. */
export function setPageZoom(percent: number | undefined): void {
  factor = percent && percent > 0 ? percent / 100 : 1;
}

/** Points on the window per CSS pixel on the page; 1 in a window that isn't zoomed. */
export function pageZoom(): number {
  return factor;
}
