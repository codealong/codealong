// What the main window has selected, and what its panel shows for it (App.tsx): the row selected
// among the rows a view lists, the session whose conversation the panel shows, and which panel
// is on the right. Pure functions of what the window has, with tests (mainSelection.test.ts).

import type { ListRow } from './boardLists';
import type { RefKind } from './canvas/model';
import type { LaneItem } from './lanes';
import type { View } from './mainViews';
import type { TodoItem } from './todos';
import type { PrRowItem } from './ui/PrsView';
import type { SessionItem } from './ui/SessionsView';

/** What a selection falls back on when the view's list no longer has its row. */
export interface SelectionHas {
  /** Every todo and every PR the project has, listed now or not. */
  todoItems: readonly TodoItem[];
  prRows: readonly PrRowItem[];
  /** The session shown in Sessions as you moved it, while it's the one selected. */
  held: SessionItem | undefined;
  /** Every listed active session in Sessions, including those in folded sections: folding the
   *  section holding the selected session keeps it in the panel until another row is picked. */
  activeSessions?: readonly SessionItem[];
  /** In Sessions, the worktree selected by its id, whether the list has its heading or not. */
  chosenLane: LaneItem | undefined;
}

/**
 * The row selected, among `visible`, the rows the view lists: the one whose key is `selectedKey`,
 * and when the list doesn't have it, what the selection holds on. A todo that leaves the list
 * (marked done while Done is folded, or no longer matching the search or a filter) stays in the
 * panel, and so does a PR that leaves it (one you merge moves under Merged & closed, folded). A
 * session listed under another worktree now (its commands moved there) stays selected; when the
 * selected agent's line goes (you marked it seen, it stopped), the selection stays on its worktree
 * rather than jumping to the top; and a session you're moving stays in Sessions' panel as it was
 * until its copy takes its place. With nothing to hold on, the top session, else the top row.
 */
export function selectedRow(view: View, selectedKey: string | null, visible: readonly ListRow[], has: SelectionHas): ListRow | null {
  const [selectedLaneId, selectedSessionKey] = selectedKey?.split('::') ?? [];
  return (
    visible.find((r) => r.key === selectedKey) ??
    (view === 'todo' || (view === 'kanban' && selectedKey?.startsWith('todo:'))
      ? has.todoItems.find((r) => r.key === selectedKey)
      : view === 'prs'
        ? has.prRows.find((r) => r.key === selectedKey)
        : (visible.find((r) => r.kind === 'agent' && r.session.key === selectedSessionKey) ??
          has.activeSessions?.find((r) => r.key === selectedKey || r.session.key === selectedSessionKey) ??
          has.held ??
          has.chosenLane ??
          visible.find((r) => r.kind !== 'pr' && r.kind !== 'todo' && r.lane.id === selectedLaneId))) ??
    visible.find((r) => r.kind === 'agent') ??
    visible[0] ??
    null
  );
}

/** What says which session the panel shows. */
export interface ShownSessionOn {
  /** The session selected under a schedule, by key. */
  scheduleSession: string | null;
  /** A PR or a worktree shows in the panel's place (`Drill`). */
  drilled: boolean;
  selected: ListRow | null;
  /** The one card selected on the canvas. */
  canvasSel: { ref: RefKind; key: string } | null;
}

/** The session whose conversation the panel shows, by key: in Sessions, under a schedule, on a
 *  Kanban card, or as the canvas's selected card. One a PR's or todo's panel shows in the peek
 *  isn't it: the panel stays on the PR or todo. */
export function shownSessionOf(view: View, on: ShownSessionOn): string | null {
  return view === 'schedules'
    ? on.scheduleSession
    : view === 'sessions' || (view === 'kanban' && !on.drilled)
      ? on.selected?.kind === 'agent'
        ? on.selected.session.key
        : null
      : view === 'canvas' && on.canvasSel?.ref === 'session'
        ? on.canvasSel.key
        : null;
}

/** The panels the window shows on the right, each with a width of its own. */
export type PanelKind = 'pr' | 'todo' | 'session' | 'schedule' | 'lane';

/** What says which panel is on the right. */
export interface PanelOn {
  /** The Board view with no board to show: nothing can be selected, so it has no panel. */
  boardless: boolean;
  /** A session is selected under a schedule. */
  scheduleSession: boolean;
  /** What the canvas's selected card is; none when nothing is selected there. */
  canvasKind: 'lane' | 'session' | 'todo' | 'pr' | null;
  /** The kind of the row selected. */
  selectedKind: ListRow['kind'] | undefined;
}

/** Which panel is on the right, for its width: the view's own, or on the canvas the selected
 *  card's; none where there's nothing to show one for. */
export function panelKindOf(view: View, on: PanelOn): PanelKind | null {
  return on.boardless
    ? null
    : view === 'schedules'
      ? on.scheduleSession
        ? 'session'
        : 'schedule'
      : view === 'canvas'
        ? on.canvasKind
        : view === 'prs'
          ? 'pr'
          : view === 'todo' || (view === 'kanban' && on.selectedKind !== 'agent')
            ? 'todo'
            : (view === 'sessions' || view === 'kanban') && on.selectedKind === 'agent'
              ? 'session'
              : 'lane';
}
