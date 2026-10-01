// The Pinned section at the top of the Worktrees and PRs views: rows in your order. Drag one, or
// press ⌃⌘↑ ⌃⌘↓, to move it; drag any other row up here to pin it, and one of these down to the
// list to unpin it (useRowDrag.tsx). Sessions has no headings: its pinned rows lead its one list.

import type { ReactNode } from 'react';
import { Kbd, SectionHeader } from './primitives';

interface PinnedSectionProps<T> {
  items: T[];
  /** A pinned row, at its place among them. */
  row: (item: T, index: number) => ReactNode;
}

export function PinnedSection<T>({ items, row }: PinnedSectionProps<T>) {
  return (
    <section className="ca-bayblock ca-pinned">
      <SectionHeader
        icon="pinned"
        label="Pinned"
        count={items.length}
        hint={
          <>
            Drag or <Kbd keys={['⌃', '⌘', '↑']} /> <Kbd keys={['⌃', '⌘', '↓']} /> to reorder
          </>
        }
      />
      <div className="ca-bay-lanes">{items.map((item, index) => row(item, index))}</div>
    </section>
  );
}
