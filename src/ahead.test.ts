import { beforeEach, describe, expect, it, vi } from 'vitest';

const api = vi.hoisted(() => ({
  sendToSession: vi.fn(),
  markSeen: vi.fn(),
  archiveSession: vi.fn(),
  renameSession: vi.fn(),
  mergePr: vi.fn(),
  sessionAnswer: vi.fn(),
  sessionSteer: vi.fn(),
  sessionUnqueue: vi.fn(),
  reviewPr: vi.fn(),
  snoozeSessions: vi.fn(() => Promise.resolve(null)),
  snoozeReviews: vi.fn(() => Promise.resolve(null)),
}));
vi.mock('./api', () => ({
  ...api,
  emitAhead: () => {},
  onAhead: () => () => {},
  windowLabel: () => 'test',
}));

import { answerAhead, barAhead, boardAhead, boardView, CONFIRM_MS, inRecord, lanesAhead, logoOf, mergeAhead, mergeDoneAhead, renameAhead, reviewAhead, sendAhead, sentWorthSaying, settled, snoozeAhead, snoozedFrom, snoozeReviewsAhead, steerAhead, unqueueAhead, type Ahead, type View } from './ahead';
import { readDraft, writeDraft } from './drafts';
import { bayOf, stuckMidTurn } from './lanes';
import * as fx from './testing/fixtures';
import type { BarFeed, BarItem, BarRecent, BarSnoozed, Lane, Session, TranscriptItem, Verdict } from './types';

const NOW = 1_790_200_000_000;

const session = (key: string, status: Session['status'], extra: Partial<Session> = {}): Session =>
  fx.session({
    key,
    status,
    live: true,
    title: `Task ${key}`,
    lastActivity: NOW - 60_000,
    lastPromptAt: NOW - 120_000,
    statusSince: NOW - 60_000,
    excerpt: 'Did it',
    headline: 'from the engine',
    detail: 'from the engine',
    ...extra,
  });

const lane = (sessions: Session[]): Lane => {
  const [bay, glyph] = bayOf(sessions, null);
  return fx.lane({ id: '/r', sessions, bay, glyph, headline: 'lane from the engine' });
};

const change = (c: Ahead['change'], extra: Partial<Ahead> = {}): Ahead => ({ id: `id-${Math.random()}`, change: c, at: NOW, doneAt: null, shownAt: null, ...extra });
const send = (key: string, extra: Partial<Ahead> = {}, working = false): Ahead => change({ kind: 'send', key, text: 'Also the docs', working, starts: true }, extra);

describe('lanesAhead', () => {
  it('shows a finished turn marked seen as idle, and the lane with it', () => {
    const lanes = [lane([session('a', 'unread')])];
    const [l] = lanesAhead(lanes, [change({ kind: 'seen', keys: ['a'] })]);
    expect(l.sessions[0].status).toBe('idle');
    expect([l.sessions[0].headline, l.sessions[0].detail]).toEqual(['`claude` · Task a', `last turn {ago:${NOW - 60_000}}`]);
    expect([l.bay, l.glyph, l.headline]).toEqual(['idle', 'idle', '`claude` · Task a']);
  });

  it('shows a session marked unread as your turn at once, from when it was marked', () => {
    const [l] = lanesAhead([lane([session('a', 'idle'), session('b', 'working')])], [change({ kind: 'unread', keys: ['a', 'b'] })]);
    // One that's working shows that.
    expect(l.sessions.map((s) => s.status)).toEqual(['unread', 'working']);
    expect(l.sessions[0].statusSince).toBe(NOW);
  });

  it('brings back a turn its agent marked seen as yours, no longer saying its agent saw it', () => {
    const seen = { at: NOW - 90_000, note: 'Nothing new on the deploy' };
    const [l] = lanesAhead([lane([session('a', 'idle', { agentSeen: seen }), session('b', 'idle', { agentSeen: seen })])], [change({ kind: 'unread', keys: ['a'] })]);
    expect(l.sessions.map((s) => [s.status, s.agentSeen])).toEqual([
      ['unread', undefined],
      ['idle', seen],
    ]);
  });

  it('settles a reply that asks you something, but not a permission prompt', () => {
    const asks = session('a', 'needs', { asks: true });
    const prompt = session('b', 'needs');
    const [l] = lanesAhead([lane([asks, prompt])], [change({ kind: 'seen', keys: ['a', 'b'] })]);
    expect(l.sessions.map((s) => s.status)).toEqual(['idle', 'needs']);
    expect(l.bay).toBe('needs');
  });

  it('settles a turn that was cut off, but not another error', () => {
    const cut = session('a', 'error', { cutOff: NOW - 60_000 });
    const error = session('b', 'error');
    const [l] = lanesAhead([lane([cut, error])], [change({ kind: 'seen', keys: ['a', 'b'] })]);
    expect(l.sessions.map((s) => s.status)).toEqual(['idle', 'error']);
  });

  it('takes an archived session out of its lane’s status', () => {
    const [l] = lanesAhead([lane([session('a', 'unread'), session('b', 'idle')])], [change({ kind: 'archive', key: 'a', archived: true })]);
    expect(l.sessions[0].archived).toBe(true);
    expect(l.bay).toBe('idle');
  });

  it('shows a session sent a message as working until the board shows it took it', () => {
    const lanes = [lane([session('a', 'idle')])];
    const going = send('a');
    const [l] = lanesAhead(lanes, [going]);
    expect([l.sessions[0].status, l.sessions[0].statusSince, l.sessions[0].lastPromptAt]).toEqual(['working', NOW, NOW]);
    expect([l.bay, l.headline, l.detail]).toEqual(['working', '`claude` · Task a', `{for:${NOW}} · Did it`]);
    // Once the board shows it, or it lapsed, the board has the say.
    expect(lanesAhead(lanes, [{ ...going, takenAt: NOW + 1 }])).toBe(lanes);
    expect(lanesAhead(lanes, [{ ...going, lapsed: true }])).toBe(lanes);
  });

  it('shows a session whose call was answered as working, until it waits on another', () => {
    const lanes = [lane([session('a', 'needs', { excerpt: 'Bash: npm test', stalledSince: NOW - 5_000 })])];
    const answer = change({ kind: 'answer', key: 'a', since: NOW - 60_000 });
    const [l] = lanesAhead(lanes, [answer]);
    expect([l.sessions[0].status, l.sessions[0].statusSince, l.sessions[0].stalledSince]).toEqual(['working', NOW, null]);
    expect([l.bay, l.headline, l.detail]).toEqual(['working', '`claude` · Task a', `{for:${NOW}} · Bash: npm test`]);
    // A call asked after the one answered is another, and a reply that asks something isn't a call.
    expect(lanesAhead([lane([session('a', 'needs', { statusSince: NOW + 2_000 })])], [answer])[0].sessions[0].status).toBe('needs');
    expect(lanesAhead([lane([session('a', 'needs', { asks: true })])], [answer])[0].sessions[0].status).toBe('needs');
  });

  it('takes an archived session sent a message out of the archive while it’s on its way', () => {
    const lanes = [lane([session('a', 'idle', { archived: true }), session('b', 'working', { archived: true }), session('c', 'stopped', { archived: true })])];
    const opens = change({ kind: 'send', key: 'c', text: 'x', working: false, starts: false });
    const [l] = lanesAhead(lanes, [send('a'), send('b', {}, true), opens]);
    expect(l.sessions.map((s) => [s.archived, s.status])).toEqual([
      [false, 'working'],
      [false, 'working'],
      [false, 'stopped'],
    ]);
    // An archive after it wins, and once it went, the board has the say: an archive that came
    // after it and settled isn't undone while the message waits to be taken in.
    expect(lanesAhead(lanes, [send('b', {}, true), change({ kind: 'archive', key: 'b', archived: true })])[0].sessions[1].archived).toBe(true);
    expect(lanesAhead(lanes, [send('b', { doneAt: NOW + 100 }, true)])).toBe(lanes);
  });

  it('leaves a working session as it is, and a message that opens an app instead', () => {
    const lanes = [lane([session('a', 'working'), session('b', 'idle')])];
    expect(lanesAhead(lanes, [send('a', {}, true), change({ kind: 'send', key: 'b', text: 'x', working: false, starts: false })])).toBe(lanes);
  });

  it('snoozes a session without moving its lane, and wakes one as Your turn', () => {
    const lanes = [lane([session('a', 'needs'), session('b', 'idle', { snoozedUntil: NOW + 60_000 })])];
    const [l] = lanesAhead(lanes, [change({ kind: 'snooze', keys: ['a'], until: NOW + 900_000 })]);
    expect([l.sessions[0].snoozedUntil, l.bay, l.headline]).toEqual([NOW + 900_000, 'needs', 'lane from the engine']);
    const [woke] = lanesAhead(lanes, [change({ kind: 'snooze', keys: ['b'], until: null })]);
    expect([woke.sessions[1].snoozedUntil, woke.sessions[1].status]).toEqual([undefined, 'unread']);
  });

  it('quiets a finished turn out of Your turn at once, and leaves what needs you', () => {
    const lanes = [lane([session('u', 'unread'), session('n', 'needs', { asks: true }), session('i', 'idle')])];
    const [l] = lanesAhead(lanes, [change({ kind: 'quiet', keys: ['u', 'n'], quiet: true })]);
    expect(l.sessions.map((s) => [s.key, s.status, !!s.quiet])).toEqual([
      ['u', 'idle', true],
      ['n', 'needs', true],
      ['i', 'idle', false],
    ]);
    expect(l.bay).toBe('needs');
    // Made heard again, it waits for the board to say whether there's a turn you haven't seen.
    const [heard] = lanesAhead([lane([session('u', 'idle', { quiet: true })])], [change({ kind: 'quiet', keys: ['u'], quiet: false })]);
    expect([heard.sessions[0].status, heard.sessions[0].quiet]).toEqual(['idle', false]);
    const quiet = [lane([session('q', 'idle', { quiet: true })])];
    expect(lanesAhead(quiet, [change({ kind: 'quiet', keys: ['q'], quiet: true })])).toBe(quiet);
  });

  it('moves a session to the top at once, bringing back one whose turn you saw, until it’s seen', () => {
    const lanes = [lane([session('u', 'unread'), session('i', 'idle'), session('z', 'needs', { snoozedUntil: NOW + 60_000 })])];
    const [l] = lanesAhead(lanes, [change({ kind: 'raise', keys: ['u', 'i', 'z'], raised: true })]);
    expect(l.sessions.map((s) => [s.key, s.status, !!s.raised, s.snoozedUntil ?? null])).toEqual([
      ['u', 'unread', true, null],
      ['i', 'unread', true, null],
      ['z', 'needs', true, null],
    ]);
    // Seen, or snoozed, it's no longer first.
    const top = [lane([session('u', 'unread', { raised: { at: NOW } })])];
    expect(lanesAhead(top, [change({ kind: 'seen', keys: ['u'] })])[0].sessions[0].raised).toBeUndefined();
    expect(lanesAhead(top, [change({ kind: 'snooze', keys: ['u'], until: NOW + 60_000 })])[0].sessions[0].raised).toBeUndefined();
    expect(lanesAhead(top, [change({ kind: 'raise', keys: ['u'], raised: false })])[0].sessions[0].raised).toBeUndefined();
  });

  it('names a renamed session, in its headline and its lane’s', () => {
    const lanes = [lane([session('a', 'working'), session('b', 'idle')])];
    const [l] = lanesAhead(lanes, [change({ kind: 'rename', key: 'a', title: 'Fix the login redirect' })]);
    expect([l.sessions[0].title, l.sessions[0].headline, l.headline]).toEqual(['Fix the login redirect', '`claude` · Fix the login redirect', '`claude` · Fix the login redirect']);
    expect(l.sessions[1]).toBe(lanes[0].sessions[1]);
    // Once the board has the name, nothing changes.
    expect(lanesAhead(lanes, [change({ kind: 'rename', key: 'b', title: 'Task b' })])).toBe(lanes);
  });

  it('hands back the same lanes when nothing applies', () => {
    const lanes = [lane([session('a', 'idle')])];
    expect(lanesAhead(lanes, [])).toBe(lanes);
    expect(lanesAhead(lanes, [change({ kind: 'seen', keys: ['a'] })])).toBe(lanes);
  });

  it('colors the mark from what’s left', () => {
    const lanes = lanesAhead([lane([session('a', 'unread'), session('b', 'working')])], [change({ kind: 'seen', keys: ['a'] })]);
    expect(logoOf(lanes, [])).toBe('working');
  });
});

describe('boardAhead', () => {
  it('snoozes a review asked of you, and the mark leaves it out', () => {
    const asked = fx.prItem({ number: 1, reviewRequested: true });
    const other = fx.prItem({ number: 2 });
    const board = fx.board({ lanes: [lane([session('a', 'idle')])], prs: [asked, other], logoStatus: 'needs' });
    const out = boardAhead(board, [change({ kind: 'review-snooze', keys: [asked.key, other.key], until: NOW + 900_000 })]);
    expect(out.prs.map((p) => p.snoozedUntil ?? null)).toEqual([NOW + 900_000, null]);
    expect(out.prs[1]).toBe(other);
    expect(out.lanes).toBe(board.lanes);
    expect(out.logoStatus).toBe('idle');
    // Woken, it waits on you again.
    const back = boardAhead(out, [change({ kind: 'review-snooze', keys: [asked.key], until: null })]);
    expect(back.prs[0].snoozedUntil).toBeNull();
    expect(back.logoStatus).toBe('needs');
    expect(boardAhead(board, [change({ kind: 'review-snooze', keys: [other.key], until: NOW + 1 })])).toBe(board);
  });

  it('shows a PR merged as it’s asked, out of what waits on your review', () => {
    const asked = fx.prItem({ number: 1, reviewRequested: true, state: 'approved', mergedAt: null, closedAt: null });
    const other = fx.prItem({ number: 2 });
    const board = fx.board({ lanes: [lane([session('a', 'idle')])], prs: [asked, other], logoStatus: 'needs' });
    const out = boardAhead(board, [change({ kind: 'pr-merge', key: asked.key })]);
    expect(out.prs[0]).toMatchObject({ state: 'merged', mergedAt: NOW, closedAt: NOW, reviewRequested: false });
    expect(out.prs[1]).toBe(other);
    expect(out.logoStatus).toBe('idle');
    // One the board shows merged already, or closed, is left as the board has it.
    const merged = { ...board, prs: [{ ...asked, state: 'merged' as const, mergedAt: NOW - 5 }] };
    expect(boardAhead(merged, [change({ kind: 'pr-merge', key: asked.key })])).toBe(merged);
  });

  it('takes a PR you reviewed out of what waits on your review as it’s asked', () => {
    const asked = fx.prItem({ number: 1, reviewRequested: true, snoozedUntil: null });
    const other = fx.prItem({ number: 2 });
    const board = fx.board({ lanes: [lane([session('a', 'idle')])], prs: [asked, other], logoStatus: 'needs' });
    const out = boardAhead(board, [change({ kind: 'pr-review', key: asked.key, verdict: 'approve', commit: null })]);
    expect(out.prs[0]).toMatchObject({ reviewRequested: false, state: asked.state });
    expect(out.prs[1]).toBe(other);
    expect(out.logoStatus).toBe('idle');
  });

  it('shows the review you gave a PR at once, until the board has it', () => {
    const pr = fx.prItem({ number: 1, headOid: 'c0ffee1aaaa', lastReview: { state: 'approved', commit: '0ld0ld0', at: NOW - 86_400_000 } });
    const board = fx.board({ prs: [pr] });
    const gave = (verdict: Verdict, commit: string | null) => boardAhead(board, [change({ kind: 'pr-review', key: pr.key, verdict, commit })]).prs[0].lastReview;
    // Of the commit it's at, unless it was a draft's, begun on another.
    expect(gave('approve', null)).toEqual({ state: 'approved', commit: 'c0ffee1aaaa', at: NOW });
    expect(gave('request-changes', 'bead123')).toEqual({ state: 'changes', commit: 'bead123', at: NOW });
    expect(gave('comment', null)?.state).toBe('commented');
    // The board's own, once it has yours or a later one, is what shows; a PR of yours has none.
    const shown = { ...board, prs: [{ ...pr, lastReview: { state: 'approved' as const, commit: 'c0ffee1aaaa', at: NOW + 400 } }] };
    expect(boardAhead(shown, [change({ kind: 'pr-review', key: pr.key, verdict: 'approve', commit: null })])).toBe(shown);
    const mine = { ...board, prs: [{ ...pr, mine: true, lastReview: undefined }] };
    expect(boardAhead(mine, [change({ kind: 'pr-review', key: pr.key, verdict: 'comment', commit: null })])).toBe(mine);
  });
});

describe('settled', () => {
  const view = (boardAt: number | null, s?: Partial<Session> & { status: Session['status'] }): View => ({
    boardAt,
    find: () => (s ? { title: s.title ?? 'Title', status: s.status, statusSince: s.statusSince ?? null, lastPromptAt: s.lastPromptAt ?? null, archived: s.archived ?? false, asks: false, snoozedUntil: s.snoozedUntil ?? null, quiet: s.quiet ?? false, queuedAt: s.queuedAt ?? null, queued: s.queued?.map((m) => m.id), stuck: stuckMidTurn({ stalledSince: s.stalledSince ?? null, background: s.background }) } : undefined),
  });

  it('keeps a seen until a board at least as new as the engine’s arrives', () => {
    const a = change({ kind: 'seen', keys: ['a'] }, { doneAt: NOW + 100, shownAt: NOW + 50 });
    // A board that came first still shows it unread, and the change holds.
    expect(settled(a, view(NOW + 40, { status: 'unread' }), NOW + 200)).toBe(a);
    // Even a board that looks right doesn't settle it: it may have turned unread again since.
    expect(settled(a, view(NOW + 50, { status: 'unread' }), NOW + 200)).toBeNull();
  });

  it('keeps a rename until the engine’s board arrives, or one from another device shows the name', () => {
    const a = change({ kind: 'rename', key: 'a', title: 'New name' }, { doneAt: NOW + 100, shownAt: NOW + 50 });
    expect(settled(a, view(NOW + 40, { status: 'idle', title: 'Old name' }), NOW + 200)).toBe(a);
    expect(settled(a, view(NOW + 50, { status: 'idle', title: 'New name' }), NOW + 200)).toBeNull();
    const remote = change({ kind: 'rename', key: 'a', title: 'New name' }, { doneAt: NOW + 100 });
    expect(settled(remote, view(NOW + 500, { status: 'idle', title: 'Old name' }), NOW + 1_000)).toBe(remote);
    expect(settled(remote, view(NOW + 900, { status: 'idle', title: 'New name' }), NOW + 1_000)).toBeNull();
  });

  it('waits on the engine, but not forever', () => {
    const a = change({ kind: 'seen', keys: ['a'] });
    expect(settled(a, view(NOW + 5_000, { status: 'idle' }), NOW + 5_000)).toBe(a);
    expect(settled(a, view(null), NOW + 61_000)).toBeNull();
  });

  it('keeps a review a while after the engine answered, its board showing it by then', () => {
    const a = change({ kind: 'pr-review', key: 'o/r#1', verdict: 'approve', commit: null });
    expect(settled(a, view(NOW + 90_000), NOW + 9_000)).toBe(a);
    const done = { ...a, doneAt: NOW + 3_000 };
    expect(settled(done, view(NOW + 9_000), NOW + 9_000)).toBe(done);
    expect(settled(done, view(NOW + 9_000), NOW + 24_000)).toBeNull();
  });

  it('keeps a merge until the engine’s board shows it, or a while with none', () => {
    const a = change({ kind: 'pr-merge', key: 'o/r#1' });
    expect(settled(a, view(NOW + 5_000), NOW + 5_000)).toBe(a);
    const done = { ...a, doneAt: NOW + 3_000, shownAt: NOW + 2_900 };
    expect(settled(done, view(NOW + 2_000), NOW + 3_100)).toBe(done);
    expect(settled(done, view(NOW + 2_900), NOW + 3_100)).toBeNull();
    expect(settled({ ...a, doneAt: NOW + 3_000 }, view(NOW + 9_000), NOW + 24_000)).toBeNull();
  });

  it('settles one the engine gave no board for (another device’s) once the view shows it', () => {
    const a = change({ kind: 'archive', key: 'a', archived: true }, { doneAt: NOW + 100 });
    expect(settled(a, view(NOW, { status: 'idle' }), NOW + 200)).toBe(a);
    expect(settled(a, view(NOW, { status: 'idle', archived: true }), NOW + 200)).toBeNull();
    expect(settled(a, view(NOW, { status: 'idle' }), NOW + 21_000)).toBeNull();
    const q = change({ kind: 'quiet', keys: ['a'], quiet: true }, { doneAt: NOW + 100 });
    expect(settled(q, view(NOW, { status: 'unread' }), NOW + 200)).toBe(q);
    expect(settled(q, view(NOW, { status: 'idle', quiet: true }), NOW + 200)).toBeNull();
  });

  it('keeps an answer until the board shows the session past the call it was for', () => {
    const a = change({ kind: 'answer', key: 'a', since: NOW - 60_000 }, { doneAt: NOW + 100 });
    expect(settled({ ...a, doneAt: null }, view(NOW + 5_000, { status: 'working' }), NOW + 5_000)).not.toBeNull();
    // Still waiting on it, or shown from an earlier time as Claude Code's registry may say it.
    expect(settled(a, view(NOW + 500, { status: 'needs', statusSince: NOW - 60_000 }), NOW + 600)).toBe(a);
    expect(settled(a, view(NOW + 500, { status: 'needs', statusSince: NOW - 90_000 }), NOW + 600)).toBe(a);
    expect(settled(a, view(NOW + 500, { status: 'working', statusSince: NOW + 400 }), NOW + 600)).toBeNull();
    expect(settled(a, view(NOW + 500, { status: 'needs', statusSince: NOW + 400 }), NOW + 600)).toBeNull();
    // One the board never shows goes a few seconds after the engine took it, the session as the
    // board has it: what it shows may be another call it can't tell from the one answered.
    expect(settled(a, view(NOW + 5_000, { status: 'needs', statusSince: NOW - 60_000 }), NOW + 5_000)).toBe(a);
    expect(settled(a, view(NOW + 8_000, { status: 'needs', statusSince: NOW - 60_000 }), NOW + 8_200)).toBeNull();
  });

  it('marks a message taken when the board has the prompt, and lets it go a moment later', () => {
    const a = send('a', { doneAt: NOW + 500 });
    const taken = settled(a, view(NOW, { status: 'working', statusSince: NOW + 800, lastPromptAt: NOW + 700 }), NOW + 1_000);
    expect(taken?.takenAt).toBe(NOW + 1_000);
    expect(settled(taken!, view(NOW, { status: 'working' }), NOW + 7_000)).toBeNull();
  });

  it('takes a turn started after it as its doing, for a session that wasn’t working', () => {
    const a = send('a', { doneAt: NOW + 500 });
    expect(settled(a, view(NOW, { status: 'needs', statusSince: NOW + 900 }), NOW + 1_000)?.takenAt).toBe(NOW + 1_000);
    expect(settled(a, view(NOW, { status: 'idle', statusSince: NOW - 90_000 }), NOW + 1_000)?.takenAt).toBeUndefined();
  });

  it('says a message went unseen only once the session has been quiet a while', () => {
    const a = send('a', { doneAt: NOW + 500 });
    const quiet = view(NOW, { status: 'idle', statusSince: NOW - 90_000 });
    expect(settled(a, quiet, NOW + 5_000)).toBe(a);
    const lapsed = settled(a, quiet, NOW + 500 + CONFIRM_MS);
    expect([lapsed?.lapsed, lapsed?.unconfirmed]).toEqual([true, true]);
    // One sent while it worked is taken in as it works: nothing to say while it does.
    const working = settled(send('b', { doneAt: NOW + 500 }, true), view(NOW, { status: 'working', statusSince: NOW - 90_000 }), NOW + 500 + CONFIRM_MS);
    expect([working?.lapsed, !!working?.unconfirmed]).toEqual([true, false]);
    // Nor while it's stuck mid-turn, where it waits for the step it's stuck on.
    const stuck = settled(send('c', { doneAt: NOW + 500 }, true), view(NOW, { status: 'needs', statusSince: NOW - 90_000, stalledSince: NOW - 90_000 }), NOW + 500 + CONFIRM_MS);
    expect([stuck?.lapsed, !!stuck?.unconfirmed]).toEqual([true, false]);
    const board = boardView(NOW, [[lane([session('c', 'needs', { statusSince: NOW - 90_000, stalledSince: NOW - 90_000 })])]]);
    expect(settled(send('c', { doneAt: NOW + 500 }, true), board, NOW + 500 + CONFIRM_MS)?.unconfirmed).toBeFalsy();
    // One stuck on a command its turn left in the background starts a turn on it, as an idle one does.
    const left = settled(send('d', { doneAt: NOW + 500 }), view(NOW, { status: 'needs', statusSince: NOW - 90_000, stalledSince: NOW - 90_000, background: 'acme login --device' }), NOW + 500 + CONFIRM_MS);
    expect([left?.lapsed, left?.unconfirmed]).toEqual([true, true]);
  });

  it('keeps a message its harness holds until the turn ends as queued, however long that is', () => {
    const a = send('a', { doneAt: NOW + 500 });
    // The board shows it held, though the session looks idle and did something since.
    const holding = view(NOW, { status: 'idle', statusSince: NOW + 900, queuedAt: NOW + 300 });
    const queued = settled(a, holding, NOW + 1_000);
    expect([queued?.queued, queued?.takenAt]).toEqual([true, undefined]);
    expect(settled(queued!, holding, NOW + 1_000)).toBe(queued);
    expect(settled(queued!, holding, NOW + 60 * 60_000)).toBe(queued);
    // One held from before it was sent is another message.
    expect(settled(a, view(NOW, { status: 'working', queuedAt: NOW - 5_000 }), NOW + 1_000)?.queued).toBeUndefined();
    // Out of the queue as the turn ends, the record has it soon; then it goes.
    const sent = settled(queued!, view(NOW, { status: 'working', lastPromptAt: NOW + 30 * 60_000 }), NOW + 30 * 60_000);
    expect([sent?.queued, sent?.takenAt]).toEqual([false, NOW + 30 * 60_000]);
    expect(settled(sent!, view(NOW, { status: 'working' }), NOW + 31 * 60_000)).toBeNull();
    // Dropped there, the same: the record never has it.
    const dropped = settled(queued!, view(NOW, { status: 'idle', statusSince: NOW + 900 }), NOW + 2_000);
    expect(dropped?.takenAt).toBe(NOW + 2_000);
  });

  it('keeps a message queued while the board lists the id it went under, and no other', () => {
    const q = (id: string, at: number) => ({ id, at, text: id });
    const a = send('a', { doneAt: NOW + 500, queuedAs: 'q-a' });
    // Not yet listed: the queue is read a moment after the send.
    expect(settled(a, view(NOW, { status: 'working' }), NOW + 1_000)?.queued).toBeUndefined();
    const listing = view(NOW, { status: 'working', queuedAt: NOW + 300, queued: [q('q-a', NOW + 300)] });
    const queued = settled(a, listing, NOW + 1_000);
    expect([queued?.queued, queued?.takenAt]).toEqual([true, undefined]);
    expect(settled(queued!, listing, NOW + 60 * 60_000)).toBe(queued);
    // Taken out to go in, while one queued after it waits: out of the queue.
    const after = view(NOW, { status: 'working', queuedAt: NOW + 9_000, queued: [q('q-b', NOW + 9_000)] });
    expect(settled(queued!, after, NOW + 10_000)?.takenAt).toBe(NOW + 10_000);
    // One sent into the turn (Send now) isn't queued by one queued after it.
    const now = send('a', { doneAt: NOW + 500 });
    expect(settled(now, after, NOW + 10_000)?.queued).toBeUndefined();
  });

  it('shows a held message sent into the turn as sent until the record has it, however long its next step takes', () => {
    const q = (id: string, at: number) => ({ id, at, text: id });
    const a = change({ kind: 'send', key: 'a', text: 'q-a', working: true, starts: false, steers: 'q-a' }, { doneAt: NOW + 500 });
    // The board lists it a moment longer, until the queue is read again: it isn't held for that.
    const listed = settled(a, view(NOW, { status: 'working', queuedAt: NOW - 60_000, queued: [q('q-a', NOW - 60_000)] }), NOW + 1_000);
    expect(listed?.queued).toBeUndefined();
    // The turn takes it in at its next step, which can be a while: still sent, not unseen.
    const waiting = settled(listed!, view(NOW, { status: 'working' }), NOW + 40_000);
    expect([waiting?.lapsed, !!waiting?.unconfirmed, waiting?.takenAt]).toEqual([true, false, undefined]);
    expect(settled(waiting!, view(NOW, { status: 'working', lastPromptAt: NOW + 38_000 }), NOW + 41_000)?.takenAt).toBe(NOW + 41_000);
  });

  it('leaves a queued message as it was where the view can’t see the queue', () => {
    const queued = settled(send('a', { doneAt: NOW + 500 }), view(NOW, { status: 'working', queuedAt: NOW + 300 }), NOW + 1_000)!;
    // The bar's view has no queue, and doesn't know when the session was last sent a prompt.
    const bar: View = { boardAt: NOW, find: () => ({ title: 'Title', status: 'idle', statusSince: NOW + 900, lastPromptAt: null, archived: false, asks: false, snoozedUntil: null, quiet: false, queuedAt: undefined }) };
    expect(settled(queued, bar, NOW + 2_000)).toBe(queued);
  });

  it('shows a message queued again when the send at the turn’s end fails and puts it back', () => {
    const queued = settled(send('a', { doneAt: NOW + 500 }), view(NOW, { status: 'working', queuedAt: NOW + 300 }), NOW + 1_000)!;
    const flushing = settled(queued, view(NOW, { status: 'working', lastPromptAt: NOW + 60_000 }), NOW + 60_000)!;
    expect([flushing.queued, flushing.takenAt]).toEqual([false, NOW + 60_000]);
    const back = settled(flushing, view(NOW, { status: 'working', lastPromptAt: NOW + 60_000, queuedAt: NOW + 300 }), NOW + 61_000);
    expect([back?.queued, back?.takenAt]).toEqual([true, undefined]);
    expect(settled(back!, view(NOW, { status: 'working', lastPromptAt: NOW + 60_000, queuedAt: NOW + 300 }), NOW + 70_000)).toBe(back);
  });
});

describe('barAhead', () => {
  const item = (key: string, group: BarItem['group'], since: number, extra: Partial<BarItem> = {}): BarItem => fx.barItem({ key, group, since, ...extra });
  const feed = (items: BarItem[], recent: BarRecent[] = [], snoozed: BarSnoozed[] = []): BarFeed => fx.barFeed({ items, recent, snoozed, logoStatus: 'needs' });

  it('takes pills marked seen off the bar, but not a permission prompt', () => {
    const f = barAhead(feed([item('p', 'needs', NOW), item('q', 'needs', NOW, { asks: true }), item('u', 'unread', NOW), item('c', 'needs', NOW, { status: 'error', cutOff: NOW - 60_000 })]), [change({ kind: 'seen', keys: ['p', 'q', 'u', 'c'] })]);
    expect(f.items.map((i) => i.key)).toEqual(['p']);
  });

  it('moves a session sent a message to the end of Working, and colors the mark', () => {
    const f = barAhead(feed([item('u', 'unread', NOW - 10), item('w', 'working', NOW - 5)], [{ ...item('r', 'working', NOW - 99), group: 'recent', status: 'idle' }]), [send('u', { at: NOW + 1 }), send('r')]);
    expect(f.items.map((i) => [i.key, i.group])).toEqual([
      ['w', 'working'],
      ['r', 'working'],
      ['u', 'working'],
    ]);
    expect(f.recent).toEqual([]);
    expect(f.logoStatus).toBe('working');
  });

  it('moves a session whose call was answered to Working, but not one asked about since', () => {
    const f = feed([item('n', 'needs', NOW - 50), item('m', 'needs', NOW + 10)]);
    const out = barAhead(f, [change({ kind: 'answer', key: 'n', since: NOW - 50 }), change({ kind: 'answer', key: 'm', since: NOW - 50 })]);
    expect(out.items.map((i) => [i.key, i.group, i.since])).toEqual([
      ['m', 'needs', NOW + 10],
      ['n', 'working', NOW],
    ]);
  });

  it('moves a snoozed session whose call was answered to Working, its snooze kept', () => {
    const z = { ...item('z', 'needs', NOW - 9), group: 'snoozed' as const, snoozedUntil: NOW + 3_600_000 };
    const out = barAhead(feed([item('w', 'working', NOW - 50)], [], [z]), [change({ kind: 'answer', key: 'z', since: NOW - 9 })]);
    expect(out.items.map((i) => [i.key, i.group, i.status, i.snoozedUntil])).toEqual([
      ['w', 'working', 'working', null],
      ['z', 'working', 'working', NOW + 3_600_000],
    ]);
    expect(out.snoozed).toEqual([]);
  });

  it('keeps Needs you a queue, and takes a session you send to off the snoozed ones', () => {
    const f = feed([item('n2', 'needs', NOW - 5), item('n1', 'needs', NOW - 50), item('u', 'unread', NOW)], [], [{ ...item('z', 'needs', NOW - 9), group: 'snoozed', snoozedUntil: NOW + 3_600_000 }]);
    const out = barAhead(f, [change({ kind: 'seen', keys: ['u'] }), send('z')]);
    expect(out.items.map((i) => i.key)).toEqual(['n1', 'n2', 'z']);
    expect(out.snoozed).toEqual([]);
  });

  it('moves a snoozed session to the snoozed pill, and a woken one back', () => {
    const z: BarSnoozed = { ...item('z', 'needs', NOW - 9), group: 'snoozed', status: 'idle', snoozedUntil: NOW + 3_600_000 };
    const f = feed([item('n', 'needs', NOW - 50), item('w', 'working', NOW)], [], [z]);
    const out = barAhead(f, [change({ kind: 'snooze', keys: ['n', 'w'], until: NOW + 900_000 }), change({ kind: 'snooze', keys: ['z'], until: null })]);
    // A working session stays on Working; a woken one with nothing to show is Your turn.
    expect(out.items.map((i) => [i.key, i.group, i.status])).toEqual([
      ['z', 'unread', 'unread'],
      ['w', 'working', 'working'],
    ]);
    expect(out.snoozed.map((i) => [i.key, i.snoozedUntil])).toEqual([['n', NOW + 900_000]]);
  });

  it('takes a quieted session’s finished turn off the bar, but not what needs you', () => {
    const f = barAhead(feed([item('n', 'needs', NOW - 50), item('u', 'unread', NOW), item('w', 'working', NOW)]), [change({ kind: 'quiet', keys: ['n', 'u', 'w'], quiet: true })]);
    expect(f.items.map((i) => i.key)).toEqual(['n', 'w']);
    expect(f.logoStatus).toBe('needs');
    // Quieted and then made heard again, it stays until the engine's feed says.
    const back = barAhead(feed([item('u', 'unread', NOW)]), [change({ kind: 'quiet', keys: ['u'], quiet: true }), change({ kind: 'quiet', keys: ['u'], quiet: false })]);
    expect(back.items.map((i) => i.key)).toEqual(['u']);
  });

  it('puts a pill moved to the top first in its group on the bar', () => {
    const f = barAhead(feed([item('n', 'needs', NOW - 50), item('a', 'unread', NOW - 40), item('b', 'unread', NOW)]), [change({ kind: 'raise', keys: ['b'], raised: true })]);
    expect(f.items.map((i) => [i.key, !!i.raised])).toEqual([
      ['n', false],
      ['b', true],
      ['a', false],
    ]);
    // A recent one, its turn seen, is Your turn again, and a snoozed one is woken, both first.
    const recent = { ...item('r', 'working', NOW - 99, { status: 'idle' }), group: 'recent' as const };
    const snoozed = { ...item('z', 'needs', NOW - 9), group: 'snoozed' as const, snoozedUntil: NOW + 3_600_000 };
    const out = barAhead(feed([item('n', 'needs', NOW - 50), item('a', 'unread', NOW - 40)], [recent], [snoozed]), [change({ kind: 'raise', keys: ['r', 'z'], raised: true })]);
    expect(out.items.map((i) => [i.key, i.group, !!i.raised])).toEqual([
      ['z', 'needs', true],
      ['n', 'needs', false],
      ['r', 'unread', true],
      ['a', 'unread', false],
    ]);
    expect([out.recent.length, out.snoozed.length]).toEqual([0, 0]);
    // Seen after it was moved up, it's taken up: back where it was.
    const taken = barAhead(feed([], [recent]), [change({ kind: 'raise', keys: ['r'], raised: true }), change({ kind: 'seen', keys: ['r'] })]);
    expect([taken.items.length, taken.recent.map((i) => i.key)]).toEqual([0, ['r']]);
    // Moved up after it was seen, a pill is still there.
    const again = barAhead(feed([item('a', 'unread', NOW - 40)]), [change({ kind: 'seen', keys: ['a'] }), change({ kind: 'raise', keys: ['a'], raised: true })]);
    expect(again.items.map((i) => [i.key, !!i.raised])).toEqual([['a', true]]);
  });

  it('names a renamed session’s pill wherever it is on the bar', () => {
    const f = feed([item('n', 'needs', NOW - 5)], [{ ...item('r', 'working', NOW - 99), group: 'recent' }], [{ ...item('z', 'needs', NOW - 9), group: 'snoozed', snoozedUntil: NOW + 3_600_000 }]);
    const out = barAhead(f, ['n', 'r', 'z'].map((key) => change({ kind: 'rename', key, title: `Named ${key}` })));
    expect([out.items[0].title, out.recent[0].title, out.snoozed[0].title]).toEqual(['Named n', 'Named r', 'Named z']);
    expect(barAhead(f, [change({ kind: 'rename', key: 'n', title: 'n' })])).toBe(f);
  });

  it('takes a snoozed review off the bar, and the mark with it', () => {
    const f = { ...feed([item('w', 'working', NOW)]), reviews: [fx.barReview({ key: 'o/r#1' }), fx.barReview({ key: 'o/r#2' })] };
    const one = barAhead(f, [change({ kind: 'review-snooze', keys: ['o/r#1'], until: NOW + 900_000 })]);
    expect(one.reviews.map((r) => r.key)).toEqual(['o/r#2']);
    expect(one.logoStatus).toBe('needs');
    const both = barAhead(f, [change({ kind: 'review-snooze', keys: ['o/r#1', 'o/r#2'], until: NOW + 900_000 })]);
    expect(both.reviews).toEqual([]);
    expect(both.logoStatus).toBe('working');
  });

  it('takes a review given here off the bar', () => {
    const f = { ...feed([item('w', 'working', NOW)]), reviews: [fx.barReview({ key: 'o/r#1' }), fx.barReview({ key: 'o/r#2' })] };
    const out = barAhead(f, [change({ kind: 'pr-review', key: 'o/r#1', verdict: 'approve', commit: null })]);
    expect([out.reviews.map((r) => r.key), out.logoStatus]).toEqual([['o/r#2'], 'needs']);
  });

  it('takes a review merged here off the bar', () => {
    const f = { ...feed([item('w', 'working', NOW)]), reviews: [fx.barReview({ key: 'o/r#1' }), fx.barReview({ key: 'o/r#2' })] };
    const out = barAhead(f, [change({ kind: 'pr-merge', key: 'o/r#2' })]);
    expect(out.reviews.map((r) => r.key)).toEqual(['o/r#1']);
  });

  it('hands back the same feed when nothing applies', () => {
    const f = feed([item('w', 'working', NOW)]);
    expect(barAhead(f, [change({ kind: 'archive', key: 'x', archived: true })])).toBe(f);
  });
});

describe('inRecord', () => {
  const said = (kind: TranscriptItem['kind'], at: number, text = 'x'): TranscriptItem => ({ kind, at, text, failed: false, cursor: at, n: 0 });

  it('finds each message in the record from when it was sent, in order', () => {
    const first = send('a', { at: NOW });
    const second = send('a', { at: NOW + 2_000 });
    const items = [said('you', NOW - 60_000), said('agent', NOW + 100), said('you', NOW + 300)];
    expect(inRecord([first, second], items)).toEqual({ showing: [second], found: [first.id] });
    // A message from Codealong to a Claude app session may be another session's in its record,
    // a note of it from a Mac before 2026-10-14.
    expect(inRecord([second], [...items, said('peer', NOW + 2_500, 'Also the docs')]).found).toEqual([second.id]);
    expect(inRecord([second], [...items, said('note', NOW + 2_500, 'From another session: Also the docs')]).found).toEqual([second.id]);
  });
});

describe('sendAhead', () => {
  beforeEach(() => {
    api.sendToSession.mockReset();
    writeDraft('s', '');
  });

  it('puts a message that didn’t go back in the box, with the ones after it, in order', async () => {
    let fail: (e: Error) => void = () => {};
    api.sendToSession.mockImplementationOnce(() => new Promise((_, reject) => (fail = reject)));
    const s = session('s', 'idle');
    const one = sendAhead(s, 'First');
    const two = sendAhead(s, 'Second');
    writeDraft('s', 'Typed since');
    await vi.waitFor(() => expect(api.sendToSession).toHaveBeenCalled());
    fail(new Error('Its terminal is gone.'));
    await expect(one).rejects.toThrow('Its terminal is gone.');
    await expect(two).rejects.toThrow('Its terminal is gone.');
    expect(api.sendToSession).toHaveBeenCalledTimes(1);
    expect(readDraft('s')).toBe('First\n\nSecond\n\nTyped since');
  });

  it('sends messages to one session one after another', async () => {
    const order: string[] = [];
    api.sendToSession.mockImplementation(async (_key: string, text: string) => {
      order.push(text);
      return { message: 'Sent.', opened: false };
    });
    const s = session('s', 'idle');
    await Promise.all([sendAhead(s, 'One'), sendAhead(s, 'Two')]);
    expect(order).toEqual(['One', 'Two']);
  });
});

describe('steerAhead', () => {
  beforeEach(() => api.sessionSteer.mockReset());

  it('asks the engine for the held message by its id, and says why it didn’t go', async () => {
    const m = { id: 'u2', at: NOW - 60_000, text: 'And the docs' };
    api.sessionSteer.mockResolvedValueOnce('Sent to “Task a” now.');
    await expect(steerAhead(session('a', 'working'), m)).resolves.toBe('Sent to “Task a” now.');
    expect(api.sessionSteer).toHaveBeenCalledWith('a', 'u2');
    api.sessionSteer.mockRejectedValueOnce(new Error('Codex didn’t take it into the turn.'));
    await expect(steerAhead(session('b', 'working'), m)).rejects.toThrow('Codex didn’t take it into the turn.');
  });
});

describe('unqueueAhead', () => {
  beforeEach(() => api.sessionUnqueue.mockReset());

  it('asks the engine to take out the held message by its id, and says why it wasn’t', async () => {
    const m = { id: 'u2', at: NOW - 60_000, text: 'And the docs' };
    api.sessionUnqueue.mockResolvedValueOnce({ message: 'Took it out of what “Task a” holds: it won’t be sent.' });
    await expect(unqueueAhead(session('a', 'working'), m)).resolves.toBe('Took it out of what “Task a” holds: it won’t be sent.');
    expect(api.sessionUnqueue).toHaveBeenCalledWith('a', 'u2');
    api.sessionUnqueue.mockRejectedValueOnce(new Error('Claude Code doesn’t hold that message any more.'));
    await expect(unqueueAhead(session('b', 'working'), m)).rejects.toThrow('Claude Code doesn’t hold that message any more.');
  });

  it('goes once the board no longer lists it as held', () => {
    const a: Ahead = { id: 'x', change: { kind: 'unqueue', key: 'a', id: 'u2' }, at: NOW, doneAt: NOW + 500, shownAt: null };
    const view = (queued: string[]): View => ({ boardAt: NOW + 1000, find: (k) => (k === 'a' ? { title: 'Task a', status: 'working', statusSince: NOW, lastPromptAt: null, archived: false, asks: false, snoozedUntil: null, quiet: false, queuedAt: NOW, queued } : undefined) });
    expect(settled(a, view(['u1', 'u2']), NOW + 1000)).toBe(a);
    expect(settled(a, view(['u1', 'new-u3']), NOW + 1000)).toBeNull();
    expect(settled({ ...a, doneAt: null }, view(['u1']), NOW + 1000)).not.toBeNull();
  });
});

describe('renameAhead', () => {
  beforeEach(() => api.renameSession.mockReset());

  it('hands the engine the name as typed, and says why when the harness didn’t take it', async () => {
    api.renameSession.mockResolvedValueOnce(NOW);
    await renameAhead('s', 'Use `vitest` for `ahead`');
    expect(api.renameSession).toHaveBeenCalledWith('s', 'Use `vitest` for `ahead`');
    api.renameSession.mockRejectedValueOnce(new Error("OpenCode's service isn't running."));
    await expect(renameAhead('t', 'Other')).rejects.toThrow("OpenCode's service isn't running.");
  });
});

describe('answerAhead', () => {
  beforeEach(() => api.sessionAnswer.mockReset());

  it('hands the engine the answer, and says why it didn’t go', async () => {
    api.sessionAnswer.mockResolvedValueOnce('Allowed Bash(npm test).');
    await expect(answerAhead(session('a', 'needs'), 'ask-1', 'allow')).resolves.toBe('Allowed Bash(npm test).');
    expect(api.sessionAnswer).toHaveBeenCalledWith('a', 'ask-1', 'allow', undefined, undefined);
    api.sessionAnswer.mockRejectedValueOnce(new Error('It isn’t waiting on that any more.'));
    await expect(answerAhead(session('a', 'needs'), 'ask-2', 'deny', undefined, 'Use pnpm')).rejects.toThrow('It isn’t waiting on that any more.');
    expect(api.sessionAnswer).toHaveBeenLastCalledWith('a', 'ask-2', 'deny', undefined, 'Use pnpm');
  });
});

describe('reviewAhead', () => {
  beforeEach(() => api.reviewPr.mockReset());

  it('hands the engine the review, and says why GitHub refused it', async () => {
    api.reviewPr.mockResolvedValueOnce({ draft: 2 });
    await expect(reviewAhead({ key: 'o/r#7', repoSlug: 'o/r', number: 7 }, 'comment', 'Looks fine', true)).resolves.toEqual({ draft: 2 });
    expect(api.reviewPr).toHaveBeenCalledWith('o/r', 7, 'comment', 'Looks fine', true);
    api.reviewPr.mockRejectedValueOnce(new Error('Can not approve your own pull request.'));
    await expect(reviewAhead({ key: 'o/r#8', repoSlug: 'o/r', number: 8 }, 'approve', '')).rejects.toThrow('your own pull request');
  });
});

describe('mergeAhead', () => {
  beforeEach(() => api.mergePr.mockReset());
  const pr = { key: 'o/r#7', repoSlug: 'o/r', number: 7 };
  const merge = { op: 'merge' as const, head: 'abc', method: 'squash' as const, title: 'Add teams (#7)', body: '', email: null, deleteBranch: null };

  it('hands the engine the merge, and what GitHub couldn’t do after', async () => {
    api.mergePr.mockResolvedValueOnce({ note: 'Merged #7, but couldn’t delete its branch: gone', shownAt: NOW });
    await expect(mergeAhead(pr, 'PR_7', merge)).resolves.toBe('Merged #7, but couldn’t delete its branch: gone');
    expect(api.mergePr).toHaveBeenCalledWith('o/r', 7, 'PR_7', merge, undefined);
  });

  it('has the engine mark a session done with the merge, for Merge and done…', async () => {
    const answer = { note: null, shownAt: NOW, done: { done: true, message: 'Done with “Fix it”: it’s archived.', at: NOW } };
    api.mergePr.mockResolvedValueOnce(answer);
    await expect(mergeDoneAhead(pr, 'PR_7', merge, 'claude:c:1')).resolves.toEqual(answer);
    expect(api.mergePr).toHaveBeenCalledWith('o/r', 7, 'PR_7', merge, 'claude:c:1');
    api.mergePr.mockRejectedValueOnce(new Error('Couldn’t merge #7: Head branch was modified.'));
    await expect(mergeDoneAhead(pr, 'PR_7', merge, 'claude:c:1')).rejects.toThrow('Head branch was modified.');
  });

  it('says why GitHub refused it', async () => {
    api.mergePr.mockRejectedValueOnce(new Error('Couldn’t merge #7: Head branch was modified.'));
    await expect(mergeAhead(pr, 'PR_7', merge)).rejects.toThrow('Head branch was modified.');
  });

  it('hands on what else the box does as it is', async () => {
    api.mergePr.mockResolvedValueOnce({ note: null, shownAt: null });
    await expect(mergeAhead(pr, 'PR_7', { op: 'ready' })).resolves.toBeNull();
    expect(api.mergePr).toHaveBeenCalledWith('o/r', 7, 'PR_7', { op: 'ready' });
  });
});

describe('sentWorthSaying', () => {
  it('says how a send went only when it went otherwise than typed, or may not have gone in', () => {
    const typed = { message: 'Typed into “Fix it” in Ghostty.', opened: false };
    expect(sentWorthSaying(typed)).toBe(false);
    expect(sentWorthSaying({ ...typed, fellBack: false, uncertain: false })).toBe(false);
    expect(sentWorthSaying({ ...typed, opened: true })).toBe(true);
    expect(sentWorthSaying({ ...typed, fellBack: true })).toBe(true);
    expect(sentWorthSaying({ ...typed, uncertain: true })).toBe(true);
  });
});

describe('snoozedFrom', () => {
  const flush = () => new Promise((r) => setTimeout(r, 0));
  const later = () => Date.now() + 60_000;

  it('hears the snooze Pick a time… asked the main window for, and no other', async () => {
    const heard = vi.fn();
    snoozedFrom(['claude:claude:a'], false, heard);
    // Your review of a PR of that key, another session, waking it, or a time from a menu of the
    // window's own (the dialog put away), isn't it.
    await snoozeReviewsAhead(['claude:claude:a'], later(), true);
    await snoozeAhead(['claude:claude:b'], later(), true);
    await snoozeAhead(['claude:claude:a'], null, true);
    await snoozeAhead(['claude:claude:a'], later());
    await flush();
    expect(heard).not.toHaveBeenCalled();
    // As the snooze is put, before anything draws it: a queue goes on only from what it listed.
    void snoozeAhead(['claude:claude:a'], later(), true);
    expect(heard).toHaveBeenCalledTimes(1);
    await flush();
    expect(heard).toHaveBeenCalledTimes(1);
  });

  it('waits on the last asked for alone', async () => {
    const first = vi.fn();
    const second = vi.fn();
    snoozedFrom(['claude:claude:c'], false, first);
    snoozedFrom(['claude:claude:c'], false, second);
    await snoozeAhead(['claude:claude:c'], later(), true);
    await flush();
    expect([first.mock.calls.length, second.mock.calls.length]).toEqual([0, 1]);
  });

  it('gives up on one that is never snoozed', async () => {
    vi.useFakeTimers();
    try {
      const heard = vi.fn();
      snoozedFrom(['northwind/app#12'], true, heard, 1000);
      vi.advanceTimersByTime(1500);
      await snoozeReviewsAhead(['northwind/app#12'], Date.now() + 60_000, true);
      await vi.runAllTimersAsync();
      expect(heard).not.toHaveBeenCalled();
    } finally {
      vi.useRealTimers();
    }
  });
});
