// The board as the main window follows it: read once as the window opens, then taken push by
// push from the engine, each push handed over with the parts that didn't change as the objects
// it already had (reuse.ts), and with your changes the engine hasn't shown yet on top (ahead.ts).
//
// The quick panel and the cards on the desktop follow the board differently, so they don't use
// this: the quick panel is handed its board by the script that opens it and listens only while
// it's up (Quick.tsx), and a card keeps the part of the board around one session while it shows
// (cardHost.ts).

import { useEffect, useMemo, useRef, useState } from 'react';
import { boardAhead, useAhead, type Ahead } from './ahead';
import { getBoard, onBoard } from './api';
import { usePerf } from './perf';
import { reuse } from './reuse';
import type { Board } from './types';

export interface FollowedBoard {
  /** The board as the engine sent it last. */
  engine: Board | null;
  /** The same with your changes on top: what the window shows. */
  board: Board | null;
  /** Your changes the engine hasn't shown yet, for what else the window shows with them (other
   *  devices' worktrees). */
  ahead: Ahead[];
  /** Why the board couldn't be read, when it couldn't. */
  loadError: string | null;
  /** Until the engine's first scan has finished, the board has none of this Mac's sessions,
   *  worktrees, todos or PRs: the views say they're being read rather than show nothing, or only
   *  the other devices' worktrees, which come in first from what their feeds kept. */
  loading: boolean;
}

export function useBoard(): FollowedBoard {
  const [engine, setEngine] = useState<Board | null>(null);
  const ahead = useAhead();
  const board = useMemo(() => (engine ? boardAhead(engine, ahead) : null), [engine, ahead]);
  const [loadError, setLoadError] = useState<string | null>(null);
  const loading = !loadError && (!engine || engine.loading);

  // When the push being drawn arrived, for the perf log.
  const arrived = useRef<number | null>(null);
  usePerf(engine, arrived);
  useEffect(() => {
    getBoard()
      .then((b) => setEngine((prev) => (prev ? reuse(prev, b) : b)))
      .catch((e: unknown) => setLoadError(String(e)));
    // A window that can't be seen keeps only the latest push and shows it when it's seen again:
    // the engine publishes many times a minute, and every render of a board no one sees is waste.
    let held: Board | null = null;
    const show = (b: Board) => {
      arrived.current = performance.now();
      setEngine((prev) => (prev ? reuse(prev, b) : b));
    };
    const onVisible = () => {
      if (document.hidden || !held) return;
      const b = held;
      held = null;
      show(b);
    };
    document.addEventListener('visibilitychange', onVisible);
    const off = onBoard((b) => {
      if (document.hidden) held = b;
      else show(b);
    });
    return () => {
      off();
      document.removeEventListener('visibilitychange', onVisible);
    };
  }, []);

  return { engine, board, ahead, loadError, loading };
}
