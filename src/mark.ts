// The mark's motion on the bar: the C holds still and the node, which carries the status color,
// is the part that moves. It breathes while agents work (styles alone, from the status), and a
// moment plays once when something happens:
//
//   run     a session shows for the first time, working: the node's color travels once round the C
//   send    you sent a session a message from Codealong: a dot leaves the stem
//   ripple  a turn ended: the node beats, a ring crosses the gap and the C takes it
//   knock   a session, or a PR's review, starts to need you: the node beats twice
//
// What happened is read from two feeds in a row, by the pills that color the mark (ahead.ts,
// `logoOfBar`). MarkLive.tsx plays the moment; app.css has the motion.

import { useEffect, useRef, useState } from 'react';
import type { Ahead } from './ahead';
import type { BarFeed } from './types';

/** The least urgent first: one that comes while another plays takes over only if it's more
 *  urgent. */
export const MOMENTS = ['run', 'send', 'ripple', 'knock'] as const;
export type MomentKind = (typeof MOMENTS)[number];

/** How long each plays, as app.css has it. */
export const MOMENT_MS: Record<MomentKind, number> = { run: 1500, send: 730, ripple: 950, knock: 1440 };
/** The quiet after one before a no more urgent one plays, so a burst of sessions doesn't make
 *  the mark flicker. */
const REST_MS = 400;

export interface Moment {
  kind: MomentKind;
  /** When it began: the mark plays it only while it's new, not again when it's drawn anew. */
  at: number;
}

/** The sessions and PRs a feed has, by what they'd set off. */
export interface Marked {
  needs: Set<string>;
  unread: Set<string>;
  /** Every session the bar knows of, in a group or not. */
  known: Set<string>;
}

export function marked(feed: BarFeed): Marked {
  const needs = new Set<string>();
  const unread = new Set<string>();
  const known = new Set<string>();
  for (const i of feed.items) {
    known.add(i.key);
    if (i.hidden) continue;
    if (i.group === 'needs') needs.add(i.key);
    if (i.group === 'unread') unread.add(i.key);
  }
  for (const r of feed.recent) known.add(r.key);
  for (const s of feed.snoozed ?? []) known.add(s.key);
  for (const r of feed.reviews) needs.add(`review:${r.key}`);
  return { needs, unread, known };
}

/** What happened between two feeds, the most urgent of it; null when nothing did. */
export function happened(was: Marked, feed: BarFeed): MomentKind | null {
  const now = marked(feed);
  const came = (a: Set<string>, b: Set<string>) => [...a].some((k) => !b.has(k));
  if (came(now.needs, was.needs)) return 'knock';
  if (came(now.unread, was.unread)) return 'ripple';
  if (feed.items.some((i) => !i.hidden && i.group === 'working' && !was.known.has(i.key))) return 'run';
  return null;
}

/** Whether a moment plays, given the last one that did. */
export function plays(next: MomentKind, last: Moment | null, now: number): boolean {
  if (!last || now - last.at >= MOMENT_MS[last.kind] + REST_MS) return true;
  return MOMENTS.indexOf(next) > MOMENTS.indexOf(last.kind);
}

/** The moment the bar's mark plays, from its feed (with what's shown ahead of the engine) and
 *  the messages on their way. Nothing plays for the first feed, or while the engine is loading:
 *  what's there at launch isn't news. */
export function useMarkMoment(feed: BarFeed | null, ahead: Ahead[]): Moment | null {
  const [moment, setMoment] = useState<Moment | null>(null);
  const was = useRef<Marked | null>(null);
  const sends = useRef<Set<string> | null>(null);
  const last = useRef<Moment | null>(null);
  useEffect(() => {
    const ids = new Set(ahead.filter((a) => a.change.kind === 'send').map((a) => a.id));
    const sent = sends.current !== null && [...ids].some((id) => !sends.current?.has(id));
    sends.current = ids;
    let kind: MomentKind | null = sent ? 'send' : null;
    if (feed && !feed.loading) {
      const news = was.current ? happened(was.current, feed) : null;
      was.current = marked(feed);
      if (news && (!kind || MOMENTS.indexOf(news) > MOMENTS.indexOf(kind))) kind = news;
    }
    const now = Date.now();
    if (!kind || !plays(kind, last.current, now)) return;
    last.current = { kind, at: now };
    setMoment(last.current);
  }, [feed, ahead]);
  return moment;
}
