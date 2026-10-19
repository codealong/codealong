// The boards of the project a window shows, and the one it shows: read from the engine as the
// project changes and whenever a board changes, here, in another window or from the command
// (boards.rs `changed`). A change made here shows at once (model.ts `applyOp`) and goes to the
// engine; once the changes still on their way have landed, what the engine has is read again, so
// a change it turned down goes away with the error it says.

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { boardOp, boardsList, onBoardsChanged, type BoardApplied } from '../api';
import { useLatest } from '../latest';
import { textByName, useStored } from '../stored';
import { applyOp, boardsIn, readBoard, type BoardDoc, type BoardOp } from './model';

/** The board each project shows, by project. */
const BOARD_KEY = 'codealong.board';

export interface BoardsHandle {
  /** The project's boards, its own first. */
  boards: BoardDoc[];
  /** The one the Board view shows. */
  board: BoardDoc;
  /** Shows a board: one of this project's, or of `inProject` for the window about to show it. */
  pick: (id: string, inProject?: string) => void;
  /** Makes a change, here at once and then in the engine; rejects with what the engine said. */
  apply: (op: BoardOp) => Promise<BoardApplied | null>;
}

export function useBoards(project: string): BoardsHandle {
  const [held, setHeld] = useState<{ project: string; boards: BoardDoc[] } | null>(null);
  const [picked, setPicked] = useStored(BOARD_KEY, textByName, {} as Record<string, string>);
  // Changes still on their way to the engine: what it has is read again once none are, so a read
  // between two changes doesn't put back what the second one changed.
  const pending = useRef(0);
  // The project shown when the last change lands, which may not be the one it was made in.
  const projectNow = useLatest(project);

  const read = useCallback((p: string) => {
    if (pending.current > 0) return;
    boardsList(p)
      .then((list) => {
        if (pending.current === 0) setHeld((h) => (h && h.project !== p ? h : { project: p, boards: list.map(readBoard) }));
      })
      // A list that doesn't read leaves what's shown, the project's own board at least.
      .catch(() => {});
  }, []);

  useEffect(() => {
    setHeld((h) => (h?.project === project ? h : { project, boards: [] }));
    read(project);
  }, [project, read]);
  useEffect(() => onBoardsChanged((c) => c.project === project && read(project)), [project, read]);

  const boards = useMemo(() => boardsIn(held?.project === project ? held.boards : [], project), [held, project]);
  const board = boards.find((b) => b.id === picked[project]) ?? boards[0];

  const pick = useCallback((id: string, inProject?: string) => setPicked((p) => ({ ...p, [inProject ?? project]: id })), [project, setPicked]);

  const apply = useCallback(
    async (op: BoardOp) => {
      setHeld((h) => ({ project, boards: applyOp(h?.project === project ? h.boards : [], op, Date.now()) }));
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

  return { boards, board: board ?? boards[0], pick, apply };
}
