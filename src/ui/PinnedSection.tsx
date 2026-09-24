// The Pinned section at the top of the Worktrees and Sessions views: rows in your order. Drag
// one, or press ⌥↑ ⌥↓, to move it.

import type { ReactNode } from 'react';
import { Kbd, SectionHeader } from './primitives';
import { useDragOrder, type DragProps } from './useDragOrder';

/** What a pinned row needs so it can be dragged. */
export type PinDrag = DragProps;

interface PinnedSectionProps<T> {
  items: T[];
  /** The pinned key of an item: a session key, or a worktree's lane id. */
  keyOf: (item: T) => string;
  /** Moves a pin to insertion point `index` among the rows showing. */
  onMove: (key: string, index: number) => void;
  row: (item: T, drag: PinDrag) => ReactNode;
}

export function PinnedSection<T>({ items, keyOf, onMove, row }: PinnedSectionProps<T>) {
  const { listRef, dragging, rowProps } = useDragOrder(onMove);
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
      <div ref={listRef} className={['ca-bay-lanes', dragging && 'is-dragging'].filter(Boolean).join(' ')}>
        {items.map((item, index) => row(item, rowProps(keyOf(item), index, items.length)))}
      </div>
    </section>
  );
}
