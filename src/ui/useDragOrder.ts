// Dragging rows into a new order: pinned worktrees and sessions, and todos. A pointer drag, not
// HTML drag and drop: it behaves the same in every webview and leaves clicks alone until the
// pointer has moved a few pixels.

import { useRef, useState, type PointerEvent as ReactPointerEvent } from 'react';

/** What a draggable row needs. */
export interface DragProps {
  /** Position among the rows that can be dragged. */
  pinIndex: number;
  onPointerDown: (e: ReactPointerEvent<HTMLDivElement>) => void;
  /** Marks the row being dragged and where it would land. */
  extraClass?: string;
}

/**
 * The rows in `listRef` with a `data-pin-index` can be dragged. `onMove` hears the key of the
 * row dropped and its insertion point among them.
 */
export function useDragOrder(onMove: (key: string, index: number) => void) {
  const listRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<{ key: string; startY: number; active: boolean; index: number } | null>(null);
  const [drag, setDrag] = useState<{ key: string; index: number } | null>(null);

  /** Where a drop at height `y` would land: before the first row whose middle is below it. */
  const insertionIndex = (y: number) => {
    const rows = listRef.current?.querySelectorAll<HTMLElement>('[data-pin-index]') ?? [];
    for (let i = 0; i < rows.length; i++) {
      const r = rows[i].getBoundingClientRect();
      if (y < r.top + r.height / 2) return i;
    }
    return rows.length;
  };

  const startDrag = (e: ReactPointerEvent<HTMLDivElement>, key: string) => {
    // ⌘- and ⇧-clicks pick rows; they never start a drag.
    if (e.button !== 0 || e.metaKey || e.shiftKey || e.ctrlKey || (e.target as HTMLElement).closest('button')) return;
    dragRef.current = { key, startY: e.clientY, active: false, index: -1 };
    const move = (ev: PointerEvent) => {
      const d = dragRef.current;
      if (!d || (!d.active && Math.abs(ev.clientY - d.startY) < 4)) return;
      d.active = true;
      d.index = insertionIndex(ev.clientY);
      setDrag({ key: d.key, index: d.index });
    };
    const end = () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', end);
      window.removeEventListener('pointercancel', end);
      const d = dragRef.current;
      dragRef.current = null;
      setDrag(null);
      if (d?.active && d.index >= 0) onMove(d.key, d.index);
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', end);
    window.addEventListener('pointercancel', end);
  };

  /** The drag props for the row with `key` at `index` of `count`. */
  const rowProps = (key: string, index: number, count: number): DragProps => {
    const extraClass = [
      drag?.key === key && 'is-drag-source',
      drag?.index === index && 'is-drop-before',
      drag != null && index === count - 1 && drag.index === count && 'is-drop-after',
    ]
      .filter(Boolean)
      .join(' ');
    return { pinIndex: index, onPointerDown: (e) => startDrag(e, key), extraClass: extraClass || undefined };
  };

  return { listRef, dragging: drag != null, rowProps };
}
