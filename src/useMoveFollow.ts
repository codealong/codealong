// Follows a moved or edited session in place for any conversation window (Peek, Kept, Notch,
// Waiting): holds the window open on the old session while the move runs and switches to the copy
// once the board lists it—either by exact key or, for async terminal/app launches, by `origin.key`.

import { useEffect, useState } from 'react';
import { findMovedCopy, forkOriginKeys, type MoveFollow } from './handoff';
import { useLatest } from './latest';
import type { Lane, Session } from './types';

/** How long a window waits for a moved session's copy to appear on the board (App.tsx matches). */
export const MOVE_FOLLOW_WAIT_MS = 60_000;

interface MoveWait {
  from: string;
  /** Exact key when returned, `'origin'` when matched asynchronously by `origin.key`, or `null`
   *  while the move request is still in flight. */
  to: string | 'origin' | null;
  /** Pre-existing forks of `from` snapshotted before the move started. */
  known: ReadonlySet<string>;
}

/** Tracks an in-flight move for `shownKey` and calls `onFollow` once the copy lands on `lanes`. */
export function useMoveFollow(
  shownKey: string | null,
  lanes: readonly Lane[] | undefined,
  onFollow: (copy: { lane: Lane; session: Session }) => void,
): { moving: boolean; start: (from: string, copy: Promise<MoveFollow>) => void } {
  const [wait, setWait] = useState<MoveWait | null>(null);
  const lanesNow = useLatest(lanes);
  const followNow = useLatest(onFollow);

  const start = (from: string, copy: Promise<MoveFollow>) => {
    const known = forkOriginKeys(lanesNow.current ?? [], from);
    const entry: MoveWait = { from, to: null, known };
    setWait(entry);
    void copy.then((to) =>
      setWait((w) => (w !== entry ? w : to ? { from, to: to.kind === 'key' ? to.key : 'origin', known } : null)),
    );
  };

  useEffect(() => {
    if (!wait) return;
    if (shownKey !== wait.from) {
      setWait(null);
      return;
    }
    if (!wait.to || !lanes) return;
    const copy = findMovedCopy(lanes, wait.from, wait.to === 'origin' ? null : wait.to, wait.known);
    if (!copy) return;
    setWait(null);
    followNow.current(copy);
  }, [wait, shownKey, lanes, followNow]);

  useEffect(() => {
    if (!wait?.to) return;
    const t = window.setTimeout(() => setWait(null), MOVE_FOLLOW_WAIT_MS);
    return () => window.clearTimeout(t);
  }, [wait?.from, wait?.to]);

  return { moving: wait !== null && shownKey === wait.from, start };
}
