// What a group's heading does in the main window's lists (ui/Groups.tsx `GroupActs`), from what a
// render of the window has (App.tsx). In Sessions a worktree's heading is a row: selected for its
// panel and picked with others; in PRs, Todo and Schedules a press on one shows the worktree as
// its chip does. Every list's groups collapse/expand and reorder by dragging. A repo's has its
// menu everywhere.

import type { ListGroupBy, RepoName } from './groups';
import { placeGroupFor, type GroupPrefs } from './groupPrefs';
import type { LaneItem } from './lanes';
import type { SelectHow } from './picks';
import type { Lane, Pr } from './types';
import type { AnyGroup, GroupActs } from './ui/Groups';
import { headItem } from './ui/SessionsView';

/** What the window has for them, as one render has it. */
export interface GroupsHas {
  /** Shows a worktree where the view shows one: its panel in place of the one beside the list. */
  focusLane: (lane: Lane) => void;
  /** Sessions' picks: the sessions picked together, and the worktrees. */
  picks: {
    sessions: { clear: () => void };
    worktrees: { select: (row: LaneItem, how: SelectHow) => void; clear: () => void; keys: ReadonlySet<string>; menu: () => void };
  };
  /** A press on a list's row, before it's selected. */
  pressRow: (key: string, how: SelectHow) => void;
  setSelectedKey: (key: string) => void;
  open: (lane: Lane, targetId: string | null | undefined) => void;
  repoMenu: (key: string, repo: RepoName) => void;
  laneMenu: (lane: Lane) => void;
  lanePrMenu: (lane: Lane) => void;
  showPrOf: (p: Pick<Pr, 'url'>, row?: { key: string; select: () => void }) => void;
  openLanePr: (lane: Lane) => void;
}

/** Per-view group controls: folding past `GROUP_SHOWN`, collapsing a group, and dragging groups to
 *  reorder them. */
export interface GroupViewControls {
  toggle: (key: string) => void;
  collapsed?: (g: AnyGroup) => boolean;
  onCollapse?: (g: AnyGroup) => void;
  onPlace?: (key: string, index: number) => void;
}

/** Convenience wrapper wiring `GroupPrefs` collapse & reorder into `groupActsOf` for one view. */
export function viewGroupActs(
  has: GroupsHas,
  view: string,
  rows: boolean,
  by: ListGroupBy,
  grouped: { groups: readonly { key: string }[] } | null | undefined,
  prefs: GroupPrefs,
  toggle: (key: string) => void,
): GroupActs {
  return groupActsOf(has, rows, {
    toggle,
    collapsed: (g) => prefs.isCollapsed(view, g),
    onCollapse: prefs.toggleFor(view),
    onPlace: placeGroupFor(prefs, by, grouped),
  });
}

/** What a group's heading does: in Sessions (`rows`), or in the other lists. */
export function groupActsOf(has: GroupsHas, rows: boolean, controls: GroupViewControls): GroupActs {
  const { picks } = has;
  return {
    onSelect: (g, how) => {
      const lane = g.lane;
      if (!lane) return;
      if (!rows) {
        if (how === 'one') has.focusLane(lane);
        return;
      }
      if (how !== 'one') {
        picks.sessions.clear();
        picks.worktrees.select(headItem(lane), how);
        return;
      }
      picks.worktrees.clear();
      picks.sessions.clear();
      has.pressRow(lane.id, how);
      has.setSelectedKey(lane.id);
    },
    onOpen: (g) => {
      if (g.lane) has.open(g.lane, g.lane.defaultTarget);
    },
    onOpenTarget: has.open,
    // On a worktree picked with others, the menu is for all of them.
    onMenu: (g) => {
      if (g.repo) has.repoMenu(g.key, g.repo);
      else if (g.lane) (rows && picks.worktrees.keys.has(g.lane.id) ? picks.worktrees.menu() : has.laneMenu(g.lane));
    },
    onPrMenu: (g) => {
      if (g.lane) has.lanePrMenu(g.lane);
    },
    onPrShow: (g) => {
      const lane = g.lane;
      if (!lane?.pr) return;
      // In Sessions, its PR in its own panel's place, not in that of the worktrees picked with it.
      const select = () => {
        picks.worktrees.clear();
        has.setSelectedKey(lane.id);
      };
      has.showPrOf(lane.pr, rows ? { key: lane.id, select } : undefined);
    },
    onPrOpen: (g) => {
      if (g.lane) has.openLanePr(g.lane);
    },
    onToggle: (g) => controls.toggle(g.fold),
    collapsed: controls.collapsed,
    onCollapse: controls.onCollapse,
    onPlace: controls.onPlace,
    ...(rows && {
      // What's on another device has no card here; a repo or project has none.
      desktop: (g: { lane: Lane | null }) => (g.lane && !g.lane.device ? { ref: 'worktree' as const, key: g.lane.id } : null),
      picked: picks.worktrees.keys,
    }),
  };
}
