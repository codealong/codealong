// A list's groups, wherever Group by groups one (Sessions, PRs, Todo; groups.ts): each group's
// heading (a worktree's, a repo's, or the one over rows in neither), its rows as its list draws
// them, the way to the ones folded away, and the pinned groups' section. In Sessions a worktree's
// heading is a row, selected for its panel and picked with others, and the pinned groups are
// dragged among themselves (`useGroupDrag`); in PRs and Todo a press on one shows the worktree's
// panel in place of the one beside the list. A repo's has its menu everywhere.

import type { ReactNode } from 'react';
import type { GroupBy, ListGroup } from '../groups';
import type { SelectHow } from '../picks';
import type { DesktopPinned, Lane } from '../types';
import { GroupFoldRow, NoneHead, RepoHead, WorktreeHead } from './LaneRow';
import { ReorderHint, SectionHeader } from './primitives';
import { useRowDrag, type DragProps } from './useRowDrag';

/** A group of any list's rows, as its heading needs it. */
export type AnyGroup = ListGroup<unknown>;

/** What a group's heading does. */
export interface GroupActs {
  /** A worktree's heading pressed, alone or with ⌘ or ⇧ held. */
  onSelect: (g: AnyGroup, how: SelectHow) => void;
  /** A double-click on a worktree's heading: opens its most recent session. */
  onOpen: (g: AnyGroup) => void;
  /** Opens one of a worktree's places: its app, from its chip. */
  onOpenTarget: (lane: Lane, targetId: string) => void;
  onMenu: (g: AnyGroup) => void;
  onPrMenu: (g: AnyGroup) => void;
  onPrShow: (g: AnyGroup) => void;
  onPrOpen: (g: AnyGroup) => void;
  /** Opens a folded group, or folds it again. */
  onToggle: (g: AnyGroup) => void;
  /** Where the headings are dragged (Sessions): a group put at insertion point `index` among the
   *  pinned ones showing, pinned there if it isn't; one dragged down to the list or out of the
   *  window unpinned; and what one pins to the desktop, if it can be. */
  onPlace?: (key: string, index: number) => void;
  onUnpin?: (key: string) => void;
  desktop?: (g: AnyGroup) => DesktopPinned | null;
  /** The worktrees picked together, by id (Sessions). */
  picked?: ReadonlySet<string>;
}

interface HeadProps {
  g: AnyGroup;
  acts: GroupActs;
  now: number;
  /** A worktree's heading is a row of the list (Sessions), with its element id; else a heading. */
  rows: boolean;
  domId?: string;
  selected?: boolean;
  /** One of the pinned groups. */
  pinned: boolean;
  drag?: DragProps;
  /** What the heading over rows in no worktree or repo says. */
  none: string;
}

/** A group's heading: its worktree's, its repo's, or the one over rows in neither. */
export function GroupHead({ g, acts, now, rows, domId, selected = false, pinned, drag, none }: HeadProps) {
  if (g.lane) {
    return (
      <WorktreeHead
        lane={g.lane}
        now={now}
        role={rows ? 'option' : 'heading'}
        domId={domId}
        selected={selected}
        picked={!!acts.picked?.has(g.lane.id)}
        pinned={pinned}
        onSelect={(how) => acts.onSelect(g, how)}
        onOpen={() => acts.onOpen(g)}
        onOpenTarget={acts.onOpenTarget}
        onMenu={() => acts.onMenu(g)}
        onPrMenu={() => acts.onPrMenu(g)}
        onPrShow={() => acts.onPrShow(g)}
        onPrOpen={() => acts.onPrOpen(g)}
        drag={drag}
      />
    );
  }
  if (g.repo) return <RepoHead repo={g.repo} pinned={pinned} onMenu={() => acts.onMenu(g)} drag={drag} />;
  return <NoneHead label={none} />;
}

/** A group: its heading, its rows as its list draws them, and the way to the ones folded away. */
export function GroupBlock({ children, ...head }: HeadProps & { children: ReactNode }) {
  const { g, acts } = head;
  return (
    <div className="ca-group">
      <GroupHead {...head} />
      {children}
      {(g.folded > 0 || g.opened) && <GroupFoldRow folded={g.folded} opened={g.opened} onToggle={() => acts.onToggle(g)} />}
    </div>
  );
}

/** The pinned groups, after a list's pinned rows: reordered by dragging where `reorder`. */
export function PinnedGroups({ by, count, reorder, children }: { by: GroupBy; count: number; reorder: boolean; children: ReactNode }) {
  return (
    <section className="ca-bayblock ca-pinned">
      <SectionHeader icon="pinned" label={by === 'worktree' ? 'Pinned worktrees' : 'Pinned repos'} count={count} hint={reorder ? <ReorderHint /> : undefined} />
      <div className="ca-bay-lanes">{children}</div>
    </section>
  );
}

/** What a dragged heading says it holds. */
function groupName(g: AnyGroup): string {
  return g.lane ? g.lane.callsign : (g.repo?.name ?? '');
}

/** The drag of a list's group headings: among the pinned groups, or out of the window. Its rows
 *  are the headings alone, apart from the list's own drag (useRowDrag.tsx `dragOf`). */
export function useGroupDrag(groups: AnyGroup[], acts: GroupActs | undefined) {
  const groupOf = (key: string) => groups.find((g) => g.key === key);
  return useRowDrag({
    onPlace: acts?.onPlace,
    joins: true,
    onLeave: acts?.onUnpin,
    desktop: (key) => {
      const g = groupOf(key);
      return g && acts?.desktop ? acts.desktop(g) : null;
    },
    label: (key) => {
      const g = groupOf(key);
      return g ? groupName(g) : '';
    },
  });
}
