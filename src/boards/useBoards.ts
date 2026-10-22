// The boards of the project a window shows (under All projects, every project's), and the one it
// shows: read from the engine as the project changes and whenever a board changes, here, in
// another window or from the command (boards.rs `changed`). A change made here shows at once (model.ts `applyOp`) and goes to the
// engine; once the changes still on their way have landed, what the engine has is read again, so
// a change it turned down goes away with the error it says.

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { boardOp, boardsList, onBoardsChanged, type BoardApplied } from '../api';
import { useLatest } from '../latest';
import { ALL_PROJECTS } from '../quick';
import { errorText } from '../report';
import { textByName, useStored } from '../stored';
import { applyOp, boardsIn, readBoard, type BoardDoc, type BoardOp } from './model';

/** The board each project shows, by project. */
const BOARD_KEY = 'codealong.board';

export interface BoardsHandle {
  /** The project's boards, as they were made. */
  boards: BoardDoc[];
  /** The one the Board view shows: the one picked last, else the first; none while the project
   *  has none. */
  board: BoardDoc | null;
  /** Whether the project's boards have been read, so that none means it has none. */
  read: boolean;
  /** Why they couldn't be read, when they couldn't. */
  error: string | null;
  /** Shows a board: one of this project's, or of `inProject` for the window about to show it. */
  pick: (id: string, inProject?: string) => void;
  /** Makes a change, here at once and then in the engine; rejects with what the engine said. */
  apply: (op: BoardOp) => Promise<BoardApplied | null>;
}

interface Held {
  project: string;
  boards: BoardDoc[];
  read: boolean;
  error: string | null;
}

export function useBoards(project: string): BoardsHandle {
  const [held, setHeld] = useState<Held | null>(null);
  const [picked, setPicked] = useStored(BOARD_KEY, textByName, {} as Record<string, string>);
  // Changes still on their way to the engine: what it has is read again once none are, so a read
  // between two changes doesn't put back what the second one changed.
  const pending = useRef(0);
  // The project shown when the last change lands, which may not be the one it was made in.
  const projectNow = useLatest(project);

  const read = useCallback((p: string) => {
    if (pending.current > 0) return;
    boardsList(p).then(
      (list) => {
        if (pending.current === 0) setHeld((h) => (h && h.project !== p ? h : { project: p, boards: list.map(readBoard), read: true, error: null }));
      },
      // A list that doesn't read leaves what's shown, and says why.
      (e: unknown) => setHeld((h) => (h && h.project !== p ? h : { project: p, boards: h?.boards ?? [], read: true, error: errorText(e) })),
    );
  }, []);

  useEffect(() => {
    setHeld((h) => (h?.project === project ? h : { project, boards: [], read: false, error: null }));
    read(project);
  }, [project, read]);
  // All projects lists every project's boards, so a change to any of them is its.
  useEffect(() => onBoardsChanged((c) => (c.project === project || project === ALL_PROJECTS) && read(project)), [project, read]);

  const here = held?.project === project ? held : null;
  const boards = useMemo(() => boardsIn(here?.boards ?? [], project), [here, project]);
  const board = boards.find((b) => b.id === picked[project]) ?? boards[0] ?? null;

  const pick = useCallback((id: string, inProject?: string) => setPicked((p) => ({ ...p, [inProject ?? project]: id })), [project, setPicked]);

  const apply = useCallback(
    async (op: BoardOp) => {
      setHeld((h) => {
        const mine = h?.project === project ? h : null;
        return { project, boards: applyOp(mine?.boards ?? [], op, Date.now()), read: mine?.read ?? false, error: mine?.error ?? null };
      });
      pending.current += 1;
      try {
        return await boardOp(op);
      } finally {
        pending.current -= 1;
        read(projectNow.current);
      }
    },
    [project, projectNow, read],
  );

  return { boards, board, read: !!here?.read, error: here?.error ?? null, pick, apply };
}
