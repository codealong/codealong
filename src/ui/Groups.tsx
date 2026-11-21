// A list's groups, wherever Group by groups one (Sessions, PRs, Todo, Schedules; groups.ts): each
// group's collapsible heading (a project's, a repo's, a worktree's, or the one over rows in
// none), its indented rows as its list draws them, and the way to the ones folded away past
// `GROUP_SHOWN`. In Sessions a worktree's heading is a selectable row; in PRs, Todo and Schedules
// a press on one shows the worktree's panel in place of the one beside the list. All non-empty
// groups reorder directly by dragging (`useGroupDrag`).

import type { ReactNode } from 'react';
import { groupLabel, type ListGroup } from '../groups';
import type { SelectHow } from '../picks';
import type { DesktopPinned, Lane } from '../types';
import { GroupFoldRow, NoneHead, ProjectHead, RepoHead, WorktreeHead } from './LaneRow';
import { ProjectSaidHere } from './ProjectSaid';
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
  /** Opens a folded group past `GROUP_SHOWN`, or folds it again. */
  onToggle: (g: AnyGroup) => void;
  /** Whether a group's children are collapsed under its heading. */
  collapsed?: (g: AnyGroup) => boolean;
  /** Collapses or expands a group's children; omitted while searching. */
  onCollapse?: (g: AnyGroup) => void;
  /** Reorders a dragged group heading to insertion point `index` among the groups showing. */
  onPlace?: (key: string, index: number) => void;
  /** What a worktree heading pins to the desktop when dragged out of the window. */
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
  drag?: DragProps;
  /** What the heading over rows in no project, repo or worktree says. */
  none: string;
}

/** A group's heading: its worktree's, its repo's, its project's, or the one over rows in none. */
export function GroupHead({ g, acts, now, rows, domId, selected = false, drag, none }: HeadProps) {
  const collapsed = !!acts.collapsed?.(g);
  const onCollapse = acts.onCollapse ? () => acts.onCollapse!(g) : undefined;
  const count = g.all.length;
  if (g.lane) {
    return (
      <WorktreeHead
        lane={g.lane}
        now={now}
        count={count}
        collapsed={collapsed}
        onCollapse={onCollapse}
        role={rows ? 'option' : 'heading'}
        domId={domId}
        selected={selected}
        picked={!!acts.picked?.has(g.lane.id)}
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
  if (g.repo) return <RepoHead repo={g.repo} count={count} collapsed={collapsed} onCollapse={onCollapse} onMenu={() => acts.onMenu(g)} drag={drag} />;
  if (g.project) return <ProjectHead project={g.project} count={count} collapsed={collapsed} onCollapse={onCollapse} drag={drag} />;
  return <NoneHead label={none} count={count} collapsed={collapsed} onCollapse={onCollapse} />;
}

/** A group: its collapsible heading, its indented rows (with the heading's project already said),
 *  and the way to rows folded away past `GROUP_SHOWN`. */
export function GroupBlock({ children, ...head }: HeadProps & { children: ReactNode }) {
  const { g, acts } = head;
  const collapsed = !!acts.collapsed?.(g);
  const project = g.project ?? g.repo?.project ?? g.lane?.project ?? null;
  return (
    <div className="ca-group">
      <GroupHead {...head} />
      {!collapsed && (
        <div className="ca-group-body">
          <ProjectSaidHere project={project}>{children}</ProjectSaidHere>
          {(g.folded > 0 || g.opened) && <GroupFoldRow folded={g.folded} opened={g.opened} onToggle={() => acts.onToggle(g)} />}
        </div>
      )}
    </div>
  );
}

/** The drag of a list's group headings: reorders the unified group list, and in Sessions lets a
 *  worktree heading drag out to the desktop. */
export function useGroupDrag(groups: AnyGroup[], acts: GroupActs | undefined) {
  const groupOf = (key: string) => groups.find((g) => g.key === key);
  return useRowDrag({
    onPlace: acts?.onPlace,
    joins: false,
    desktop: (key) => {
      const g = groupOf(key);
      return g && acts?.desktop ? acts.desktop(g) : null;
    },
    label: (key) => {
      const g = groupOf(key);
      return g ? groupLabel(g) : '';
    },
  });
}
