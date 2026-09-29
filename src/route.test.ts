import { describe, expect, it } from 'vitest';
import { duplicateOf, rankedBy, suggestions, sureness, todoTitle, worthRouting } from './route';
import type { Board, Lane, Route, Session } from './types';

const session = (key: string, status: Session['status'], extra: Partial<Session> = {}): Session => ({ key, status, title: key, visitor: null, ...extra }) as Session;
const lane = (id: string, sessions: Session[], repo = id): Lane => ({ id, sessions, callsign: id, repo, project: 'P' }) as unknown as Lane;
const board = (lanes: Lane[]): Board => ({ lanes }) as unknown as Board;

const b = board([lane('/api', [session('s:fix', 'unread'), session('s:ask', 'needs')]), lane('/web', [session('s:copy', 'working')])]);

const route = (r: Partial<Route>): Route => ({
  action: 'start',
  actions: [],
  sessions: [],
  noSession: 0,
  lanes: [],
  repos: [],
  ownWorktree: 0,
  ownWorktreeAsked: 0,
  habit: { new: 0, of: 0 },
  duplicate: null,
  model: 'jev-1.13.0',
  ms: 200,
  inputTokens: 1000,
  ...r,
});

const rows = (r: Route) => suggestions(r, b).map((s) => [s.key, Math.round(s.p * 100) / 100]);

describe('suggestions', () => {
  it('sends to the likeliest session, and offers starting new work when that could be meant', () => {
    const r = route({
      action: 'send',
      actions: [
        ['send', 0.7],
        ['start', 0.3],
      ],
      sessions: [
        { id: 's:fix', p: 0.9 },
        { id: 's:copy', p: 0.05 },
      ],
      noSession: 0.05,
      lanes: [{ id: '/api', p: 0.8 }],
    });
    expect(rows(r)).toEqual([
      ['send:s:fix', 0.63],
      ['start:/api', 0.24],
    ]);
  });

  it('opens a session that needs you instead of typing into it', () => {
    const r = route({ action: 'send', actions: [['send', 1]], sessions: [{ id: 's:ask', p: 1 }], lanes: [{ id: '/api', p: 1 }] });
    expect(suggestions(r, b)[0]).toMatchObject({ kind: 'open', key: 'open:s:ask' });
  });

  it('sends the answer to a session whose reply asks you something', () => {
    const asking = board([lane('/api', [session('s:ask', 'needs', { asks: true })])]);
    const r = route({ action: 'send', actions: [['send', 1]], sessions: [{ id: 's:ask', p: 1 }], lanes: [{ id: '/api', p: 1 }] });
    expect(suggestions(r, asking)[0]).toMatchObject({ kind: 'send', key: 'send:s:ask' });
  });

  it('starts new work in the likeliest worktree, in a worktree of its own when it wants one', () => {
    const r = route({
      action: 'start',
      actions: [
        ['start', 0.9],
        ['send', 0.1],
      ],
      sessions: [{ id: 's:copy', p: 0.6 }],
      noSession: 0.4,
      lanes: [
        { id: '/web', p: 0.7 },
        { id: '/api', p: 0.3 },
      ],
      ownWorktree: 0.8,
    });
    const s = suggestions(r, b);
    expect(s.map((x) => x.key)).toEqual(['start:new:P//web', 'start:new:P//api']);
    expect(s[0]).toMatchObject({ kind: 'start', newWorktree: true });
    // Two worktrees of one repo make one new-worktree row.
    const two = board([lane('/api', []), lane('/api-wt', [], '/api')]);
    expect(suggestions({ ...r, lanes: [{ id: '/api', p: 0.6 }, { id: '/api-wt', p: 0.4 }] }, two).map((x) => x.key)).toEqual(['start:new:P//api']);
  });

  it('offers the session too when sending is a real possibility', () => {
    const r = route({
      action: 'start',
      actions: [
        ['start', 0.6],
        ['send', 0.4],
      ],
      sessions: [{ id: 's:copy', p: 0.8 }],
      lanes: [{ id: '/web', p: 1 }],
    });
    expect(rows(r)).toEqual([
      ['start:/web', 0.6],
      ['send:s:copy', 0.32],
    ]);
  });

  it('finds sessions to open', () => {
    const r = route({ action: 'find', actions: [['find', 0.9]], sessions: [{ id: 's:copy', p: 0.9 }], lanes: [{ id: '/web', p: 1 }] });
    expect(suggestions(r, b).map((x) => [x.kind, x.key])).toEqual([['open', 'open:s:copy']]);
  });

  it('keeps a note for later as a todo, with starting on it as the runner-up', () => {
    const r = route({
      action: 'todo',
      actions: [
        ['todo', 0.8],
        ['start', 0.2],
      ],
      lanes: [{ id: '/api', p: 1 }],
    });
    expect(suggestions(r, b).map((x) => x.key)).toEqual(['todo:/api', 'start:/api']);
  });

  it('skips sessions and worktrees the board no longer has', () => {
    const r = route({ action: 'send', actions: [['send', 1]], sessions: [{ id: 'gone', p: 1 }], lanes: [{ id: '/gone', p: 1 }] });
    expect(suggestions(r, b)).toEqual([]);
  });
});

describe('sureness', () => {
  it('reads probabilities as words', () => {
    expect([sureness(0.9), sureness(0.5), sureness(0.2)]).toEqual(['sure', 'likely', 'unsure']);
  });
});

describe('worthRouting', () => {
  it('asks about a few words, not a search word', () => {
    expect(worthRouting('login')).toBe(false);
    expect(worthRouting('fix it')).toBe(false);
    expect(worthRouting('make the login button blue')).toBe(true);
  });
});

describe('todoTitle', () => {
  it('drops the lead-in and keeps the first line', () => {
    expect(todoTitle('todo: rotate the API keys\nmore')).toBe('Rotate the API keys');
    expect(todoTitle('later, look at flaky tests')).toBe('Look at flaky tests');
    expect(todoTitle('Remember to email Sam')).toBe('Email Sam');
  });
});

describe('duplicates', () => {
  it('offers sending to the session already on it, right after starting', () => {
    const r = route({
      action: 'start',
      actions: [['start', 0.9]],
      sessions: [],
      lanes: [
        { id: '/web', p: 0.7 },
        { id: '/api', p: 0.3 },
      ],
      duplicate: { id: 's:copy', p: 0.7 },
    });
    expect(suggestions(r, b).map((x) => x.key)).toEqual(['start:/web', 'send:s:copy', 'start:/api']);
    expect(duplicateOf(r, b)?.session.key).toBe('s:copy');
    expect(duplicateOf(route({ duplicate: { id: 'gone', p: 1 } }), b)).toBeNull();
  });
});

describe('rankedBy', () => {
  it('puts the sessions Jev scored first, in its order, and keeps the rest as found', () => {
    const found = ['a', 'b', 'c', 'd'].map((k) => ({ session: session(k, 'idle') }));
    const r = route({ sessions: [{ id: 'c', p: 0.6 }, { id: 'a', p: 0.3 }, { id: 'zz', p: 0.1 }] });
    expect(rankedBy(found, r).map((x) => x.session.key)).toEqual(['c', 'a', 'b', 'd']);
    expect(rankedBy(found, null).map((x) => x.session.key)).toEqual(['a', 'b', 'c', 'd']);
  });
});

