// The quick panel's rows that are its own, beside the sessions, reviews, todos and PRs it lists
// (quick.ts) and the skills (skills.ts): what Jev suggests, the row that lists the rest of the
// sessions a search found, and Send to's. With them, what a row pins to the desktop when it's
// dragged out of the panel, and the groups the rows are listed in. Kept free of React so the
// rules have tests; ui/QuickRows.tsx draws them.

import type { PrRow, ReviewRow, RowGroup, SessionRow, TodoRow } from './quick';
import { suggestions, type Suggestion } from './route';
import type { SkillRow } from './skills';
import type { Board, DesktopPinned, Route } from './types';

/** The new session's row, under the list: its key among the rows the arrows move through. */
export const COMPOSE = 'compose';

/** A suggestion from Jev, as a row. */
export interface SuggestRow {
  kind: 'suggest';
  key: string;
  group: 'suggested';
  s: Suggestion;
}

/** Under a search's first few sessions, the rest of them, a click away. */
export interface MoreRow {
  kind: 'more';
  key: string;
  group: 'found';
  count: number;
}

/** Under Jev's suggestions for a prompt (under the sessions found for a search, and with routing off), the way to
 *  send it to any session, not only one Jev suggests: the box then finds the session while the
 *  prompt waits under it (`sendTo`). */
export interface SendToRow {
  kind: 'sendto';
  key: string;
  group: 'suggested' | 'found';
  /** How many sessions it could go to. */
  count: number;
}

export type Row = SessionRow | ReviewRow | TodoRow | PrRow | SuggestRow | MoreRow | SendToRow | SkillRow;

/** Jev's suggestions as rows. Jev knows this Mac's worktrees, so it suggests starting only while a
 *  session would start here, not on another Mac. `isolated` is the place's `isolatesIn`, a line
 *  each, so new work starts in a repo where the start row would. */
export function suggestRows(route: Route | null, board: Board | null, elsewhere: boolean, isolated: string): SuggestRow[] {
  if (!board || !route) return [];
  return suggestions(route, board, isolated ? isolated.split('\n') : [])
    .filter((sg) => !elsewhere || sg.kind !== 'start')
    .map((sg) => ({ kind: 'suggest', key: sg.key, group: 'suggested', s: sg }));
}

/** A title with the typed words underlined. */
/** What a result pins to the desktop when it's dragged out of the panel: its session, todo or
 *  PR. Null for what has no card: a session on another device or archived, a start, more results. */
export function deskPin(r: Row | undefined): DesktopPinned | null {
  const session = r?.kind === 'session' ? r : r?.kind === 'suggest' && (r.s.kind === 'send' || r.s.kind === 'open') ? r.s : null;
  if (session) return session.lane.device || session.session.archived ? null : { ref: 'session', key: session.session.key };
  if (r?.kind === 'todo') return { ref: 'todo', key: r.item.todo.id };
  if (r?.kind === 'pr' || r?.kind === 'review') return { ref: 'pr', key: r.pr.key };
  return null;
}

/** What the pointer holds while it drags a result. */
export function deskTitle(r: Row | undefined): string {
  if (r?.kind === 'session') return r.session.title;
  if (r?.kind === 'suggest' && (r.s.kind === 'send' || r.s.kind === 'open')) return r.s.session.title;
  if (r?.kind === 'todo') return r.item.todo.title || 'Untitled';
  if (r?.kind === 'pr' || r?.kind === 'review') return `#${r.pr.number} ${r.pr.title}`;
  return '';
}

/** A run of rows of one group, under its heading. */
export interface RowSection {
  group: RowGroup;
  rows: Row[];
}

/** The rows in the groups they're listed in, each run of one group under its heading, with the
 *  headings that are there with nothing under them. `routed`: what's typed goes to Jev;
 *  `searching`: it's looked for; `inSendTo`: the box finds the session a prompt goes to. */
export function rowSections(rows: Row[], { routed, searching, inSendTo }: { routed: boolean; searching: boolean; inSendTo: boolean }): RowSection[] {
  const sections: RowSection[] = [];
  for (const r of rows) {
    const last = sections[sections.length - 1];
    if (last && last.group === r.group) last.rows.push(r);
    else sections.push({ group: r.group, rows: [r] });
  }
  // While what's typed goes to Jev, Suggested is there before Jev answers and when it has nothing
  // to suggest, so the rows under it don't move as answers come (ui/QuickRows.tsx `SuggestedStandIn`).
  if (routed && !inSendTo && sections[0]?.group !== 'suggested') sections.unshift({ group: 'suggested', rows: [] });
  // While there's a search, Sessions' heading is there even with no session found: it has the box
  // that lists archived sessions too. So has Send to's, with no session found in it.
  if (searching && !sections.some((x) => x.group === 'found')) sections.splice(sections[0]?.group === 'suggested' ? 1 : 0, 0, { group: 'found', rows: [] });
  if (inSendTo && sections.length === 0) sections.push({ group: 'sendto', rows: [] });
  return sections;
}
