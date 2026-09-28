// What a window costs, for the perf log (perf.rs): on while `[debug] perf_log` is set.

import { useCallback, useEffect, useLayoutEffect, useRef, type MutableRefObject } from 'react';
import { perfMark } from './api';

/** While `[debug] perf_log` is on, reports to the perf log what this window costs: each board
 *  push (board or bar feed) from arrival to commit and to paint, pointer events that waited in the queue (the
 *  wait a hover feels), and event-loop stalls, once a second at most. */
export function usePerf(source: { perfLog?: boolean } | null, arrived: MutableRefObject<number | null>) {
  const on = source?.perfLog ?? false;
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
  }, [source, on, arrived]);
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

/** While `[debug] perf_log` is on, how long typed keys take to show: from each key's event to the
 *  frame drawn after it, summed up once a second while keys come. A key event carries the time
 *  macOS had it, so waiting on the app's main thread counts as well as waiting in the page. Call
 *  what this returns on every keydown. */
export function useKeyPerf(on: boolean): (e: { timeStamp: number }) => void {
  const keys = useRef<number[]>([]);
  useEffect(() => {
    if (!on) return;
    const id = window.setInterval(() => {
      const k = keys.current.sort((a, b) => a - b);
      if (k.length === 0) return;
      keys.current = [];
      perfMark(`keys: ${k.length} in 1 s, drawn ${k[Math.floor(k.length / 2)].toFixed(0)} ms after the key (median), ${k[k.length - 1].toFixed(0)} ms at most`);
    }, 1000);
    return () => window.clearInterval(id);
  }, [on]);
  return useCallback(
    (e: { timeStamp: number }) => {
      if (!on) return;
      const at = e.timeStamp;
      requestAnimationFrame(() => window.setTimeout(() => keys.current.push(performance.now() - at), 0));
    },
    [on],
  );
}
