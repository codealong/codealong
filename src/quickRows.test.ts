import { describe, expect, it } from 'vitest';
import type { PrRow, ReviewRow, SessionRow, TodoRow } from './quick';
import { deskPin, deskTitle, rowSections, suggestRows, type MoreRow, type Row, type SendToRow, type SuggestRow } from './quickRows';
import type { Suggestion } from './route';
import type { SkillRow } from './skills';
import * as fx from './testing/fixtures';
import { todoItem } from './todos';
import type { Route } from './types';

// Northwind's app has Kestrel, which needs you, and Wren, whose turn ended.
const KESTREL = 'claude:northwind:kestrel';
const WREN = 'claude:northwind:wren';
const kestrel = fx.session({ key: KESTREL, title: 'Kestrel', status: 'needs', live: true });
const wren = fx.session({ key: WREN, title: 'Wren', status: 'unread' });
const app = fx.lane({ id: '/r/northwind/app', project: 'Northwind', repo: 'app', repoSlug: 'northwind/app', origin: 'main', sessions: [kestrel, wren] });
const board = fx.board({ lanes: [app] });

const sessionRow = (over: Partial<SessionRow> = {}): SessionRow => ({ kind: 'session', key: KESTREL, group: 'needs', lane: app, session: kestrel, marks: [], ...over });
const pr = fx.prItem({ repoSlug: 'northwind/app', number: 12, title: 'Kestrel login' });
const reviewRow: ReviewRow = { kind: 'review', key: pr.key, group: 'reviews', pr, marks: [] };
const prRow: PrRow = { kind: 'pr', key: `pr:${pr.key}`, group: 'prs', pr, marks: [] };
const todo = fx.todo({ id: 't1', title: 'Ship the login' });
const todoRow = (over: Partial<TodoRow> = {}): TodoRow => ({ kind: 'todo', key: 'todo:t1', group: 'todos', item: todoItem(todo, new Map(), new Map(), new Map()), waiting: false, marks: [], ...over });
const suggestRow = (s: Suggestion): SuggestRow => ({ kind: 'suggest', key: s.key, group: 'suggested', s });
const moreRow: MoreRow = { kind: 'more', key: 'more:found', group: 'found', count: 3 };
const sendToRow = (group: SendToRow['group']): SendToRow => ({ kind: 'sendto', key: 'sendto', group, count: 2 });
const skillRow: SkillRow = { kind: 'skill', key: 'skill:@:deploy', group: 'skills', skill: { name: 'deploy', description: 'Ships what’s merged', typed: true, byModel: false, written: {}, uses: 0, lastUsed: 0 }, lane: null, accounts: ['northwind'], marks: [] };

/** Jev's answer: likeliest, send it to Wren; else start new work in the app. */
const ROUTE: Route = {
  action: 'send',
  actions: [
    ['send', 0.6],
    ['start', 0.4],
  ],
  sessions: [{ id: WREN, p: 0.6 }],
  noSession: 0.4,
  lanes: [{ id: app.id, p: 0.4 }],
  repos: [{ id: 'Northwind/app', p: 0.4 }],
  ownWorktree: 0,
  ownWorktreeAsked: 0,
  habit: { new: 0, of: 0 },
  duplicate: null,
  model: 'jev',
  ms: 5,
  inputTokens: 1,
};

describe('suggestRows', () => {
  it('lists nothing before the board or Jev’s answer is in', () => {
    expect([suggestRows(null, board, false, ''), suggestRows(ROUTE, null, false, '')]).toEqual([[], []]);
  });

  it('makes a row of each of Jev’s suggestions, keyed as the suggestion is, under Suggested', () => {
    const rows = suggestRows(ROUTE, board, false, '');
    expect(rows.length).toBeGreaterThan(1);
    expect(rows.every((r) => r.kind === 'suggest' && r.group === 'suggested' && r.key === r.s.key)).toBe(true);
    expect(rows[0].s).toMatchObject({ kind: 'send', session: { key: WREN } });
    expect(rows.some((r) => r.s.kind === 'start')).toBe(true);
  });

  it('suggests no start while a session would start on another machine, whose worktrees Jev doesn’t know', () => {
    const here = suggestRows(ROUTE, board, false, '');
    const elsewhere = suggestRows(ROUTE, board, true, '');
    expect(elsewhere.map((r) => r.key)).toEqual(here.filter((r) => r.s.kind !== 'start').map((r) => r.key));
    expect(elsewhere.length).toBeLessThan(here.length);
  });
});

describe('what a row dragged out of the panel pins to the desktop', () => {
  const send: Suggestion = { kind: 'send', key: 'send:wren', lane: app, session: wren, p: 0.6 };
  const open: Suggestion = { kind: 'open', key: 'open:kestrel', lane: app, session: kestrel, p: 0.3 };
  const start: Suggestion = { kind: 'start', key: 'start:app', lane: app, newWorktree: false, p: 0.4 };
  const save: Suggestion = { kind: 'todo', key: 'todo:app', lane: app, p: 0.1 };

  it('is its session, todo or PR, a suggestion’s being the session it names', () => {
    const pinned = (r: Row | undefined) => [deskPin(r), deskTitle(r)];
    expect(pinned(sessionRow())).toEqual([{ ref: 'session', key: KESTREL }, 'Kestrel']);
    expect(pinned(suggestRow(send))).toEqual([{ ref: 'session', key: WREN }, 'Wren']);
    expect(pinned(suggestRow(open))).toEqual([{ ref: 'session', key: KESTREL }, 'Kestrel']);
    expect(pinned(todoRow())).toEqual([{ ref: 'todo', key: 't1' }, 'Ship the login']);
    expect(pinned(reviewRow)).toEqual([{ ref: 'pr', key: pr.key }, '#12 Kestrel login']);
    expect(pinned(prRow)).toEqual([{ ref: 'pr', key: pr.key }, '#12 Kestrel login']);
  });

  it('is nothing for what has no card: a start, a todo to save, more results, Send to, a skill, no row at all', () => {
    for (const r of [suggestRow(start), suggestRow(save), moreRow, sendToRow('found'), skillRow, undefined]) expect([deskPin(r), deskTitle(r)], r?.key).toEqual([null, '']);
  });

  it('is nothing for a session on another device or an archived one, though the pointer still holds its title', () => {
    const away = fx.lane({ ...app, device: fx.deviceView({ name: 'Mac mini' }) });
    expect([deskPin(sessionRow({ lane: away })), deskTitle(sessionRow({ lane: away }))]).toEqual([null, 'Kestrel']);
    const archived = { ...kestrel, archived: true };
    expect([deskPin(sessionRow({ session: archived })), deskTitle(sessionRow({ session: archived }))]).toEqual([null, 'Kestrel']);
  });

  it('calls a todo with no title Untitled', () => {
    expect(deskTitle(todoRow({ item: todoItem({ ...todo, title: '' }, new Map(), new Map(), new Map()) }))).toBe('Untitled');
  });
});

describe('rowSections', () => {
  const groups = (rows: Row[], how: Partial<Parameters<typeof rowSections>[1]> = {}) => rowSections(rows, { routed: false, searching: false, inSendTo: false, ...how }).map((s) => [s.group, s.rows.map((r) => r.key)]);
  const found = sessionRow({ group: 'found' });
  const foundToo = sessionRow({ key: WREN, session: wren, group: 'found' });

  it('puts each run of rows of one group under its heading, in the rows’ order', () => {
    expect(groups([sessionRow(), reviewRow, sessionRow({ key: WREN, session: wren, group: 'unread' }), todoRow({ group: 'ready' })])).toEqual([
      ['needs', [KESTREL]],
      ['reviews', [pr.key]],
      ['unread', [WREN]],
      ['ready', ['todo:t1']],
    ]);
    expect(groups([found, foundToo, moreRow, todoRow(), prRow])).toEqual([
      ['found', [KESTREL, WREN, 'more:found']],
      ['todos', ['todo:t1']],
      ['prs', [`pr:${pr.key}`]],
    ]);
    expect(groups([])).toEqual([]);
  });

  it('keeps Suggested there, first, while what’s typed goes to Jev, with nothing suggested yet', () => {
    expect(groups([found], { routed: true, searching: true })).toEqual([
      ['suggested', []],
      ['found', [KESTREL]],
    ]);
    // Once something is suggested it's the suggestions' own heading.
    expect(groups([sendToRow('suggested'), found], { routed: true, searching: true })).toEqual([
      ['suggested', ['sendto']],
      ['found', [KESTREL]],
    ]);
  });

  it('keeps Sessions there while there’s a search that found none, after Suggested and before what else was found', () => {
    expect(groups([], { searching: true })).toEqual([['found', []]]);
    expect(groups([todoRow()], { searching: true })).toEqual([
      ['found', []],
      ['todos', ['todo:t1']],
    ]);
    expect(groups([todoRow()], { routed: true, searching: true })).toEqual([
      ['suggested', []],
      ['found', []],
      ['todos', ['todo:t1']],
    ]);
  });

  it('in Send to, has Send to’s heading with no session found, and never Suggested', () => {
    expect(groups([], { inSendTo: true, routed: true })).toEqual([['sendto', []]]);
    expect(groups([sessionRow({ group: 'sendto' })], { inSendTo: true, routed: true })).toEqual([['sendto', [KESTREL]]]);
  });
});
