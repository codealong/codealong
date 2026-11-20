// What a card on the canvas does in the main window (App.tsx, ui/CanvasView.tsx), from what a
// render of the window has: opened where its thing lives, as a double-click on its row would, and
// its right-click menu, which is the one its row has in the other views.

import { openUrl } from '../api';
import type { ListRow } from '../boardLists';
import type { MenuSpec } from '../contextMenu';
import type { TodoItem } from '../todos';
import type { Lane, PrItem, Session, Todo } from '../types';
import type { PrRowItem } from '../ui/PrsView';
import type { Entities, RefItem } from './model';

/** What the window has for them, as one render has it. */
export interface CardsHas {
  /** What the canvas can show, and every todo of the board's project as the Todo view rows them. */
  canvasEntities: Entities;
  todoItems: readonly TodoItem[];
  /** Opens one of a worktree's targets where it lives; and a row, as a double-click on it does. */
  open: (lane: Lane | null, targetId: string | null | undefined) => void;
  openRow: (row: ListRow | null) => void;
  setToast: (message: string) => void;
  /** The menus the rows have in the other views. */
  laneMenuSpec: (lane: Lane) => MenuSpec;
  sessionMenuSpec: (lane: Lane, s: Session, pin: 'session' | 'worktree') => MenuSpec;
  todoMenuSpec: (r: TodoItem) => MenuSpec;
  prMenuSpec: (r: PrRowItem) => MenuSpec;
  prRowOf: (pr: PrItem) => PrRowItem;
  /** Shows a todo in the Todo view. */
  showTodo: (t: Todo) => void;
}

export function cardActsOf(has: CardsHas): { openCard: (it: RefItem) => void; cardMenu: (it: RefItem) => MenuSpec } {
  /** Opens a card on the canvas where it lives, as a double-click on its row would. */
  const openCard = (it: RefItem) => {
    const e = has.canvasEntities;
    if (it.ref === 'worktree') {
      const lane = e.lanes.get(it.key);
      if (lane) has.open(lane, lane.defaultTarget);
    } else if (it.ref === 'session') {
      const p = e.sessions.get(it.key);
      if (p) has.open(p.lane, p.session.openTarget);
    } else if (it.ref === 'todo') {
      has.openRow(has.todoItems.find((r) => r.todo.id === it.key) ?? null);
    } else {
      const pr = e.prs.get(it.key);
      if (pr) openUrl(pr.url).catch((err: unknown) => has.setToast(String(err)));
    }
  };

  /** A card's right-click menu: the one its row has in the other views. */
  const cardMenu = (it: RefItem): MenuSpec => {
    const e = has.canvasEntities;
    if (it.ref === 'worktree') {
      const lane = e.lanes.get(it.key);
      return lane ? has.laneMenuSpec(lane) : [];
    }
    if (it.ref === 'session') {
      const p = e.sessions.get(it.key);
      return p ? has.sessionMenuSpec(p.lane, p.session, 'session') : [];
    }
    if (it.ref === 'todo') {
      const r = has.todoItems.find((x) => x.todo.id === it.key);
      return r ? [...has.todoMenuSpec(r), 'separator', { label: 'Show in Todo', run: () => has.showTodo(r.todo) }] : [];
    }
    const pr = e.prs.get(it.key);
    return pr ? has.prMenuSpec(has.prRowOf(pr)) : [];
  };
  return { openCard, cardMenu };
}
