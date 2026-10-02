// The handles on the sidebar's and the right panel's inner edges. Drag one to resize its panel;
// double-click it to put the panel back to its default width. Each panel's width is kept in
// localStorage, like the view and the age filter.

import { useState, type PointerEvent as ReactPointerEvent } from 'react';
import { useStored, type Codec } from '../stored';
import { cx } from './primitives';

/** A width as a number of pixels; none, or one that doesn't read, is the panel's default. */
const WIDTH_STORED: Codec<number | null> = {
  parse: (raw) => {
    const width = Number(raw);
    return Number.isFinite(width) && width > 0 ? width : undefined;
  },
  format: (width) => (width == null ? null : String(width)),
};

/** A panel's width as you last left it, and a setter that saves it. Null means the default. */
export function usePanelWidth(key: string): [number | null, (width: number | null) => void] {
  return useStored(key, WIDTH_STORED, null);
}

/**
 * The width a drag of `delta` pixels asks for (positive is wider): no narrower than `min`, and
 * no wider than the board allows. `spare` is how far the board is over its own minimum.
 */
export function dragWidth(start: number, delta: number, min: number, spare: number): number {
  return Math.round(Math.max(min, Math.min(start + delta, start + Math.max(0, spare))));
}

const minWidth = (el: Element) => parseFloat(getComputedStyle(el).minWidth) || 0;

interface ResizerProps {
  /** The panel this handle resizes: the one before it (the sidebar) or after it (the right panel). */
  panel: 'before' | 'after';
  /** The custom property on the window body that sizes the panel's column. */
  cssVar: string;
  onResize: (width: number | null) => void;
}

/**
 * Sits in a zero-width column of the window body's grid, between a panel and the board. The
 * limits come from the CSS: the panel's min-width, and the board's, which a drag leaves it.
 */
export function Resizer({ panel, cssVar, onResize }: ResizerProps) {
  const [active, setActive] = useState(false);

  // A pointer drag, as in PinnedSection. It starts after a few pixels, so a double-click stays a
  // double-click. While it runs the width goes straight onto the grid's style, and the app
  // hears it once, at the end, instead of re-rendering the board on every move. It also ends
  // if the window loses focus, so a button let go in another app can't leave it running.
  const startDrag = (e: ReactPointerEvent<HTMLDivElement>) => {
    const handle = e.currentTarget;
    const grid = handle.parentElement;
    const target = panel === 'before' ? handle.previousElementSibling : handle.nextElementSibling;
    const board = panel === 'before' ? handle.nextElementSibling : handle.previousElementSibling;
    if (e.button !== 0 || !grid || !target || !board) return;
    const start = target.getBoundingClientRect().width;
    const min = minWidth(target);
    const spare = board.getBoundingClientRect().width - minWidth(board);
    const startX = e.clientX;
    let width: number | null = null;
    const move = (ev: PointerEvent) => {
      const delta = (ev.clientX - startX) * (panel === 'before' ? 1 : -1);
      if (width == null) {
        if (Math.abs(delta) < 3) return;
        // Until the drag ends nothing else reacts to the pointer, and the cursor stays the same.
        document.body.classList.add('is-resizing');
        setActive(true);
      }
      width = dragWidth(start, delta, min, spare);
      grid.style.setProperty(cssVar, `${width}px`);
    };
    const end = () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', end);
      window.removeEventListener('pointercancel', end);
      window.removeEventListener('blur', end);
      if (width == null) return;
      document.body.classList.remove('is-resizing');
      setActive(false);
      onResize(width);
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', end);
    window.addEventListener('pointercancel', end);
    window.addEventListener('blur', end);
  };

  return (
    <div
      className={cx('ca-resizer', active && 'is-active')}
      role="separator"
      aria-orientation="vertical"
      title="Drag to resize, double-click to reset"
      onPointerDown={startDrag}
      // Leaves focus where it was and starts no text selection.
      onMouseDown={(e) => e.preventDefault()}
      onDoubleClick={() => onResize(null)}
    />
  );
}
