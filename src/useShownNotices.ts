// What isn't working in the background, as the main window shows it at the end of its accounts
// bar (notices.ts, ui/Notices.tsx): the engine's notices and this window's own, less the ones
// dismissed, which stay dismissed across launches while they still stand.

import { useEffect, useMemo } from 'react';
import { keptDismissals, shownNotices, useReportedNotices } from './notices';
import { strings, useStored } from './stored';
import type { Board } from './types';

const DISMISSED_KEY = 'codealong.dismissedNotices';

export function useShownNotices(board: Board | null) {
  const reported = useReportedNotices();
  const allNotices = useMemo(() => [...(board?.notices ?? []), ...reported], [board?.notices, reported]);
  const [dismissed, setDismissed] = useStored<string[]>(DISMISSED_KEY, strings, []);
  const boardRead = !!board && !board.loading;
  useEffect(() => {
    // Before the board is read it lists none, which isn't their having gone.
    if (!boardRead) return;
    setDismissed((d) => {
      const kept = keptDismissals(d, allNotices);
      return kept.length === d.length ? d : kept;
    });
  }, [allNotices, boardRead, setDismissed]);
  const notices = useMemo(() => shownNotices(allNotices, dismissed), [allNotices, dismissed]);
  /** The notices to show, and the dismissed ones' ids to add one to. */
  return { notices, setDismissed };
}
