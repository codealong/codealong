// A terminal of Codealong's the server asked the main window to show (launch/codealong.rs; a
// reveal of kind `terminal`, App.tsx `reveal`): its session's panel, or a window of its own for
// one that's no session's, once the board says whose it is (terminals.ts `placeOf`), looked at
// again as boards come and once the wait is up.

import { useEffect, useRef } from 'react';
import { terminalWindow, type Reveal } from './api';
import { useLatest } from './latest';
import { allTerminals, placeOf, setDrawer, setSessionView, TERMINAL_WAIT_MS, usePoppedTerminals } from './terminals';
import type { Board } from './types';

/** The terminal asked for, by its id; whether it may yet turn out to be a session's; and when it
 *  was asked for. */
export interface PendingTerminal {
  id: string;
  wait: boolean;
  since: number;
}

export function useShownTerminal({ board, reveal, setToast }: { board: Board | null; reveal: { readonly current: (r: Reveal) => void }; setToast: (message: string) => void }) {
  const pendingTerminal = useRef<PendingTerminal | null>(null);
  const poppedTerminals = usePoppedTerminals();
  const placeTerminal = useLatest(() => {
    const p = pendingTerminal.current;
    if (!p) return;
    const t = allTerminals(board?.terminals).find((x) => x.id === p.id);
    const place = placeOf(t, p.wait, Date.now() - p.since >= TERMINAL_WAIT_MS);
    if (place.to === 'wait') return;
    pendingTerminal.current = null;
    if (place.to === 'none') return;
    // One in a window of its own (no session's, or popped out) comes forward there.
    if (place.to === 'window' || poppedTerminals.includes(p.id)) {
      terminalWindow(t?.session ?? '', p.id, true).catch((e: unknown) => setToast(String(e)));
      return;
    }
    const lane = (board?.lanes ?? []).find((l) => l.sessions.some((s) => s.key === place.key));
    if (lane) reveal.current({ kind: 'session', laneId: lane.id, key: place.key });
    if (place.home) {
      setSessionView(place.key, 'terminal');
      setDrawer(place.key, { homeAt: Date.now() });
    } else setDrawer(place.key, { open: true, active: p.id, focusAt: Date.now() });
  });
  useEffect(() => placeTerminal.current(), [board, placeTerminal]);
  /** The one asked for, set as it's asked; and placing it now, by what the latest board says. */
  return { pendingTerminal, placeTerminal };
}
