// What a card shows, for the Canvas view's cards and the cards pinned to the desktop alike
// (card/CardFace.tsx draws it, card/CardPopover.tsx its popover): a worktree, session, todo or PR
// looked up from the board, and what it's up to in a line.

import { dayKey } from '../../due';
import type { Entities, RefItem } from '../../canvas/model';
import type { Label, Lane, PrItem, Session, Todo } from '../../types';

/** What a card shows, looked up from the board; null when the board no longer has it. */
export type CardData =
  | { ref: 'worktree'; lane: Lane }
  | { ref: 'session'; lane: Lane; session: Session }
  | { ref: 'todo'; todo: Todo; labels: Label[]; sessions: { lane: Lane; session: Session }[]; lane: Lane | null }
  | { ref: 'pr'; pr: PrItem };

export function cardData(e: Entities, it: RefItem, labels: Map<string, Label>): CardData | null {
  switch (it.ref) {
    case 'worktree': {
      const lane = e.lanes.get(it.key);
      return lane ? { ref: 'worktree', lane } : null;
    }
    case 'session': {
      const p = e.sessions.get(it.key);
      return p ? { ref: 'session', lane: p.lane, session: p.session } : null;
    }
    case 'todo': {
      const todo = e.todos.get(it.key);
      if (!todo) return null;
      const sessions = todo.sessions.flatMap((k) => e.sessions.get(k) ?? []).sort((a, b) => b.session.lastActivity - a.session.lastActivity);
      const lane = todo.worktrees.map((id) => e.lanes.get(id)).find(Boolean) ?? sessions[0]?.lane ?? null;
      return { ref: 'todo', todo, labels: todo.labels.flatMap((id) => labels.get(id) ?? []), sessions, lane };
    }
    case 'pr': {
      const pr = e.prs.get(it.key);
      return pr ? { ref: 'pr', pr } : null;
    }
  }
}

/** What a session is up to, in a line: what it runs or asks while it's busy, else the start of
 *  its last message, which agents open with where the work stands. */
export function sessionLine(s: Session): string {
  const busy = s.status === 'working' || s.status === 'needs' || s.status === 'error';
  const detail = (s.detail ?? '').replace(/^\{(?:for|ago):-?\d+\}\s*·\s*/, '');
  const said = (s.excerpt ?? '').split('\n')[0];
  return (busy ? detail || said : said || detail) || s.headline;
}

/** Everything a card draws, as one string: a card redraws only when this changes. */
export function cardSig(d: CardData | null, now: number): string {
  if (!d) return 'gone';
  const minute = Math.floor(now / 60_000);
  switch (d.ref) {
    case 'worktree': {
      const l = d.lane;
      return `w|${l.glyph}|${l.bay}|${l.callsign}|${l.branch}|${l.headline}|${l.repo}|${l.pr?.number}|${l.pr?.state}|${l.pr?.checks.failed}|${l.pr?.checks.pending}|${l.pr?.checks.passed}|${l.git?.baseAhead}|${l.git?.baseBehind}|${l.lastActivity}|${l.device?.color}|${l.device?.online}|${minute}`;
    }
    case 'session': {
      const s = d.session;
      const pr = d.lane.pr;
      return `s|${s.status}|${s.title}|${sessionLine(s)}|${s.account}|${s.surface}|${s.lastActivity}|${s.statusSince}|${d.lane.callsign}|${d.lane.device?.color}|${d.lane.device?.online}|${pr?.number}|${pr?.state}|${pr?.checks.failed}|${pr?.checks.pending}|${pr?.checks.passed}|${minute}`;
    }
    case 'todo': {
      const t = d.todo;
      // A due date reads by the day, so the card redraws when the day turns.
      return `t|${t.status}|${t.title}|${!!t.waiting}|${t.labels.join()}|${d.labels.map((l) => l.color + l.name).join()}|${d.sessions.map((s) => s.session.key + s.session.status).join()}|${t.updatedAt}|${t.due ? `${t.due}@${dayKey(now)}` : ''}`;
    }
    case 'pr': {
      const p = d.pr;
      return `p|${p.state}|${p.title}|${p.checks.failed}|${p.checks.pending}|${p.checks.passed}|${p.merge}|${p.updatedAt}|${minute}`;
    }
  }
}
