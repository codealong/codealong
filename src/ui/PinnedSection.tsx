// The Pinned section at the top of the Worktrees and Sessions views: rows in your order. Drag
// one, or press ⌥↑ ⌥↓, to move it.

import { useRef, useState, type PointerEvent as ReactPointerEvent, type ReactNode } from 'react';
import { Kbd, SectionHeader } from './primitives';

/** What a pinned row needs so it can be dragged. */
export interface PinDrag {
  /** Position among the pinned rows. */
  pinIndex: number;
  onPointerDown: (e: ReactPointerEvent<HTMLDivElement>) => void;
  /** Marks the row being dragged and where it would land. */
  extraClass?: string;
}

interface PinnedSectionProps<T> {
  items: T[];
  /** The pinned key of an item: a session key, or a worktree's lane id. */
  keyOf: (item: T) => string;
  /** Moves a pin to insertion point `index` among the rows showing. */
  onMove: (key: string, index: number) => void;
  row: (item: T, drag: PinDrag) => ReactNode;
}

export function PinnedSection<T>({ items, keyOf, onMove, row }: PinnedSectionProps<T>) {
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

  // A pointer drag, not HTML drag and drop: it behaves the same in every webview and leaves
  // clicks alone until the pointer has moved a few pixels.
  const startDrag = (e: ReactPointerEvent<HTMLDivElement>, key: string) => {
    if (e.button !== 0 || (e.target as HTMLElement).closest('button')) return;
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

  return (
    <section className="ca-bayblock ca-pinned">
      <SectionHeader
        icon="pinned"
        label="Pinned"
        count={items.length}
        hint={
          <>
            Drag or <Kbd keys={['⌥', '↑']} /> <Kbd keys={['⌥', '↓']} /> to reorder
          </>
        }
      />
      <div ref={listRef} className={['ca-bay-lanes', drag && 'is-dragging'].filter(Boolean).join(' ')}>
        {items.map((item, index) => {
          const key = keyOf(item);
          const extraClass = [
            drag?.key === key && 'is-drag-source',
            drag?.index === index && 'is-drop-before',
            drag != null && index === items.length - 1 && drag.index === items.length && 'is-drop-after',
          ]
            .filter(Boolean)
            .join(' ');
          return row(item, { pinIndex: index, onPointerDown: (e) => startDrag(e, key), extraClass: extraClass || undefined });
        })}
      </div>
    </section>
  );
}
