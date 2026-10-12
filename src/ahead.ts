// Changes you make that the engine takes a moment to show: sessions marked seen or unread, snoozed,
// quieted, archived, renamed, sent a message or let go on past a call they waited on, reviews asked
// of you snoozed, and PRs reviewed or merged.
// The engine takes them in its own loop, which waits behind scans and git on a
// busy Mac (half a second typically, seconds at worst), and only then does every window get a
// board that shows them. So each window shows a change at once, on top of the board it has, and
// tells the other windows (the `ahead` event): the bar's pill goes as you mark its session seen in
// the peek, and the main window shows the session working as you send it something from the bar.
//
// A seen, unread, snooze, quiet, archive, rename or merge goes once a board at least as new as the one the engine says
// shows it arrives. Going by whether the board looks right instead would hide a session that turned unread
// again a moment later. One the engine turns down goes at once, and the caller says why.
//
// A merge shows as you confirm it, before GitHub has done it, and goes back with why if GitHub
// refuses it; once it has, the engine shows the PR merged at once (ghpoll.rs `on_merged`). A
// review you submit takes the PR out of what waits on your review the same way, any verdict, as
// GitHub and the engine do (ghpoll.rs `on_approved`).
//
// An answer that lets a session go on (a call allowed, questions answered, a deny with words for
// it to go on with) shows it working as you give it, until the board shows it no longer waits on
// that call: the engine has no board for it, since the session's harness says it went on only
// once it has. After a plain deny some agents end the turn and others carry on, which only the
// engine knows, so that waits.
//
// A message waits for any other on its way to the same session, so they arrive in order. The box
// clears as you send, the thread shows the message as sending, then sent, until the session's
// record has it (SessionThread.tsx), and the session shows as working meanwhile. A message that
// doesn't go is back in the box with why, with any sent after it. One that went but that nothing
// shows the session took says so in the thread.

import { useSyncExternalStore } from 'react';
import { archiveSession, emitAhead, markSeen, markUnread, mergePr, onAhead, quietSessions, renameSession, resumeSession, reviewPr, sendToSession, sessionAnswer, sessionDone, snoozeReviews, snoozeSessions, windowLabel, type Done } from './api';
import { readDraft, writeDraft } from './drafts';
import { nameShown } from './harness';
import { heldForAgent } from './prStatus';
import { relaned, sessionHeadline, unreadable } from './lanes';
import type { Answers, BarFeed, BarItem, BarRecent, BarSnoozed, Board, Lane, LogoStatus, MergeOp, PrItem, ResumeIn, Reviewed, Sent, Session, Status, TranscriptItem, Verdict } from './types';

/** A message the board shows no sign of this long after it went stops showing its session as
 *  working; if the session isn't working either, the thread says it may not have reached it. */
export const CONFIRM_MS = 20_000;
/** A message the board shows the session took stays this long, for its thread to read it from
 *  the session's record. */
const TAKEN_MS = 5_000;
/** A prompt a little before the send still counts as it: another device's clock may be behind. */
export const SLACK_MS = 1_000;
/** A seen, archive or rename the engine gave no board for (a session on another device) goes
 *  when the board shows it, or after this. */
const UNANSWERED_MS = 20_000;
/** An answer the board shows nothing of goes this long after the engine took it: the call may
 *  have been followed by another the board can't tell from it, which mustn't stay hidden. */
const ANSWER_MS = 8_000;
/** A seen, archive or rename goes after this, whatever the board says. Mark as done waits less
 *  than this for a session's harness to remove its worktree (idle.rs `REMOVAL_WAIT`). */
const LONGEST_MS = 60_000;
/** A message goes after this, whatever the board says: its thread has said what it knows. */
const SEND_LONGEST_MS = 10 * 60_000;

export type Change =
  | { kind: 'seen'; keys: string[] }
  /** Marked unread: Your turn again, from when it was marked. */
  | { kind: 'unread'; keys: string[] }
  /** Snoozed until then, or woken now with null. */
  | { kind: 'snooze'; keys: string[]; until: number | null }
  /** Quieted, or made heard again with false: a quiet one's finished turn is idle. */
  | { kind: 'quiet'; keys: string[]; quiet: boolean }
  /** Your review of these PRs (by PR key) snoozed until then, or woken now with null. */
  | { kind: 'review-snooze'; keys: string[]; until: number | null }
  /** A PR (by its key) merged: merged from when it was made, and waiting on no review. */
  | { kind: 'pr-merge'; key: string }
  /** Your review of a PR (by its key) submitted: it waits on your review no more. */
  | { kind: 'pr-review'; key: string }
  | { kind: 'archive'; key: string; archived: boolean }
  /** Named, as the board will show it. */
  | { kind: 'rename'; key: string; title: string }
  /** The call it waited on answered so that it goes on: working from when it was answered.
   *  `since` is when it began waiting on that call, as the board had it. */
  | { kind: 'answer'; key: string; since: number | null }
  | {
      kind: 'send';
      key: string;
      text: string;
      /** The session was working as it was sent, so it takes the message in as it works. */
      working: boolean;
      /** The message reaches the session itself, rather than opening its app with the text to
       *  paste, so it starts a turn. */
      starts: boolean;
    };

export interface Ahead {
  id: string;
  change: Change;
  /** When it was made. */
  at: number;
  /** When the engine answered; null while the change is on its way. */
  doneAt: number | null;
  /** The time of the first board that shows it, when the engine said. */
  shownAt: number | null;
  // A message, as this window reads the board. Each window reads its own; other windows don't hear.
  /** When the board first showed the session took it. */
  takenAt?: number;
  /** CONFIRM_MS went by with no sign of it: the session shows as the board has it. */
  lapsed?: boolean;
  /** Lapsed, and the session isn't working either. */
  unconfirmed?: boolean;
}

type SendChange = Extract<Change, { kind: 'send' }>;

/** A session as a window shows it, before its changes: what settling a change looks at. */
export interface Shown {
  title: string;
  status: Status;
  statusSince: number | null;
  /** Unknown to the bar. */
  lastPromptAt: number | null;
  archived: boolean;
  asks: boolean;
  snoozedUntil: number | null;
  quiet: boolean;
}

/** What a window shows: the time of its board (null with none yet) and its sessions. */
export interface View {
  boardAt: number | null;
  find: (key: string) => Shown | undefined;
}

/* ---- What the changes do to what a window shows ---- */

/** Marking it seen settles it: a finished turn, or a reply that asks you something. */
function settledBySeen(s: { status: Status; asks?: boolean }): boolean {
  return s.status === 'unread' || (s.status === 'needs' && !!s.asks);
}

/** Whether it still shows waiting on the call an answer was for: a call asked after it began
 *  waiting is another, which the answer doesn't touch. A session that needs you does so from when
 *  it asked what it asks now (acp.rs `overlay`, opencode.rs `Asked::since`, Codex's oldest call,
 *  Claude Code's request or registry), so the next call shows by its time; where a harness can't
 *  say, `ANSWER_MS` keeps it from staying hidden long. */
function stillAsked(s: { status: Status; asks?: boolean }, since: number | null, c: Extract<Change, { kind: 'answer' }>): boolean {
  return s.status === 'needs' && !s.asks && !(since != null && c.since != null && since > c.since);
}

/** A message that's still showing its session as working. */
function startsNow(a: Ahead): boolean {
  const c = a.change;
  return c.kind === 'send' && c.starts && !c.working && a.takenAt == null && !a.lapsed;
}

/** Whether a message reaches the session itself, as `replyPlaceholder` (SessionReply.tsx) says:
 *  the Antigravity and Cursor apps, and a Claude app session that isn't running, open with it on
 *  the clipboard. */
export function startsTurn(s: Session): boolean {
  if ((s.harness === 'antigravity' || s.harness === 'cursor') && s.surface === 'app') return false;
  return !(!s.live && s.harness === 'claude' && s.surface === 'app');
}

function byKey(changes: Ahead[]): Map<string, Ahead[]> {
  const m = new Map<string, Ahead[]>();
  const add = (k: string, a: Ahead) => m.set(k, [...(m.get(k) ?? []), a]);
  for (const a of changes) {
    // A review's snooze, a review and a merge are a PR's, not a session's.
    if (a.change.kind === 'review-snooze' || a.change.kind === 'pr-merge' || a.change.kind === 'pr-review') continue;
    if (a.change.kind === 'seen' || a.change.kind === 'unread' || a.change.kind === 'snooze' || a.change.kind === 'quiet') for (const k of a.change.keys) add(k, a);
    else add(a.change.key, a);
  }
  return m;
}

/** A session with this window's changes to it. */
export function sessionAhead(s: Session, changes: Ahead[] | undefined): Session {
  if (!changes) return s;
  let out = s;
  for (const a of changes) {
    const c = a.change;
    if (c.kind === 'seen' && settledBySeen(out)) out = { ...out, status: 'idle', asks: false };
    else if (c.kind === 'unread' && unreadable(out)) out = { ...out, status: 'unread', statusSince: a.at };
    else if (c.kind === 'archive' && out.archived !== c.archived) out = { ...out, archived: c.archived };
    else if (c.kind === 'rename' && out.title !== c.title) out = { ...out, title: c.title };
    else if (c.kind === 'answer' && stillAsked(out, out.statusSince, c)) out = { ...out, status: 'working', statusSince: a.at, stalledSince: null };
    // Quieted, a finished turn is idle (board.rs, `apply_quiet`). Made heard again, one you
    // haven't seen is Your turn again, which only the engine knows: the board brings it.
    else if (c.kind === 'quiet' && !!out.quiet !== c.quiet) out = { ...out, quiet: c.quiet, ...(c.quiet && out.status === 'unread' ? { status: 'idle' as const } : {}) };
    else if (c.kind === 'snooze' && !out.archived) {
      if (c.until != null) {
        if (out.snoozedUntil !== c.until) out = { ...out, snoozedUntil: c.until };
      } else if (out.snoozedUntil) {
        // Woken, one with nothing to show for itself is Your turn (board.rs, `apply_snooze`).
        const back = out.status === 'idle' || out.status === 'stopped';
        out = { ...out, snoozedUntil: undefined, ...(back ? { status: 'unread' as const, statusSince: a.at } : {}) };
      }
    }
    else if (c.kind === 'send' && startsNow(a) && out.status !== 'working') {
      // A prompt sent to a snoozed session ends its snooze: you took it up (board.rs, `apply_snooze`).
      out = { ...out, status: 'working', statusSince: a.at, stalledSince: null, asks: false, snoozedUntil: undefined, lastPromptAt: Math.max(out.lastPromptAt, a.at) };
    }
  }
  if (out === s) return s;
  // The headline names the session, after what it's doing.
  if (out.status === s.status && out.title === s.title) return out;
  const [headline, detail] = sessionHeadline(out);
  return { ...out, headline, detail };
}

/** The lanes with this window's changes to their sessions, each lane's bay and sentence worked
 *  out again. The same array when nothing changes. */
export function lanesAhead(lanes: Lane[], changes: Ahead[]): Lane[] {
  if (changes.length === 0) return lanes;
  const by = byKey(changes);
  let changed = false;
  const out = lanes.map((l) => {
    if (!l.sessions.some((s) => by.has(s.key))) return l;
    const sessions = l.sessions.map((s) => sessionAhead(s, by.get(s.key)));
    if (sessions.every((s, i) => s === l.sessions[i])) return l;
    changed = true;
    // The lane's status and sentence come from its sessions' statuses and headlines; a snooze
    // alone leaves them.
    const moved = sessions.some((s, i) => s.status !== l.sessions[i].status || s.archived !== l.sessions[i].archived || s.title !== l.sessions[i].title);
    return moved ? relaned(l, sessions) : { ...l, sessions };
  });
  return changed ? out : lanes;
}

const LOGO_RANK: LogoStatus[] = ['error', 'needs', 'unread', 'working', 'idle'];

/** A session's color on the mark: a snoozed one only while it works. */
function logoColor(s: { status: Status; snoozedUntil?: number | null }): LogoStatus {
  if (s.status === 'working') return 'working';
  return s.snoozedUntil || s.status === 'stopped' ? 'idle' : s.status;
}

/** What the mark's colors say, as the engine works it out (board.rs, `logo_status`). */
export function logoOf(lanes: Lane[], prs: PrItem[]): LogoStatus {
  let best = LOGO_RANK.length - 1;
  const see = (st: LogoStatus) => (best = Math.min(best, LOGO_RANK.indexOf(st)));
  for (const l of lanes) {
    if (l.hidden) continue;
    for (const s of l.sessions) if (!s.archived && !s.visitor) see(logoColor(s));
  }
  if (prs.some((p) => p.reviewRequested && !p.snoozedUntil && !heldForAgent(p) && p.state !== 'merged' && p.state !== 'closed')) see('needs');
  return LOGO_RANK[best];
}

/** The last snooze or wake of each review this window's changes snooze or wake: a time, or null
 *  to wake. */
function reviewSnoozes(changes: Ahead[]): Map<string, number | null> {
  const until = new Map<string, number | null>();
  for (const a of changes) if (a.change.kind === 'review-snooze') for (const k of a.change.keys) until.set(k, a.change.until);
  return until;
}

/** The PRs this window's changes review, by PR key. */
function reviewed(changes: Ahead[]): Set<string> {
  const keys = new Set<string>();
  for (const a of changes) if (a.change.kind === 'pr-review') keys.add(a.change.key);
  return keys;
}

/** When each PR this window's changes merge was merged, by PR key. */
function merges(changes: Ahead[]): Map<string, number> {
  const at = new Map<string, number>();
  for (const a of changes) if (a.change.kind === 'pr-merge') at.set(a.change.key, a.at);
  return at;
}

/** The PRs with this window's changes: snoozes of reviews, on the ones that wait on your review,
 *  as board.rs `apply_review_snoozes` puts them, reviews, as ghpoll.rs `on_approved` takes them
 *  out of what waits on yours, and merges, as ghpoll.rs `shown_merged` does. The same array when
 *  nothing changes. */
export function prsAhead(prs: PrItem[], changes: Ahead[]): PrItem[] {
  const until = reviewSnoozes(changes);
  const merged = merges(changes);
  const done = reviewed(changes);
  if (until.size === 0 && merged.size === 0 && done.size === 0) return prs;
  let changed = false;
  const out = prs.map((p) => {
    let next = p;
    const at = merged.get(p.key);
    if (at !== undefined && p.state !== 'merged' && p.state !== 'closed') next = { ...next, state: 'merged', mergedAt: at, closedAt: at, reviewRequested: false, snoozedUntil: null };
    if (done.has(p.key) && next.reviewRequested) next = { ...next, reviewRequested: false };
    if (until.has(p.key) && next.reviewRequested && next.state !== 'merged' && next.state !== 'closed') {
      const snooze = until.get(p.key) ?? null;
      if ((next.snoozedUntil ?? null) !== snooze) next = { ...next, snoozedUntil: snooze };
    }
    if (next !== p) changed = true;
    return next;
  });
  return changed ? out : prs;
}

/** The board with this window's changes. The same board when nothing changes. */
export function boardAhead(board: Board, changes: Ahead[]): Board {
  const lanes = lanesAhead(board.lanes, changes);
  const prs = prsAhead(board.prs, changes);
  return lanes === board.lanes && prs === board.prs ? board : { ...board, lanes, prs, logoStatus: logoOf(lanes, prs) };
}

const GROUP_RANK = ['needs', 'unread', 'working'];

/** The floating bar's feed with this window's changes: pills marked seen or archived go, a
 *  session sent a message moves to Working (out of the snoozed ones too, as sending ends a
 *  snooze), as does one whose call was answered so it goes on, and a renamed one takes its name.
 *  The same feed when nothing changes. */
export function barAhead(feed: BarFeed, changes: Ahead[]): BarFeed {
  if (changes.length === 0) return feed;
  return barReviews(barMoved(barRenamed(feed, changes), changes), changes);
}

/** The bar's feed without the reviews snoozed, given or merged here. One woken comes back with
 *  the engine's next feed, which has what the pill's menu says of it. The same feed when nothing
 *  changes. */
function barReviews(feed: BarFeed, changes: Ahead[]): BarFeed {
  const until = reviewSnoozes(changes);
  const merged = merges(changes);
  const done = reviewed(changes);
  if (until.size === 0 && merged.size === 0 && done.size === 0) return feed;
  const reviews = feed.reviews.filter((r) => until.get(r.key) == null && !merged.has(r.key) && !done.has(r.key));
  if (reviews.length === feed.reviews.length) return feed;
  return { ...feed, reviews, logoStatus: logoOfBar(feed.items, reviews.length > 0) };
}

/** The bar's feed with renamed sessions' names. The same feed when nothing changes. */
function barRenamed(feed: BarFeed, changes: Ahead[]): BarFeed {
  const names = new Map<string, string>();
  for (const a of changes) if (a.change.kind === 'rename') names.set(a.change.key, a.change.title);
  if (names.size === 0) return feed;
  let changed = false;
  const retitle = <T extends { key: string; title: string }>(list: T[]): T[] => {
    let hit = false;
    const out = list.map((i) => {
      const title = names.get(i.key);
      if (title === undefined || title === i.title) return i;
      hit = true;
      return { ...i, title };
    });
    if (hit) changed = true;
    return hit ? out : list;
  };
  const next = { ...feed, items: retitle(feed.items), recent: retitle(feed.recent), snoozed: feed.snoozed && retitle(feed.snoozed) };
  return changed ? next : feed;
}

/** The bar's feed with pills gone, moved or snoozed. The same feed when nothing changes. */
function barMoved(feed: BarFeed, changes: Ahead[]): BarFeed {
  const by = byKey(changes);
  const archived = (key: string) => (by.get(key) ?? []).some((a) => a.change.kind === 'archive' && a.change.archived);
  const moved = (key: string) => (by.get(key) ?? []).find(startsNow);
  /** The answer that lets a pill waiting on a call go on, if one does. */
  const goesOn = (i: Pick<BarItem, 'key' | 'status' | 'asks' | 'since'>) => (by.get(i.key) ?? []).find((a) => a.change.kind === 'answer' && stillAsked(i, i.since, a.change));
  /** The last snooze or wake for a session: a time, null to wake, undefined with none. */
  const snooze = (key: string): number | null | undefined => {
    const last = (by.get(key) ?? []).filter((a) => a.change.kind === 'snooze').pop()?.change;
    return last?.kind === 'snooze' ? last.until : undefined;
  };
  /** Quieted by the last quiet change for it: a finished turn of its leaves Your turn. */
  const quieted = (key: string): boolean => {
    const last = (by.get(key) ?? []).filter((a) => a.change.kind === 'quiet').pop()?.change;
    return last?.kind === 'quiet' && last.quiet;
  };
  const snoozing: BarSnoozed[] = [];
  let changed = false;
  const items: BarItem[] = [];
  for (const i of feed.items) {
    const seen = (by.get(i.key) ?? []).some((a) => a.change.kind === 'seen') && settledBySeen(i);
    if (archived(i.key) || seen || (i.status === 'unread' && quieted(i.key))) {
      changed = true;
      continue;
    }
    const m = moved(i.key) ?? goesOn(i);
    const until = snooze(i.key);
    if (m && i.group !== 'working') {
      changed = true;
      items.push({ ...i, group: 'working', status: 'working', asks: false, since: m.at });
    } else if (until != null && i.group !== 'working') {
      changed = true;
      snoozing.push({ ...i, group: 'snoozed', snoozedUntil: until });
    } else items.push(i);
  }
  const leave = <T extends BarRecent | BarSnoozed>(list: T[]): T[] =>
    list.filter((r) => {
      const m = moved(r.key);
      const answer = goesOn(r);
      const until = snooze(r.key);
      const pill = { ...r, snoozedUntil: null } as unknown as BarItem;
      let gone = archived(r.key);
      if (!gone && m) {
        items.push({ ...pill, group: 'working', status: 'working', asks: false, since: m.at });
        gone = true;
      } else if (!gone && answer) {
        // Let go on, it works through its snooze, as barfeed.rs puts a working one.
        items.push({ ...(r as unknown as BarItem), group: 'working', status: 'working', asks: false, since: answer.at });
        gone = true;
      } else if (!gone && until != null && (r.group === 'recent' || r.snoozedUntil !== until)) {
        snoozing.push({ ...r, group: 'snoozed', snoozedUntil: until } as BarSnoozed);
        gone = true;
      } else if (!gone && until === null && r.group === 'snoozed') {
        // Woken, it's back where its status puts it; one with nothing to show for itself is Your turn.
        const needs = r.status === 'needs' || r.status === 'error';
        items.push({ ...pill, group: needs ? 'needs' : 'unread', status: needs ? r.status : 'unread' });
        gone = true;
      }
      if (gone) changed = true;
      return !gone;
    });
  const recent = leave(feed.recent);
  const snoozed = [...leave(feed.snoozed ?? []), ...snoozing].sort((a, b) => a.snoozedUntil - b.snoozedUntil || a.key.localeCompare(b.key));
  if (!changed) return feed;
  // As barfeed.rs orders them: by group, each a queue, the longest first.
  items.sort((a, b) => GROUP_RANK.indexOf(a.group) - GROUP_RANK.indexOf(b.group) || a.since - b.since || a.key.localeCompare(b.key));
  const logoStatus = logoOfBar(items, feed.reviews.length > 0);
  return { ...feed, items, recent, snoozed, logoStatus };
}

/** The mark's colors from the bar's pills, which are the sessions that color it. */
function logoOfBar(items: BarItem[], reviews: boolean): LogoStatus {
  let best = LOGO_RANK.length - 1;
  for (const i of items) if (!i.hidden) best = Math.min(best, LOGO_RANK.indexOf(logoColor(i)));
  if (reviews) best = Math.min(best, LOGO_RANK.indexOf('needs'));
  return LOGO_RANK[best];
}

/** A window's view of the board, for settling its changes. */
export function boardView(boardAt: number | null, laneLists: Lane[][]): View {
  return {
    boardAt,
    find: (key) => {
      for (const lanes of laneLists) {
        for (const l of lanes) {
          const s = l.sessions.find((x) => x.key === key && !x.visitor);
          if (s) return { title: s.title, status: s.status, statusSince: s.statusSince, lastPromptAt: s.lastPromptAt, archived: s.archived, asks: !!s.asks, snoozedUntil: s.snoozedUntil ?? null, quiet: !!s.quiet };
        }
      }
      return undefined;
    },
  };
}

/** The bar's view: its pills, and the recent sessions. */
export function barView(feed: BarFeed): View {
  return {
    boardAt: feed.generatedAt ?? null,
    find: (key) => {
      const i = feed.items.find((x) => x.key === key) ?? feed.recent.find((x) => x.key === key) ?? (feed.snoozed ?? []).find((x) => x.key === key);
      return i && { title: i.title, status: i.status, statusSince: i.since, lastPromptAt: null, archived: false, asks: !!i.asks, snoozedUntil: i.snoozedUntil ?? null, quiet: !!i.quiet };
    },
  };
}

/* ---- When a change can go ---- */

/** Whether the board shows the session took the message. */
function took(s: Shown, a: Ahead, c: SendChange): boolean {
  const from = a.at - SLACK_MS;
  if (s.lastPromptAt != null && s.lastPromptAt >= from) return true;
  // A session that wasn't working has done something since: the message started it.
  return !c.working && s.statusSince != null && s.statusSince >= from;
}

/** The change as `v` leaves it at `now`: the same, read further (a message), or null once it
 *  can go. */
export function settled(a: Ahead, v: View, now: number): Ahead | null {
  const c = a.change;
  if (c.kind !== 'send') {
    if (now - a.at > LONGEST_MS) return null;
    if (a.doneAt == null) return a;
    if (a.shownAt != null) return v.boardAt != null && v.boardAt >= a.shownAt ? null : a;
    if (now - a.doneAt > (c.kind === 'answer' ? ANSWER_MS : UNANSWERED_MS)) return null;
    // A window's view finds sessions, not PRs: a review's snooze, a review and a merge go by the
    // board's time alone.
    if (c.kind === 'review-snooze' || c.kind === 'pr-merge' || c.kind === 'pr-review') return a;
    const keys = c.kind === 'archive' || c.kind === 'rename' || c.kind === 'answer' ? [c.key] : c.keys;
    const shows = keys.every((k) => {
      const s = v.find(k);
      if (s === undefined) return false;
      if (c.kind === 'answer') return !stillAsked(s, s.statusSince, c);
      if (c.kind === 'seen') return !settledBySeen(s);
      if (c.kind === 'unread') return !unreadable(s);
      if (c.kind === 'snooze') return s.snoozedUntil === c.until;
      if (c.kind === 'quiet') return s.quiet === c.quiet;
      if (c.kind === 'rename') return s.title === c.title;
      return s.archived === c.archived;
    });
    return shows ? null : a;
  }
  if (a.doneAt == null) return a;
  if (now - a.at > SEND_LONGEST_MS) return null;
  const s = v.find(c.key);
  let next = a;
  if (next.takenAt == null && s && took(s, a, c)) next = { ...next, takenAt: now, lapsed: false, unconfirmed: false };
  if (next.takenAt != null) return now - next.takenAt > TAKEN_MS ? null : next;
  if (!next.lapsed && now - a.doneAt >= CONFIRM_MS) next = { ...next, lapsed: true };
  if (next.lapsed) {
    const quiet = s !== undefined && s.status !== 'working' && now - Math.max(a.doneAt, s.statusSince ?? 0) >= CONFIRM_MS;
    if (quiet !== !!next.unconfirmed) next = { ...next, unconfirmed: quiet };
  }
  return next;
}

/**
 * The messages the thread should still show, and those its items have: a prompt of yours (or the
 * note a message from another session leaves) from about when each was sent, taken in order.
 */
export function inRecord(sends: Ahead[], items: TranscriptItem[]): { showing: Ahead[]; found: string[] } {
  const said = items.filter((it) => it.kind === 'you' || (it.kind === 'note' && it.text.startsWith('From another session')));
  const used = new Set<number>();
  const showing: Ahead[] = [];
  const found: string[] = [];
  for (const a of [...sends].sort((x, y) => x.at - y.at)) {
    const i = said.findIndex((it, j) => !used.has(j) && it.at >= a.at - SLACK_MS);
    if (i >= 0) {
      used.add(i);
      found.push(a.id);
    } else showing.push(a);
  }
  return { showing, found };
}

/* ---- This window's changes ---- */

type Event = { from: string; put?: Ahead; drop?: string };

const me = windowLabel() ?? 'page';
let all: Ahead[] = [];
let view: View = { boardAt: null, find: () => undefined };
const listeners = new Set<() => void>();
let seq = 0;
let timer: number | null = null;
let hearing = false;

function publish(next: Ahead[]): void {
  if (next === all) return;
  all = next;
  for (const l of listeners) l();
  // Changes go with time too, so the clock runs while there are any.
  if (all.length > 0 && timer == null && typeof window !== 'undefined') timer = window.setInterval(() => settle(), 1000);
  if (all.length === 0 && timer != null) {
    window.clearInterval(timer);
    timer = null;
  }
}

/** Only what every window shares: each reads the board for itself. */
function shared(a: Ahead): Ahead {
  return { id: a.id, change: a.change, at: a.at, doneAt: a.doneAt, shownAt: a.shownAt };
}

function put(a: Ahead, tell: boolean): void {
  const i = all.findIndex((x) => x.id === a.id);
  const kept = i < 0 ? a : { ...all[i], ...shared(a) };
  publish(i < 0 ? [...all, kept] : all.map((x, j) => (j === i ? kept : x)));
  settle();
  if (tell) emitAhead<Event>({ from: me, put: shared(a) });
}

function drop(id: string, tell: boolean): void {
  if (all.some((x) => x.id === id)) publish(all.filter((x) => x.id !== id));
  if (tell) emitAhead<Event>({ from: me, drop: id });
}

function settle(now = Date.now()): void {
  let changed = false;
  const next: Ahead[] = [];
  for (const a of all) {
    const s = settled(a, view, now);
    if (s !== a) changed = true;
    if (s) next.push(s);
  }
  if (changed) publish(next);
}

function subscribe(cb: () => void): () => void {
  if (!hearing) {
    hearing = true;
    onAhead<Event>((e) => {
      if (e.from === me) return;
      if (e.put) put(e.put, false);
      if (e.drop) drop(e.drop, false);
    });
  }
  listeners.add(cb);
  return () => listeners.delete(cb);
}

const snapshot = () => all;

/** Every change shown ahead of the engine, this window's and the others'. */
export function useAhead(): Ahead[] {
  return useSyncExternalStore(subscribe, snapshot, snapshot);
}

/** What the window shows now, as the engine has it: its changes go as boards show them. */
export function observe(v: View): void {
  view = v;
  settle();
}

/** A message the session's record has now: its thread shows it from there. */
export function delivered(id: string): void {
  drop(id, true);
}

/** A message the thread said it can't see reaching the session, put away. */
export function dismiss(id: string): void {
  drop(id, true);
}

function made(change: Change): Ahead {
  return { id: `${me}:${Date.now().toString(36)}:${++seq}`, change, at: Date.now(), doneAt: null, shownAt: null };
}

/** The engine answered: the change now waits for a board this new, or for one that shows it. */
function answered(id: string, shownAt: number | null): void {
  const a = all.find((x) => x.id === id);
  if (a) put({ ...a, doneAt: Date.now(), shownAt }, true);
}

/** Marks sessions seen, in every window at once. Rejects with why when the engine can't. */
/** Marks sessions unread, showing it at once (`markUnread`). */
export function unreadAhead(keys: string[]): Promise<void> {
  if (keys.length === 0) return Promise.resolve();
  const a = made({ kind: 'unread', keys });
  put(a, true);
  return markUnread(keys).then(
    (shownAt) => answered(a.id, shownAt),
    (e: unknown) => {
      drop(a.id, true);
      throw e;
    },
  );
}

export function seenAhead(keys: string[]): Promise<void> {
  if (keys.length === 0) return Promise.resolve();
  const a = made({ kind: 'seen', keys });
  put(a, true);
  return markSeen(keys).then(
    (shownAt) => answered(a.id, shownAt),
    (e: unknown) => {
      drop(a.id, true);
      throw e;
    },
  );
}

/** Snoozes sessions until `until`, or wakes them now with null, in every window at once. */
export function snoozeAhead(keys: string[], until: number | null): Promise<void> {
  if (keys.length === 0) return Promise.resolve();
  const a = made({ kind: 'snooze', keys, until });
  put(a, true);
  return snoozeSessions(keys, until).then(
    (shownAt) => answered(a.id, shownAt),
    (e: unknown) => {
      drop(a.id, true);
      throw e;
    },
  );
}

/** Quiets sessions, or makes them heard again with false, in every window at once. */
export function quietAhead(keys: string[], quiet: boolean): Promise<void> {
  if (keys.length === 0) return Promise.resolve();
  const a = made({ kind: 'quiet', keys, quiet });
  put(a, true);
  return quietSessions(keys, quiet).then(
    (shownAt) => answered(a.id, shownAt),
    (e: unknown) => {
      drop(a.id, true);
      throw e;
    },
  );
}

/** Snoozes your review of PRs (by PR key) until `until`, or wakes them now with null, in every
 *  window at once. */
export function snoozeReviewsAhead(keys: string[], until: number | null): Promise<void> {
  if (keys.length === 0) return Promise.resolve();
  const a = made({ kind: 'review-snooze', keys, until });
  put(a, true);
  return snoozeReviews(keys, until).then(
    (shownAt) => answered(a.id, shownAt),
    (e: unknown) => {
      drop(a.id, true);
      throw e;
    },
  );
}

/** Does what a PR's merge box asked (MergeBox.tsx). A merge shows in every window as it's asked,
 *  the PR merged and out of what waits on your review, and goes back with why when GitHub refuses
 *  it; what else the box does waits on GitHub. Resolves with what GitHub couldn't do after (a
 *  branch that stayed). */
export function mergeAhead(pr: Pick<PrItem, 'key' | 'repoSlug' | 'number'>, id: string, op: MergeOp): Promise<string | null> {
  if (op.op !== 'merge') return mergePr(pr.repoSlug, pr.number, id, op).then((d) => d.note);
  const a = made({ kind: 'pr-merge', key: pr.key });
  put(a, true);
  return mergePr(pr.repoSlug, pr.number, id, op).then(
    (d) => {
      answered(a.id, d.shownAt);
      return d.note;
    },
    (e: unknown) => {
      drop(a.id, true);
      throw e;
    },
  );
}

/** Reviews a PR as you (`reviewPr`): it's out of what waits on your review in every window as
 *  it's asked, and back, with why, if GitHub refuses it. The engine takes it out as GitHub has it
 *  (ghpoll.rs `on_approved`) and shows that before it answers, so the change goes a while after. */
export function reviewAhead(pr: Pick<PrItem, 'key' | 'repoSlug' | 'number'>, verdict: Verdict, body: string, agents = false): Promise<Reviewed> {
  const a = made({ kind: 'pr-review', key: pr.key });
  put(a, true);
  return reviewPr(pr.repoSlug, pr.number, verdict, body, agents).then(
    (done) => {
      answered(a.id, null);
      return done;
    },
    (e: unknown) => {
      drop(a.id, true);
      throw e;
    },
  );
}

/** Archives or unarchives a session, in every window at once. Rejects with why when it can't:
 *  Codex didn't archive the thread, say. */
export function archiveAhead(key: string, archived: boolean): Promise<void> {
  const a = made({ kind: 'archive', key, archived });
  put(a, true);
  return archiveSession(key, archived).then(
    (shownAt) => answered(a.id, shownAt),
    (e: unknown) => {
      drop(a.id, true);
      throw e;
    },
  );
}

/** Done with a session: marked seen and archived in every window at once, as the engine ends
 *  it (actions.rs `done`). One the engine held for work under way comes back, and so does one it
 *  couldn't do, which rejects with why. With `after`, the engine is asked once that settles:
 *  several done together all show at once, and their terminals close one after another. */
export function doneAhead(key: string, stop: boolean, after?: Promise<unknown>): Promise<Done> {
  const seen = made({ kind: 'seen', keys: [key] });
  const archived = made({ kind: 'archive', key, archived: true });
  put(seen, true);
  put(archived, true);
  const back = () => {
    drop(seen.id, true);
    drop(archived.id, true);
  };
  const asked = after ? after.then(() => sessionDone(key, stop), () => sessionDone(key, stop)) : sessionDone(key, stop);
  return asked.then(
    (r) => {
      if (r.done) {
        answered(seen.id, r.at ?? null);
        answered(archived.id, r.at ?? null);
      } else back();
      return r;
    },
    (e: unknown) => {
      back();
      throw e;
    },
  );
}

/** Names a session where its harness keeps its name, in every window at once. Rejects with why
 *  when it can't: OpenCode's service isn't running, say. */
export function renameAhead(key: string, name: string): Promise<void> {
  const a = made({ kind: 'rename', key, title: nameShown(name) });
  put(a, true);
  return renameSession(key, name).then(
    (shownAt) => answered(a.id, shownAt),
    (e: unknown) => {
      drop(a.id, true);
      throw e;
    },
  );
}

/** Answers the call `id` a session waits on (`sessionAnswer`). An answer that lets it go on shows
 *  it working in every window at once; a plain deny waits for the board. Resolves with what
 *  happened; rejects with why it didn't go, the session showing as it was. */
export function answerAhead(s: Session, id: string, answer: 'allow' | 'deny' | 'always', answers?: Answers, message?: string): Promise<string> {
  const goesOn = answer !== 'deny' || !!message?.trim();
  if (!goesOn || s.status !== 'needs' || s.asks) return sessionAnswer(s.key, id, answer, answers, message);
  const a = made({ kind: 'answer', key: s.key, since: s.statusSince ?? null });
  put(a, true);
  return sessionAnswer(s.key, id, answer, answers, message).then(
    (said) => {
      answered(a.id, null);
      return said;
    },
    (e: unknown) => {
      drop(a.id, true);
      throw e;
    },
  );
}

/** Messages in this window not yet gone, by session, in the order they were sent. */
const queued = new Map<string, { id: string; text: string; failed: unknown }[]>();
/** Each session's last message on its way: the next one waits for it. */
const tails = new Map<string, Promise<unknown>>();

/**
 * Sends a message to a session, shown at once. Resolves with what happened once it went. When it
 * doesn't go, it and every message sent after it are back in the session's draft, before
 * whatever's been written since, and each rejects with why. `toDraft: false` leaves the draft
 * alone, for a caller that puts the text back itself (the quick panel's input). `now` sends it
 * into the turn the session is taking at once (Send now). `resume`: a session that isn't running
 * goes on there with it (start.rs `resume`), rather than in a terminal.
 */
export function sendAhead(s: Session, text: string, { toDraft = true, now = false, resume }: { toDraft?: boolean; now?: boolean; resume?: ResumeIn } = {}): Promise<Sent> {
  const key = s.key;
  const a = made({ kind: 'send', key, text, working: s.status === 'working', starts: startsTurn(s) });
  put(a, true);
  const mine = { id: a.id, text, failed: undefined as unknown };
  queued.set(key, [...(queued.get(key) ?? []), mine]);
  const leave = () => {
    const left = (queued.get(key) ?? []).filter((q) => q !== mine);
    if (left.length > 0) queued.set(key, left);
    else queued.delete(key);
  };
  const before = tails.get(key) ?? Promise.resolve();
  const flight = before
    // Only waits for the one before, which says how it went itself.
    .catch(() => {})
    .then(() => {
      // One before it didn't go, so this one didn't either: it's in the box already.
      if (mine.failed !== undefined) throw mine.failed;
      return resume ? resumeSession(key, resume, text) : sendToSession(key, text, now);
    });
  tails.set(key, flight);
  const clear = () => {
    if (tails.get(key) === flight) tails.delete(key);
  };
  return flight.then(
    (sent) => {
      leave();
      clear();
      // Opened with it to paste rather than sent: nothing to wait for.
      if (sent.opened) drop(a.id, true);
      else answered(a.id, null);
      return sent;
    },
    (e: unknown) => {
      if (mine.failed === undefined) {
        // It and the messages after it, which wait on it, go back in the box in order.
        const list = queued.get(key) ?? [];
        const later = list.slice(list.indexOf(mine) + 1);
        for (const q of later) q.failed = e;
        const texts = [mine, ...later].map((q) => q.text);
        if (toDraft) writeDraft(key, [...texts, readDraft(key)].filter((t) => t.trim()).join('\n\n'));
        for (const q of [mine, ...later]) drop(q.id, true);
      }
      leave();
      clear();
      throw e;
    },
  );
}

/** Whether a message to the session is on its way from this window or another. */
export function sending(changes: Ahead[], key: string): boolean {
  return changes.some((a) => a.change.kind === 'send' && a.change.key === key && a.doneAt == null);
}
