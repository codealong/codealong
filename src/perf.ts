// What a window costs, for the perf log (perf.rs): on while `[debug] perf_log` is set.

import { useEffect, useLayoutEffect, type MutableRefObject } from 'react';
import { perfMark } from './api';
import type { Board } from './types';

/** While `[debug] perf_log` is on, reports to the perf log what this window costs: each board
 *  push from arrival to commit and to paint, pointer events that waited in the queue (the
 *  wait a hover feels), and event-loop stalls, once a second at most. */
export function usePerf(board: Board | null, arrived: MutableRefObject<number | null>) {
  const on = board?.perfLog ?? false;
  useLayoutEffect(() => {
    const at = arrived.current;
    if (!on || at == null) return;
    arrived.current = null;
    const commit = performance.now() - at;
    requestAnimationFrame(() =>
      window.setTimeout(() => {
        const painted = performance.now() - at;
        const rows = document.querySelectorAll('[role="option"]').length;
        perfMark(`push: commit ${commit.toFixed(0)} ms, painted ${painted.toFixed(0)} ms, ${rows} rows`);
      }, 0),
    );
  }, [board, on, arrived]);
  useEffect(() => {
    if (!on) return;
    let worst = 0;
    let moves = 0;
    const onMove = (e: PointerEvent) => {
      worst = Math.max(worst, performance.now() - e.timeStamp);
      moves++;
    };
    window.addEventListener('pointermove', onMove, { capture: true, passive: true });
    let last = performance.now();
    const id = window.setInterval(() => {
      const now = performance.now();
      const late = now - last - 1000;
      last = now;
      // A hidden window's timers are slowed on purpose (the bar, when a board window is up).
      if (late > 100 && !document.hidden) perfMark(`stall: a 1 s timer ran ${late.toFixed(0)} ms late`);
      if (worst > 50) perfMark(`input: pointer moves waited up to ${worst.toFixed(0)} ms (${moves} in 1 s)`);
      worst = 0;
      moves = 0;
    }, 1000);
    return () => {
      window.removeEventListener('pointermove', onMove, { capture: true });
      window.clearInterval(id);
    };
  }, [on]);
}
