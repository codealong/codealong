// Changes you make that the engine takes a moment to show: sessions marked seen or unread, snoozed,
// set aside for their side conversations, quieted, moved to the top, labelled, archived, renamed, sent a message or let go on past a call they waited on, reviews asked
// of you snoozed, and PRs reviewed or merged.
// The engine takes them in its own loop, which waits behind scans and git on a
// busy Mac (half a second typically, seconds at worst), and only then does every window get a
// board that shows them. So each window shows a change at once, on top of the board it has, and
// tells the other windows (the `ahead` event): the bar's pill goes as you mark its session seen in
// the peek, and the main window shows the session working as you send it something from the bar.
//
// A seen, unread, snooze, set aside, quiet, move to the top, label, archive, rename or merge goes once a board at least as new as the one the engine says
// shows it arrives. Going by whether the board looks right instead would hide a session that turned unread
// again a moment later. One the engine turns down goes at once, and the caller says why.
//
// A merge shows as you confirm it, before GitHub has done it, and goes back with why if GitHub
// refuses it; once it has, the engine shows the PR merged at once (ghpoll.rs `on_merged`). Merge
// and done shows its session done with it, as Mark as done does. A review you submit takes the PR out of what waits on your review the same way, any verdict, as
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
// doesn't go is back in the box with why, with any sent after it. One that its harness holds
// until the session's turn ends shows as queued for as long as it waits: while the board lists
// the id its send said it went under (`Session.queued`, the Codex app's queue), or where the
// board doesn't list what's held, while something held came after it (`Session.queuedAt`).
// One sent into the turn from there (Send now on it, `steerAhead`) shows in its place as a message
// sent, until the record has it. One that went but that nothing shows the session took says so in
// the thread.

import { useSyncExternalStore } from 'react';
import { archiveSession, asideSessions, emitAhead, labelSessions, markSeen, markUnread, mergePr, onAhead, quietSessions, raiseSessions, renameSession, resumeSession, reviewPr, sendToSession, sessionAnswer, sessionDone, sessionSteer, sessionUnqueue, snoozeReviews, snoozeSessions, windowLabel, type Done, type MergeDone } from './api';
import { readDraft, writeDraft } from './drafts';
import { nameShown } from './harness';
import { heldForAgent } from './prStatus';
import { byRaise, needOf, relaned, sessionHeadline, stuckMidTurn, unreadable } from './lanes';
import type { Answers, BarFeed, BarItem, BarRecent, BarSnoozed, Board, Label, Lane, LastReview, LogoStatus, MergeOp, PrItem, QueuedMessage, ResumeIn, Reviewed, Sent, Session, Status, TranscriptItem, Verdict } from './types';

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
/** A message goes after this, whatever the board says, unless its harness still holds it: its
 *  thread has said what it knows. */
const SEND_LONGEST_MS = 10 * 60_000;

export type Change =
  | { kind: 'seen'; keys: string[] }
  /** Marked unread: Your turn again, from when it was marked. */
  | { kind: 'unread'; keys: string[] }
  /** Snoozed until then, or woken now with null. `picked` by Pick a time… (ui/SnoozeDialog.tsx),
   *  which the window it was asked from waits on (`snoozedFrom`). */
  | { kind: 'snooze'; keys: string[]; until: number | null; picked?: boolean }
  /** Set aside until their side conversations answer: out of Needs you, with what's working. */
  | { kind: 'aside'; keys: string[] }
  /** Quieted, or made heard again with false: a quiet one's finished turn is idle. */
  | { kind: 'quiet'; keys: string[]; quiet: boolean }
  /** Moved to the top of what waits on you, or back to their places with false. */
  | { kind: 'raise'; keys: string[]; raised: boolean }
  /** Labels put on, as they'll show (a new one in the color it'll get), and taken off by name. */
  | { kind: 'label'; keys: string[]; add: Label[]; remove: string[] }
  /** Your review of these PRs (by PR key) snoozed until then, or woken now with null; `picked`
   *  as a session's snooze is. */
  | { kind: 'review-snooze'; keys: string[]; until: number | null; picked?: boolean }
  /** A PR (by its key) merged: merged from when it was made, and waiting on no review. */
  | { kind: 'pr-merge'; key: string }
  /** Your review of a PR (by its key) submitted: it waits on your review no more, and shows
   *  yours, of `commit` where that's known (a draft's), else of the commit it's at. */
  | { kind: 'pr-review'; key: string; verdict: Verdict; commit: string | null }
  | { kind: 'archive'; key: string; archived: boolean }
  /** Named, as the board will show it. */
  | { kind: 'rename'; key: string; title: string }
  /** The call it waited on answered so that it goes on: working from when it was answered.
   *  `since` is when it began waiting on that call, as the board had it. */
  | { kind: 'answer'; key: string; since: number | null }
  /** A message its harness held, by its id in `Session.queued`, taken out so it never goes
   *  (`unqueueAhead`): its thread stops showing it until the board does. */
  | { kind: 'unqueue'; key: string; id: string }
  | {
      kind: 'send';
      key: string;
      text: string;
      /** The session was working as it was sent, or stuck mid-turn, so it takes the message in
       *  as it works. */
      working: boolean;
      /** The message reaches the session itself, rather than opening its app with the text to
       *  paste, so it starts a turn. */
      starts: boolean;
      /** A message its harness held, by its id in `Session.queued`, sent into the turn now
       *  (`steerAhead`): its thread shows this in its place. */
      steers?: string;
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
  /** A message's id where its harness holds it until the turn ends, as its send said
   *  (`Sent.queued`). */
  queuedAs?: string;
  // A message, as this window reads the board. Each window reads its own; other windows don't hear.
  /** When the board first showed the session took it. */
  takenAt?: number;
  /** CONFIRM_MS went by with no sign of it: the session shows as the board has it. */
  lapsed?: boolean;
  /** Lapsed, and the session isn't working either. */
  unconfirmed?: boolean;
  /** The board shows its harness holds it until the session's turn ends. */
  queued?: boolean;
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
  /** Set aside for its side conversation (`Session.aside`). */
  aside?: boolean;
  /** When the side conversation that answered since it was last seen answered
   *  (`Session.sideAnswer`). Unknown to the bar. */
  answeredAt?: number;
  quiet: boolean;
  /** Moved to the top (`Session.raised`). */
  raised?: boolean;
  /** When the latest message its harness holds came (`Session.queuedAt`), null when it holds
   *  none; undefined where the view can't say (the bar's). */
  queuedAt: number | null | undefined;
  /** The ids of the messages its harness holds, where the board lists them (`Session.queued`);
   *  undefined where it lists none, or the view can't say. */
  queued?: string[];
  /** Stuck mid-turn (lanes.ts `stuckMidTurn`), so a message waits for the step it's on, as it
   *  does while it works. Unknown to the bar. */
  stuck?: boolean;
  /** The labels on it (`Session.labels`). Unknown to the bar, which shows none. */
  labels?: Label[];
}

/** What a window shows: the time of its board (null with none yet) and its sessions. */
export interface View {
  boardAt: number | null;
  find: (key: string) => Shown | undefined;
}

/* ---- What the changes do to what a window shows ---- */

/** Marking it seen settles it: a finished turn, a reply that asks you something, or a turn that
 *  was cut off (lanes.ts `seenable`). */
function settledBySeen(s: { status: Status; asks?: boolean; cutOff?: number }): boolean {
  return s.status === 'unread' || (s.status === 'needs' && !!s.asks) || (s.status === 'error' && s.cutOff != null);
}

/** Setting it aside takes it out of Needs you: one that needs you and isn't set aside yet. The
 *  engine sets it aside only while its side conversation answers (board.rs `aside_holds`), which
 *  the window that asks has just seen it do. */
function settledByAside(s: { status: Status; aside?: number | boolean | null; archived?: boolean }): boolean {
  return s.status === 'needs' && !s.aside && !s.archived;
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
    if (a.change.kind === 'seen' || a.change.kind === 'unread' || a.change.kind === 'snooze' || a.change.kind === 'aside' || a.change.kind === 'quiet' || a.change.kind === 'raise' || a.change.kind === 'label') for (const k of a.change.keys) add(k, a);
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
    // Seen, a finished turn is taken up, and no longer first (board.rs `seen_takes_up`).
    if (c.kind === 'seen' && settledBySeen(out)) out = { ...out, status: 'idle', asks: false, raised: undefined };
    else if (c.kind === 'unread' && unreadable(out)) out = { ...out, status: 'unread', statusSince: a.at };
    else if (c.kind === 'archive' && out.archived !== c.archived) out = { ...out, archived: c.archived };
    else if (c.kind === 'rename' && out.title !== c.title) out = { ...out, title: c.title };
    else if (c.kind === 'answer' && stillAsked(out, out.statusSince, c)) out = { ...out, status: 'working', statusSince: a.at, stalledSince: null };
    // Quieted, a finished turn is idle (board.rs, `apply_quiet`). Made heard again, one you
    // haven't seen is Your turn again, which only the engine knows: the board brings it.
    else if (c.kind === 'quiet' && !!out.quiet !== c.quiet) out = { ...out, quiet: c.quiet, ...(c.quiet && out.status === 'unread' ? { status: 'idle' as const } : {}) };
    else if (c.kind === 'label' && !labelled(out, c)) out = { ...out, labels: withLabels(out.labels ?? [], c.add, c.remove) };
    // Moved to the top, it's first until you take it up, and one whose turn you've seen waits on
    // you again (board.rs `apply_raise`); it's woken from a snooze, the two being opposites.
    else if (c.kind === 'raise' && !out.archived && !!out.raised !== c.raised) {
      const back = c.raised && (out.status === 'idle' || out.status === 'stopped');
      out = { ...out, raised: c.raised ? { at: a.at } : undefined, ...(c.raised ? { snoozedUntil: undefined } : {}), ...(back ? { status: 'unread' as const, statusSince: a.at } : {}) };
    }
    else if (c.kind === 'snooze' && !out.archived) {
      if (c.until != null) {
        if (out.snoozedUntil !== c.until) out = { ...out, snoozedUntil: c.until, raised: undefined };
      } else if (out.snoozedUntil) {
        // Woken, one with nothing to show for itself is Your turn (board.rs, `apply_snooze`).
        const back = out.status === 'idle' || out.status === 'stopped';
        out = { ...out, snoozedUntil: undefined, ...(back ? { status: 'unread' as const, statusSince: a.at } : {}) };
      }
    }
    // Set aside, one that needs you waits on its side conversation first (board.rs `apply_aside`).
    else if (c.kind === 'aside' && settledByAside(out)) out = { ...out, aside: a.at };
    else if (c.kind === 'send' && a.takenAt == null && !a.lapsed) {
      // A message takes a session out of the archive before it's sent: you took it up again
      // (actions.rs `unarchive_to_send`). Only while it's on its way: once it went, the engine
      // has published that, and the board says what's archived, an archive after it too.
      if (out.archived && a.doneAt == null) out = { ...out, archived: false };
      // A prompt sent to a snoozed session ends its snooze: you took it up (board.rs, `apply_snooze`).
      // So does its move to the top (board.rs, `apply_raise`).
      if (startsNow(a) && out.status !== 'working') out = { ...out, status: 'working', statusSince: a.at, stalledSince: null, asks: false, snoozedUntil: undefined, raised: undefined, lastPromptAt: Math.max(out.lastPromptAt, a.at) };
    }
  }
  // Brought back, or taken up, a turn its agent marked seen waits on you as any does (board.rs
  // `settle_agent_seen`).
  if (out.agentSeen && out.status !== 'idle') out = { ...out, agentSeen: undefined };
  if (out === s) return s;
  // What it waited on goes with the status that said so.
  if (out.need !== needOf(out)) out = { ...out, need: needOf(out) };
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

/** A session's color on the mark: a snoozed one only while it works, and one set aside as its
 *  side conversation works for it. */
function logoColor(s: { status: Status; snoozedUntil?: number | null; aside?: number | null }): LogoStatus {
  if (s.status === 'working') return 'working';
  if (s.snoozedUntil || s.status === 'stopped') return 'idle';
  return s.aside ? 'working' : s.status;
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

/** The PRs this window's changes review, by PR key, each with the review as it shows: the commit
 *  it's of where that's known (a draft's), else the one the PR's at. */
function reviewed(changes: Ahead[]): Map<string, { state: LastReview['state']; commit: string | null; at: number }> {
  const done = new Map<string, { state: LastReview['state']; commit: string | null; at: number }>();
  const said = { approve: 'approved', 'request-changes': 'changes', comment: 'commented' } as const;
  for (const a of changes) if (a.change.kind === 'pr-review') done.set(a.change.key, { state: said[a.change.verdict], commit: a.change.commit, at: a.at });
  return done;
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
    const review = done.get(p.key);
    if (review && next.reviewRequested) next = { ...next, reviewRequested: false };
    // Yours shows until the board has it, or one given after it (the engine's, ghpoll.rs
    // `on_approved`, is a moment later).
    if (review && !p.mine && !((next.lastReview?.at ?? -Infinity) >= review.at)) next = { ...next, lastReview: { state: review.state, commit: review.commit ?? next.headOid ?? null, at: review.at } };
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
  /** When it was set aside for its side conversation, if it was. */
  const asideAt = (key: string): number | undefined => (by.get(key) ?? []).find((a) => a.change.kind === 'aside')?.at;
  /** Quieted by the last quiet change for it: a finished turn of its leaves Your turn. */
  const quieted = (key: string): boolean => {
    const last = (by.get(key) ?? []).filter((a) => a.change.kind === 'quiet').pop()?.change;
    return last?.kind === 'quiet' && last.quiet;
  };
  /** The last move to the top, or back, for a pill: when it was moved, null back, undefined with none. */
  const raised = (key: string): number | null | undefined => {
    const last = (by.get(key) ?? []).filter((a) => a.change.kind === 'raise').pop();
    return last?.change.kind === 'raise' ? (last.change.raised ? last.at : null) : undefined;
  };
  /** The last seen or move to the top for a session: a seen takes up a move made before it, and a
   *  move brings back one seen before it (board.rs `seen_takes_up`, `apply_raise`). */
  const upLast = (key: string) => (by.get(key) ?? []).filter((a) => a.change.kind === 'seen' || (a.change.kind === 'raise' && a.change.raised)).pop();
  const snoozing: BarSnoozed[] = [];
  let changed = false;
  const items: BarItem[] = [];
  for (const i of feed.items) {
    const seen = upLast(i.key)?.change.kind === 'seen' && settledBySeen(i);
    if (archived(i.key) || seen || (i.status === 'unread' && quieted(i.key))) {
      changed = true;
      continue;
    }
    const m = moved(i.key) ?? goesOn(i);
    const until = snooze(i.key);
    const aside = asideAt(i.key);
    if (m && i.group !== 'working') {
      changed = true;
      items.push({ ...i, group: 'working', status: 'working', asks: false, since: m.at });
    } else if (aside != null && i.group === 'needs' && i.status === 'needs') {
      // Set aside, it's with what's working, drawn waiting, as barfeed.rs lists it.
      changed = true;
      items.push({ ...i, group: 'working', status: 'working', waiting: true, since: aside });
    } else if (until != null && i.group !== 'working') {
      changed = true;
      snoozing.push({ ...i, group: 'snoozed', snoozedUntil: until });
    } else {
      const at = raised(i.key);
      if (at !== undefined && !!i.raised !== (at !== null)) {
        changed = true;
        items.push({ ...i, raised: at === null ? undefined : { at } });
      } else items.push(i);
    }
  }
  const leave = <T extends BarRecent | BarSnoozed>(list: T[]): T[] =>
    list.filter((r) => {
      const m = moved(r.key);
      const answer = goesOn(r);
      const until = snooze(r.key);
      const up = upLast(r.key);
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
      } else if (!gone && up?.change.kind === 'raise') {
        // Moved to the top, it waits on you, woken from a snooze, and one whose turn you'd seen
        // is Your turn again (board.rs `apply_raise`), until a seen after it.
        const needs = r.status === 'needs' || r.status === 'error';
        items.push({ ...pill, group: needs ? 'needs' : 'unread', status: needs ? r.status : 'unread', since: r.group === 'recent' ? up.at : r.since, raised: { at: up.at } });
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
  // As barfeed.rs orders them: by group, the ones moved to the top first, then each a queue, the
  // longest first.
  items.sort((a, b) => GROUP_RANK.indexOf(a.group) - GROUP_RANK.indexOf(b.group) || byRaise(a.raised, b.raised) || a.since - b.since || a.key.localeCompare(b.key));
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
          if (s) return { title: s.title, status: s.status, statusSince: s.statusSince, lastPromptAt: s.lastPromptAt, archived: s.archived, asks: !!s.asks, snoozedUntil: s.snoozedUntil ?? null, aside: s.aside != null, answeredAt: s.sideAnswer?.at, quiet: !!s.quiet, raised: !!s.raised, queuedAt: s.queuedAt ?? null, queued: s.queued?.map((m) => m.id), stuck: stuckMidTurn(s), labels: s.labels ?? [] };
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
      return i && { title: i.title, status: i.status, statusSince: i.since, lastPromptAt: null, archived: false, asks: !!i.asks, snoozedUntil: i.snoozedUntil ?? null, quiet: !!i.quiet, raised: 'raised' in i && !!i.raised, queuedAt: undefined };
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
    const keys = c.kind === 'archive' || c.kind === 'rename' || c.kind === 'answer' || c.kind === 'unqueue' ? [c.key] : c.keys;
    const shows = keys.every((k) => {
      const s = v.find(k);
      if (s === undefined) return false;
      if (c.kind === 'answer') return !stillAsked(s, s.statusSince, c);
      if (c.kind === 'unqueue') return !s.queued?.includes(c.id);
      // A side conversation that answered after it was marked seen brings it back, as the
      // engine has it (board.rs `apply_side_answer`), whether or not a board showed it seen
      // first: ⌘⏎ asks seen and goes on before the engine takes the mark (menus.ts `askAside`).
      if (c.kind === 'seen') return !settledBySeen(s) || (s.answeredAt ?? 0) > a.at;
      if (c.kind === 'unread') return !unreadable(s);
      if (c.kind === 'snooze') return s.snoozedUntil === c.until;
      // The bar can't tell one set aside from one working, nor can a window whose session no
      // longer needs you: either way it's out of Needs you.
      if (c.kind === 'aside') return !settledByAside(s);
      if (c.kind === 'quiet') return s.quiet === c.quiet;
      if (c.kind === 'raise') return !!s.raised === c.raised;
      if (c.kind === 'label') return s.labels === undefined || labelled(s, c);
      if (c.kind === 'rename') return s.title === c.title;
      return s.archived === c.archived;
    });
    return shows ? null : a;
  }
  if (a.doneAt == null) return a;
  const s = v.find(c.key);
  // Its harness holds it until the turn ends, however long that is: its record has it only then.
  // That's so whatever else the board says, and even after it seemed taken: a send at the turn's
  // end that fails puts it back. Where the board lists what's held, it's held while that has the
  // id it went under, and one that went into the turn (Send now) isn't, whatever was queued after;
  // elsewhere, while something held came after it.
  const listed = a.queuedAs != null || s?.queued != null;
  const held = listed ? a.queuedAs != null && !!s?.queued?.includes(a.queuedAs) : s?.queuedAt != null && s.queuedAt >= a.at - SLACK_MS;
  if (held) return a.queued ? a : { ...a, queued: true, takenAt: undefined, lapsed: false, unconfirmed: false };
  // A view that can't see the queue (the bar's) leaves one held as it was.
  if (a.queued && s?.queuedAt === undefined) return a;
  let next = a;
  if (next.takenAt == null && s && took(s, a, c)) next = { ...next, takenAt: now, lapsed: false, unconfirmed: false, queued: false };
  if (next.takenAt != null) return now - next.takenAt > TAKEN_MS ? null : next;
  // Out of the queue: sent in as the turn ended, or dropped there.
  if (next.queued) return { ...next, takenAt: now, queued: false };
  if (now - a.at > SEND_LONGEST_MS) return null;
  if (!next.lapsed && now - a.doneAt >= CONFIRM_MS) next = { ...next, lapsed: true };
  if (next.lapsed) {
    const quiet = s !== undefined && s.status !== 'working' && !s.stuck && now - Math.max(a.doneAt, s.statusSince ?? 0) >= CONFIRM_MS;
    if (quiet !== !!next.unconfirmed) next = { ...next, unconfirmed: quiet };
  }
  return next;
}

/**
 * The messages the thread should still show, and those its items have: a prompt of yours (or
 * another session's message, as a session that took yours as one may record it; a Mac before
 * 2026-10-14 sends that as a note) from about when each was sent, taken in order.
 */
export function inRecord(sends: Ahead[], items: TranscriptItem[]): { showing: Ahead[]; found: string[] } {
  const said = items.filter((it) => it.kind === 'you' || it.kind === 'peer' || (it.kind === 'note' && it.text.startsWith('From another session')));
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
  return { id: a.id, change: a.change, at: a.at, doneAt: a.doneAt, shownAt: a.shownAt, ...(a.queuedAs != null && { queuedAs: a.queuedAs }) };
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
function answered(id: string, shownAt: number | null, queuedAs?: string): void {
  const a = all.find((x) => x.id === id);
  if (a) put({ ...a, doneAt: Date.now(), shownAt, ...(queuedAs != null && { queuedAs }) }, true);
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

/** Marks sessions seen, showing it at once: by `mark`, where something else marks them (a side
 *  conversation asked with `seen`), else `markSeen`. */
export function seenAhead(keys: string[], mark: (keys: string[]) => Promise<number | null> = markSeen): Promise<void> {
  if (keys.length === 0) return Promise.resolve();
  const a = made({ kind: 'seen', keys });
  put(a, true);
  return mark(keys).then(
    (shownAt) => answered(a.id, shownAt),
    (e: unknown) => {
      drop(a.id, true);
      throw e;
    },
  );
}

/** Sets sessions that need you aside until their side conversations answer, in every window at
 *  once (`asideSessions`). */
export function asideAhead(keys: string[]): Promise<void> {
  if (keys.length === 0) return Promise.resolve();
  const a = made({ kind: 'aside', keys });
  put(a, true);
  return asideSessions(keys).then(
    (shownAt) => answered(a.id, shownAt),
    (e: unknown) => {
      drop(a.id, true);
      throw e;
    },
  );
}

/** Snoozes sessions until `until`, or wakes them now with null, in every window at once. */
export function snoozeAhead(keys: string[], until: number | null, picked = false): Promise<void> {
  if (keys.length === 0) return Promise.resolve();
  const a = made({ kind: 'snooze', keys, until, ...(picked && { picked }) });
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

const named = (l: Label, name: string) => l.name.toLowerCase() === name.trim().toLowerCase();

/** `labels` with those named in `remove` taken off, and those in `add` put on after the rest, a
 *  label by its name, whatever its case: the engine puts them on by name (`labelSessions`). */
export function withLabels(labels: readonly Label[], add: readonly Label[], remove: readonly string[]): Label[] {
  const kept = labels.filter((l) => !remove.some((r) => named(l, r)));
  return [...kept, ...add.filter((a) => !remove.some((r) => named(a, r)) && !kept.some((l) => named(l, a.name)))];
}

/** Whether a session shows a label change: it has each put on, and none taken off. */
function labelled(s: Pick<Session, 'labels'>, c: { add: Label[]; remove: string[] }): boolean {
  const has = s.labels ?? [];
  return c.add.every((a) => has.some((l) => named(l, a.name))) && !c.remove.some((r) => has.some((l) => named(l, r)));
}

/** Puts labels on sessions and takes others off, by name, in every window at once: `add` as each
 *  will show, a new one in the color its device will give it (todos.ts `nextColor`). Rejects with
 *  why when a device didn't. */
export function labelAhead(keys: string[], add: Label[], remove: string[]): Promise<void> {
  if (keys.length === 0 || (add.length === 0 && remove.length === 0)) return Promise.resolve();
  const a = made({ kind: 'label', keys, add, remove });
  put(a, true);
  return labelSessions(
    keys,
    add.map((l) => l.name),
    remove,
  ).then(
    (shownAt) => answered(a.id, shownAt),
    (e: unknown) => {
      drop(a.id, true);
      throw e;
    },
  );
}

/** Moves sessions to the top of what waits on you, or back to their places with false, in every
 *  window at once. */
export function raiseAhead(keys: string[], raised: boolean): Promise<void> {
  if (keys.length === 0) return Promise.resolve();
  const a = made({ kind: 'raise', keys, raised });
  put(a, true);
  return raiseSessions(keys, raised).then(
    (shownAt) => answered(a.id, shownAt),
    (e: unknown) => {
      drop(a.id, true);
      throw e;
    },
  );
}

/** How long a window waits on the main window's Pick a time… (snooze.ts `timesMenu`): one left
 *  open longer, or cancelled, says nothing in the window it was asked from. */
const PICK_MS = 10 * 60_000;

/** The Pick a time… this window waits on (`snoozedFrom`), to stop waiting: one at a time. */
let picking: (() => void) | null = null;

/** Calls `heard` once Pick a time…'s dialog (ui/SnoozeDialog.tsx), in the main window, snoozes
 *  these sessions or your review of these PRs, so the window it was asked from says they left, as
 *  it does for a time in its own menu. It's called as the snooze is put, before the window draws
 *  it, as a time of the menu's own settles: once drawn, the session is no longer in the waiting
 *  list a queue goes on from (queue.ts `left`). Never, when the dialog is put away, left open past
 *  `wait`, or asked again from this window: a snooze of the window's own isn't it, nor one asked
 *  for since. */
export function snoozedFrom(keys: string[], reviews: boolean, heard: () => void, wait = PICK_MS): void {
  picking?.();
  const since = Date.now();
  const same = (a: Ahead) => {
    const c = a.change;
    return a.at >= since && (c.kind === 'snooze' || c.kind === 'review-snooze') && !!c.picked && (c.kind === 'review-snooze') === reviews && c.until != null && c.keys.length === keys.length && c.keys.every((k) => keys.includes(k));
  };
  const unsubscribe = subscribe(() => {
    if (!all.some(same)) return;
    stop();
    heard();
  });
  const timer = setTimeout(() => stop(), wait);
  const stop = () => {
    clearTimeout(timer);
    unsubscribe();
    if (picking === stop) picking = null;
  };
  picking = stop;
}

/** Snoozes your review of PRs (by PR key) until `until`, or wakes them now with null, in every
 *  window at once. */
export function snoozeReviewsAhead(keys: string[], until: number | null, picked = false): Promise<void> {
  if (keys.length === 0) return Promise.resolve();
  const a = made({ kind: 'review-snooze', keys, until, ...(picked && { picked }) });
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
  return merging(pr, id, op).then((d) => d.note);
}

/** Merge and done… (ui/MergeDone.tsx): merges a PR as `mergeAhead` does, and once it's merged
 *  the engine marks the session `key` done, ending work under way too, so that's done even when
 *  the window that asked has gone by then. The session shows seen and archived as it's asked, as
 *  Mark as done does (`doneAhead`), and is back as it was where GitHub refuses the merge or the
 *  engine didn't mark it done. */
export function mergeDoneAhead(pr: Pick<PrItem, 'key' | 'repoSlug' | 'number'>, id: string, op: MergeOp & { op: 'merge' }, key: string): Promise<MergeDone> {
  const shown = doneShown(key);
  return merging(pr, id, op, key).then(
    (d) => {
      shown.answered(d.done);
      return d;
    },
    (e: unknown) => {
      shown.back();
      throw e;
    },
  );
}

/** A merge, shown merged in every window as it's asked, and back with why when GitHub refuses it. */
function merging(pr: Pick<PrItem, 'key' | 'repoSlug' | 'number'>, id: string, op: MergeOp, done?: string): Promise<MergeDone> {
  const a = made({ kind: 'pr-merge', key: pr.key });
  put(a, true);
  return mergePr(pr.repoSlug, pr.number, id, op, done).then(
    (d) => {
      answered(a.id, d.shownAt);
      return d;
    },
    (e: unknown) => {
      drop(a.id, true);
      throw e;
    },
  );
}

/** Reviews a PR as you (`reviewPr`): it's out of what waits on your review in every window as
 *  it's asked, showing your review (of `commit`, a draft's, where that isn't the commit the PR's
 *  at), and back, with why, if GitHub refuses it. The engine does the same as GitHub has it
 *  (ghpoll.rs `on_approved`) and shows that before it answers, so the change goes a while after. */
export function reviewAhead(pr: Pick<PrItem, 'key' | 'repoSlug' | 'number'>, verdict: Verdict, body: string, agents = false, commit: string | null = null): Promise<Reviewed> {
  const a = made({ kind: 'pr-review', key: pr.key, verdict, commit });
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
  const shown = doneShown(key);
  const asked = after ? after.then(() => sessionDone(key, stop), () => sessionDone(key, stop)) : sessionDone(key, stop);
  return asked.then(
    (r) => {
      shown.answered(r);
      return r;
    },
    (e: unknown) => {
      shown.back();
      throw e;
    },
  );
}

/** The session `key` shown done, seen and archived, in every window as it's asked: kept once the
 *  engine says it's done (`answered`) until a board shows it, back as it was where it isn't. */
function doneShown(key: string): { answered: (r: Done | undefined) => void; back: () => void } {
  const seen = made({ kind: 'seen', keys: [key] });
  const archived = made({ kind: 'archive', key, archived: true });
  put(seen, true);
  put(archived, true);
  const back = () => {
    drop(seen.id, true);
    drop(archived.id, true);
  };
  return {
    answered: (r) => {
      if (!r?.done) return back();
      answered(seen.id, r.at ?? null);
      answered(archived.id, r.at ?? null);
    },
    back,
  };
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

/** Whether how a send went is worth saying where a sent message usually goes without a word: it
 *  was put on the clipboard or in its app rather than sent, it went as a message from Codealong
 *  because its terminal couldn't be typed into, or it may not have gone in. */
export function sentWorthSaying(sent: Sent): boolean {
  return sent.opened || !!sent.fellBack || !!sent.uncertain;
}

/**
 * Sends a message to a session, shown at once. Resolves with what happened once it went. When it
 * doesn't go, it and every message sent after it are back in the session's draft, before
 * whatever's been written since, and each rejects with why. `toDraft: false` leaves the draft
 * alone, for a caller that puts the text back itself (the quick panel's input). `now` sends it
 * into the turn the session is taking at once (Send now). `resume`: a session that isn't running
 * goes on there with it (start.rs `resume`), rather than in a terminal. `starts: false` is for one
 * that may start no turn (a command for its shell), which the session isn't shown working for
 * meanwhile.
 */
export function sendAhead(s: Session, text: string, { toDraft = true, now = false, resume, starts = startsTurn(s) }: { toDraft?: boolean; now?: boolean; resume?: ResumeIn; starts?: boolean } = {}): Promise<Sent> {
  const key = s.key;
  const a = made({ kind: 'send', key, text, working: s.status === 'working' || stuckMidTurn(s), starts });
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
      else answered(a.id, null, sent.queued);
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

/**
 * Sends a message the session's harness holds until its turn ends into the turn it's taking now
 * (`sessionSteer`), shown at once in its place: sending, then sent until the session's record
 * has it, which out of the queue is only at the turn's next step. Resolves with what happened;
 * when it doesn't go it's still held, and rejects with why.
 */
export function steerAhead(s: Session, m: QueuedMessage): Promise<string> {
  // One of yours it holds is still its send's, which the thread hides behind the queue's row (by
  // `queuedAs`): this takes its place, its words as you wrote them, and gives it back if it
  // doesn't go.
  const was = all.find((x) => x.change.kind === 'send' && x.change.key === s.key && x.queuedAs === m.id);
  const text = was && was.change.kind === 'send' ? was.change.text : m.text;
  const a = made({ kind: 'send', key: s.key, text, working: true, starts: false, steers: m.id });
  if (was) drop(was.id, true);
  put(a, true);
  return sessionSteer(s.key, m.id).then(
    (said) => {
      answered(a.id, null);
      return said;
    },
    (e: unknown) => {
      drop(a.id, true);
      if (was) put(was, true);
      throw e;
    },
  );
}

/**
 * Takes a message the session's harness holds until its turn ends out of what it holds
 * (`sessionUnqueue`), so it never goes: its thread stops showing it at once, and one of yours
 * with it, and shows them again if it's still held after all. Where the harness gave back the
 * others it held with it, which went back in under new ids, one of yours among them goes on
 * showing as held. Resolves with what happened; rejects with why it wasn't taken out.
 */
export function unqueueAhead(s: Session, m: QueuedMessage): Promise<string> {
  const yours = (id: string) => all.find((x) => x.change.kind === 'send' && x.change.key === s.key && x.queuedAs === id);
  const was = yours(m.id);
  const a = made({ kind: 'unqueue', key: s.key, id: m.id });
  if (was) drop(was.id, true);
  put(a, true);
  return sessionUnqueue(s.key, m.id).then(
    (done) => {
      answered(a.id, null);
      for (const r of done.requeued ?? []) {
        const sent = yours(r.was);
        if (sent) put({ ...sent, queuedAs: r.now }, true);
      }
      return done.message;
    },
    (e: unknown) => {
      drop(a.id, true);
      if (was) put(was, true);
      throw e;
    },
  );
}

/** A long paste or an image in what a harness lists as held, which only names it
 *  (claude/unqueue.rs `by_placeholder`, codex/queue.rs `words`). */
const NAMES_WHAT_IT_HOLDS = /\[(?:Pasted text|Image|\.\.\.Truncated text) #/;

/**
 * A held message's words to write again (Edit), from `sends`, the session's: one of yours as you
 * wrote it, what it came with too, else as its harness lists it, or null where that only names
 * part of it, which would be lost as it came out.
 */
export function heldWords(sends: Ahead[], m: QueuedMessage): string | null {
  const yours = sends.find((a) => a.queuedAs === m.id)?.change;
  if (yours?.kind === 'send') return yours.text;
  return NAMES_WHAT_IT_HOLDS.test(m.text) ? null : m.text;
}

/** Whether a message to the session is on its way from this window or another. */
export function sending(changes: Ahead[], key: string): boolean {
  return changes.some((a) => a.change.kind === 'send' && a.change.key === key && a.doneAt == null);
}
