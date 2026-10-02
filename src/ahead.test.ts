import { beforeEach, describe, expect, it, vi } from 'vitest';

const api = vi.hoisted(() => ({
  sendToSession: vi.fn(),
  markSeen: vi.fn(),
  archiveSession: vi.fn(),
  renameSession: vi.fn(),
}));
vi.mock('./api', () => ({
  ...api,
  emitAhead: () => {},
  onAhead: () => () => {},
  windowLabel: () => 'test',
}));

import { barAhead, CONFIRM_MS, inRecord, lanesAhead, logoOf, renameAhead, sendAhead, settled, type Ahead, type View } from './ahead';
import { readDraft, writeDraft } from './drafts';
import { bayOf } from './lanes';
import * as fx from './testing/fixtures';
import type { BarFeed, BarItem, BarRecent, BarSnoozed, Lane, Session, TranscriptItem } from './types';

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

  it('settles a reply that asks you something, but not a permission prompt', () => {
    const asks = session('a', 'needs', { asks: true });
    const prompt = session('b', 'needs');
    const [l] = lanesAhead([lane([asks, prompt])], [change({ kind: 'seen', keys: ['a', 'b'] })]);
    expect(l.sessions.map((s) => s.status)).toEqual(['idle', 'needs']);
    expect(l.bay).toBe('needs');
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

describe('settled', () => {
  const view = (boardAt: number | null, s?: Partial<Session> & { status: Session['status'] }): View => ({
    boardAt,
    find: () => (s ? { title: s.title ?? 'Title', status: s.status, statusSince: s.statusSince ?? null, lastPromptAt: s.lastPromptAt ?? null, archived: s.archived ?? false, asks: false, snoozedUntil: s.snoozedUntil ?? null } : undefined),
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

  it('settles one the engine gave no board for (another device’s) once the view shows it', () => {
    const a = change({ kind: 'archive', key: 'a', archived: true }, { doneAt: NOW + 100 });
    expect(settled(a, view(NOW, { status: 'idle' }), NOW + 200)).toBe(a);
    expect(settled(a, view(NOW, { status: 'idle', archived: true }), NOW + 200)).toBeNull();
    expect(settled(a, view(NOW, { status: 'idle' }), NOW + 21_000)).toBeNull();
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
  });
});

describe('barAhead', () => {
  const item = (key: string, group: BarItem['group'], since: number, extra: Partial<BarItem> = {}): BarItem => fx.barItem({ key, group, since, ...extra });
  const feed = (items: BarItem[], recent: BarRecent[] = [], snoozed: BarSnoozed[] = []): BarFeed => fx.barFeed({ items, recent, snoozed, logoStatus: 'needs' });

  it('takes pills marked seen off the bar, but not a permission prompt', () => {
    const f = barAhead(feed([item('p', 'needs', NOW), item('q', 'needs', NOW, { asks: true }), item('u', 'unread', NOW)]), [change({ kind: 'seen', keys: ['p', 'q', 'u'] })]);
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

  it('names a renamed session’s pill wherever it is on the bar', () => {
    const f = feed([item('n', 'needs', NOW - 5)], [{ ...item('r', 'working', NOW - 99), group: 'recent' }], [{ ...item('z', 'needs', NOW - 9), group: 'snoozed', snoozedUntil: NOW + 3_600_000 }]);
    const out = barAhead(f, ['n', 'r', 'z'].map((key) => change({ kind: 'rename', key, title: `Named ${key}` })));
    expect([out.items[0].title, out.recent[0].title, out.snoozed[0].title]).toEqual(['Named n', 'Named r', 'Named z']);
    expect(barAhead(f, [change({ kind: 'rename', key: 'n', title: 'n' })])).toBe(f);
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
    // A message from Codealong to a Claude app session is a note in its record.
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
