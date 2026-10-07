// How a window follows the board: the engine's pushes, and the board as it is now, read once as
// the window starts following. The read goes out only once the pushes are listened to, so no
// push falls between the two. It can come back after a push that's newer than it, and is then
// dropped: it would put the older board back in place of the newer, and nothing might come to
// put that right, since the engine pushes only a board that changed. Pushes come in the order
// the engine made them, and each is taken: a board's time is the engine's clock, and a clock set
// back mustn't hold every push after it.
//
// The main window (useBoard.ts), the waiting window (Waiting.tsx), a kept session's window
// (Peek.tsx) and the cards that keep up with the board while they show (cardHost.ts) follow it
// this way. The quick panel doesn't: the script that opens it hands it the board (Quick.tsx).

import { getBoard, onBoard } from './api';
import type { Board } from './types';

/** Whether a board read on asking is taken, by when the engine made it: not when the board taken
 *  last (a push, or an earlier read) was made after it. */
export function readWins(lastAt: number | null, readAt: number): boolean {
  return lastAt === null || readAt >= lastAt;
}

export interface Following {
  /** Stops following: nothing more is taken, a read still out included. */
  off: () => void;
  /** Reads the board again, taken by the same rule; `failed` hears when it couldn't be read. */
  read: (failed?: (e: unknown) => void) => void;
}

/** Follows the board, handing `take` each board that comes and how it came; `failed` hears when
 *  the first read couldn't be made. */
export function followBoard(take: (board: Board, how: 'push' | 'read') => void, failed?: (e: unknown) => void): Following {
  let over = false;
  let lastAt: number | null = null;
  const read = (didFail?: (e: unknown) => void) => {
    getBoard().then(
      (b) => {
        if (over || !readWins(lastAt, b.generatedAt)) return;
        lastAt = b.generatedAt;
        take(b, 'read');
      },
      (e: unknown) => {
        if (!over) didFail?.(e);
      },
    );
  };
  const unlisten = onBoard(
    (b) => {
      if (over) return;
      lastAt = b.generatedAt;
      take(b, 'push');
    },
    () => {
      if (!over) read(failed);
    },
  );
  return {
    off: () => {
      over = true;
      unlisten();
    },
    read,
  };
}
