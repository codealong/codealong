// What isn't working in the background, said in one place and one way: the board's notices
// (the engine's: a file that doesn't read, GitHub not answering) and any a window reports
// itself (`reportNotice`). `ui/Notices.tsx` draws them. What goes wrong with something you just
// did is a toast where you did it, never one of these.
import { useSyncExternalStore } from 'react';
import { duration } from './format';
import type { Notice } from './types';

/** What a dismissal goes by. A notice that comes back in other words is said again. */
export function noticeId(n: Notice): string {
  return `${n.key}\n${n.title}`;
}

/** The notices to show: those you haven't dismissed, one for each thing they're about. */
export function shownNotices(notices: Notice[], dismissed: string[]): Notice[] {
  const seen = new Set<string>();
  return notices.filter((n) => {
    const id = noticeId(n);
    if (seen.has(id) || dismissed.includes(id)) return false;
    seen.add(id);
    return true;
  });
}

/** The dismissals to keep: those whose notice still stands. One that went away is forgotten,
 *  so when it happens again it's said again. */
export function keptDismissals(dismissed: string[], notices: Notice[]): string[] {
  const standing = new Set(notices.map(noticeId));
  return dismissed.filter((id) => standing.has(id));
}

function clock(at: number): string {
  return new Date(at).toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit' });
}

/** A notice in a few words: its title, and until when where that's known and still ahead. */
export function noticeLabel(n: Notice, now: number): string {
  return n.until && n.until > now ? `${n.title} until ${clock(n.until)}` : n.title;
}

/** When a notice ends, in full: `Until 3:43 PM, in 12m`. */
export function untilPhrase(n: Notice, now: number): string | null {
  return n.until && n.until > now ? `Until ${clock(n.until)}, in ${duration(now, n.until)}` : null;
}

/** What a notice offers to open, by what it's about. */
export function noticeAction(n: Notice): { label: string; opens: 'settings-file' } | null {
  return n.key === 'config' ? { label: 'Open config.toml', opens: 'settings-file' } : null;
}

// ---- Notices a window reports itself ----

let reported: Notice[] = [];
const listeners = new Set<() => void>();

function publish(next: Notice[]) {
  reported = next;
  listeners.forEach((tell) => tell());
}

/** Says something in the background isn't working, until `clearNotice(key)`: something that
 *  failed without your asking for it, which a toast would say once and lose. One for a key. */
export function reportNotice(n: Notice): void {
  const at = reported.findIndex((r) => r.key === n.key);
  if (at >= 0 && noticeId(reported[at]) === noticeId(n) && reported[at].text === n.text && reported[at].until === n.until) return;
  publish(at >= 0 ? reported.map((r, i) => (i === at ? n : r)) : [...reported, n]);
}

export function clearNotice(key: string): void {
  if (reported.some((r) => r.key === key)) publish(reported.filter((r) => r.key !== key));
}

/** The notices this window reported, for showing with the board's. */
export function useReportedNotices(): Notice[] {
  return useSyncExternalStore(
    (tell) => {
      listeners.add(tell);
      return () => listeners.delete(tell);
    },
    () => reported,
  );
}
