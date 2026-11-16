// What a window costs, for the perf log (perf.rs): on while `[debug] perf_log` is set.

import { useEffect, useLayoutEffect, type MutableRefObject } from 'react';
import { perfMark } from './api';
import { useTypingTrace } from './typingTrace';

/** While `[debug] perf_log` is on, reports to the perf log what this window costs: each board
 *  push (board or bar feed) from arrival to commit and to paint, pointer events that waited in the queue (the
 *  wait a hover feels), typed keys (`useKeyPerf`) and event-loop stalls, once a second at most;
 *  and what moves the caret in a session's box (typingTrace.ts). */
export function usePerf(source: { perfLog?: boolean } | null, arrived: MutableRefObject<number | null>) {
  const on = source?.perfLog ?? false;
  // What moves the caret in a session's box, too (typingTrace.ts).
  useTypingTrace(on);
  useKeyPerf(on);
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

/** A key's times, in ms after macOS had it. */
interface KeyTimes {
  /** When the page had it: what it waited on before the page ran anything for it. */
  reached: number;
  /** When the frame after it was drawn. */
  drawn: number;
}

/** The median of what's sorted. */
function median(sorted: number[]): number {
  return sorted[Math.floor(sorted.length / 2)];
}

/** What a second's keys say in the perf log: how long they took to show (median and worst), and
 *  how much of it was spent before the page had them. */
export function keysLine(keys: KeyTimes[]): string {
  const drawn = keys.map((k) => k.drawn).sort((a, b) => a - b);
  const reached = keys.map((k) => k.reached).sort((a, b) => a - b);
  return `keys: ${keys.length} in 1 s, drawn ${median(drawn).toFixed(0)} ms after the key (median), ${drawn[drawn.length - 1].toFixed(0)} ms at most; in the page ${median(reached).toFixed(0)} ms after it (median), ${reached[reached.length - 1].toFixed(0)} ms at most`;
}

/** While `[debug] perf_log` is on, how long keys typed in this window take to show: from each
 *  key's event to the frame drawn after it, summed up once a second while keys come. A key event
 *  carries the time macOS had it, so waiting on the app's main thread counts as well as waiting
 *  in the page; when the page had it is said apart, as what came before the page ran anything,
 *  and the rest is the page's own work and its wait for a frame. */
export function useKeyPerf(on: boolean): void {
  useEffect(() => {
    if (!on) return;
    let keys: KeyTimes[] = [];
    const onKey = (e: KeyboardEvent) => {
      const at = e.timeStamp;
      const reached = performance.now() - at;
      requestAnimationFrame(() => window.setTimeout(() => keys.push({ reached, drawn: performance.now() - at }), 0));
    };
    // Heard before anything else in the window, so the page's own work for the key comes after.
    window.addEventListener('keydown', onKey, true);
    const id = window.setInterval(() => {
      if (keys.length === 0) return;
      const line = keysLine(keys);
      keys = [];
      perfMark(line);
    }, 1000);
    return () => {
      window.removeEventListener('keydown', onKey, true);
      window.clearInterval(id);
    };
  }, [on]);
}
