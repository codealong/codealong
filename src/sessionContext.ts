// What a session's header shows besides the session itself (SessionHeader.tsx), looked up from
// the board: the PRs it worked on, the todos it's on, and the session it came from with the ones
// that came of it. The main window has indexes of its own for these; the windows that show one
// session at a time (the bar's peek, the waiting window) look them up here.

import { sessionHomes, sessionPrs } from './lanes';
import { lineageOf, type Lineage } from './lineage';
import type { Board, Lane, Pr, Session, Todo } from './types';

export interface SessionContext {
  prs: Pr[];
  todos: Todo[];
  lineage: Lineage;
}

/** Without a board yet, what the session's own worktree says: its PR, when the session is on its branch. */
export function sessionContext(board: Pick<Board, 'prs' | 'lanes' | 'todos'> | null | undefined, lane: Lane, session: Session): SessionContext {
  // Archived sessions are off the lists, so they aren't named as kin either.
  const homes = new Map([...sessionHomes(board?.lanes ?? [])].filter(([, p]) => !p.session.archived));
  return {
    prs: sessionPrs(board?.prs ?? [], lane, session),
    todos: (board?.todos ?? []).filter((t) => t.sessions.includes(session.key)),
    lineage: lineageOf(session, homes),
  };
}
