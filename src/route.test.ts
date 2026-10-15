import { describe, expect, it } from 'vitest';
import { agoPhrase } from './format';
import { alreadyOnWords, duplicateOf, rankedBy, resolveSuggestedAgent, suggestions, suggestionWords, sureness, todoTitle, worthRouting, type Suggestion, type SuggestionContext } from './route';
import { startHappens, startTitleWords, startWhereWords } from './start';
import * as fx from './testing/fixtures';
import type { Board, Lane, Route, Session } from './types';

const session = (key: string, status: Session['status'], extra: Partial<Session> = {}): Session => fx.session({ key, status, title: key, ...extra });
// Reusable unless it says otherwise, so new work can start in any of them.
const lane = (id: string, sessions: Session[], repo = id, extra: Partial<Lane> = { reuse: 'open' }): Lane => fx.lane({ id, sessions, callsign: id, repo, project: 'P', lastActivity: 0, ...extra });
const board = (lanes: Lane[]): Board => fx.board({ lanes });

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
    // And to one whose turn ended on a command in the background that looks stuck.
    const stuck = board([lane('/api', [session('s:ask', 'needs', { background: 'northwind login --device' })])]);
    expect(suggestions(r, stuck)[0]).toMatchObject({ kind: 'send', key: 'send:s:ask' });
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

describe('a question that needs no repo', () => {
  const chats = lane('/chats', [], 'Chats', { project: 'Elsewhere', startsChats: true, hidden: true });
  const withChats = board([...b.lanes, chats]);
  const start = (noRepo: number, lanes = [{ id: '/api', p: 0.3 }]) => route({ action: 'start', actions: [['start', 1]], lanes, noRepo });

  it('starts a chat of its own when Jev reads it as needing none of the worktrees', () => {
    expect(suggestions(start(0.6), withChats).map((x) => [x.key, x.lane.id])).toEqual([
      ['start:chat', '/chats'],
      ['start:/api', '/api'],
    ]);
    expect(suggestions(start(0.6), withChats)[0]).toMatchObject({ kind: 'start', newWorktree: false, p: 0.6 });
  });

  it('offers a chat after the worktree Jev likes better, when it’s likely too', () => {
    expect(suggestions(start(0.2, [{ id: '/api', p: 0.7 }]), withChats).map((x) => x.key)).toEqual(['start:/api', 'start:chat']);
    expect(suggestions(start(0.05, [{ id: '/api', p: 0.7 }]), withChats).map((x) => x.key)).toEqual(['start:/api']);
  });

  it('goes by the worktrees where the board has no chats, or the Mac said nothing of them', () => {
    expect(suggestions(start(0.6), b).map((x) => x.key)).toEqual(['start:/api']);
    expect(suggestions(route({ action: 'start', actions: [['start', 1]], lanes: [{ id: '/api', p: 0.3 }] }), withChats).map((x) => x.key)).toEqual(['start:/api']);
  });
});

describe('new work in reusable worktrees', () => {
  const start = (lanes: { id: string; p: number }[]) => route({ action: 'start', actions: [['start', 1]], noSession: 1, lanes });
  const kept = (id: string, repo: string, extra: Partial<Lane> = {}) => lane(id, [], repo, extra);
  const repo = board([
    kept('/app', 'app', { origin: 'main', reuse: 'open' }),
    kept('/app-billing', 'app'),
    kept('/app-2', 'app', { reuse: 'landed', lastActivity: 5 }),
    kept('/app-3', 'app', { reuse: 'free', lastActivity: 1 }),
    kept('/app-4', 'app', { reuse: 'free', lastActivity: 2 }),
    // A main checkout marked as kept for its own work, and a worktree of it.
    kept('/infra', 'infra', { origin: 'main' }),
    kept('/infra-tls', 'infra'),
  ]);

  it('goes to a free reusable worktree of the repo when Jev picks one kept for its own work', () => {
    // Free before landed, and of the free ones the latest active.
    expect(suggestions(start([{ id: '/app-billing', p: 0.9 }]), repo).map((x) => x.key)).toEqual(['start:/app-4']);
    // One Jev ranked leads, even landed.
    expect(suggestions(start([{ id: '/app-billing', p: 0.6 }, { id: '/app-2', p: 0.3 }]), repo)[0].key).toBe('start:/app-2');
  });

  it('is as sure of a worktree chosen in the repo as of the repo', () => {
    const r = { ...start([{ id: '/app-billing', p: 0.3 }]), repos: [{ id: 'P/app', p: 0.9 }] };
    expect(suggestions(r, repo)[0]).toMatchObject({ key: 'start:/app-4', p: 0.9 });
    // In the worktree Jev picked, as sure as of it.
    expect(suggestions({ ...r, lanes: [{ id: '/app-3', p: 0.3 }] }, repo)[0]).toMatchObject({ key: 'start:/app-3', p: 0.3 });
  });

  it('joins the work under way in a reusable worktree only when Jev is sure of it, and never in the main worktree', () => {
    const busy = board([...repo.lanes, kept('/app-5', 'app', { reuse: 'review', lastActivity: 9 })]);
    expect(suggestions(start([{ id: '/app-5', p: 0.8 }]), busy)[0]).toMatchObject({ key: 'start:/app-5', newWorktree: false });
    // Less sure, Jev matched the repo more than the work: a free one.
    expect(suggestions(start([{ id: '/app-5', p: 0.4 }]), busy)[0].key).toBe('start:/app-4');
    // The main worktree with work under way is the repo, not the work.
    expect(suggestions(start([{ id: '/app', p: 0.9 }]), repo)[0].key).toBe('start:/app-4');
  });

  it('starts in the main worktree when none is free, and in a new worktree only where there is no reusable one', () => {
    const none = board(repo.lanes.filter((l) => l.reuse !== 'free' && l.reuse !== 'landed'));
    expect(suggestions(start([{ id: '/app-billing', p: 0.9 }]), none)[0]).toMatchObject({ key: 'start:/app', newWorktree: false });
  });

  it('makes a new worktree from the main worktree when the repo has no reusable one', () => {
    const s = suggestions(start([{ id: '/infra-tls', p: 0.9 }]), repo);
    expect(s[0]).toMatchObject({ key: 'start:new:P/infra', newWorktree: true });
    expect(s[0].lane.id).toBe('/infra');
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

// A suggestion's row, pinned as it reads for each kind and each state its session can be in.
describe('suggestionWords', () => {
  const NOW = fx.NOW;
  const ctx = (over: Partial<SuggestionContext> = {}): SuggestionContext => ({ now: NOW, text: 'later: fix the login page', account: 'work', onModel: '', place: 'terminal', makesWorktree: true, terminal: 'Ghostty', behind: false, duplicate: null, ...over });
  const at = lane('/api', [], 'api', { reuse: 'open', callsign: 'api', project: 'P' });
  const send = (over: Partial<Session>): Suggestion => ({ kind: 'send', key: 'k', lane: at, session: fx.session({ title: 'Fix it', account: 'work', startedAt: null, ...over }), p: 1 });
  const open = (over: Partial<Session>): Suggestion => ({ kind: 'open', key: 'k', lane: at, session: fx.session({ title: 'Fix it', account: 'work', startedAt: null, ...over }), p: 1 });
  const start = (l: Lane, newWorktree = false): Suggestion => ({ kind: 'start', key: 'k', lane: l, newWorktree, p: 1 });

  it('says a chat starts in a folder of its own', () => {
    const chats = lane('/chats', [], 'Chats', { project: 'Elsewhere', startsChats: true, callsign: 'Chats' });
    expect(suggestionWords(start(chats, true), ctx())).toEqual({ icon: 'message-circle', title: 'Start a chat in no project', why: 'Starts work in a folder of its own in Ghostty.', where: 'No project' });
    expect(suggestionWords(start(chats), ctx({ place: 'claude-app' })).why).toBe('The Claude app opens a new session in a folder of its own with this prompt filled in.');
  });

  it('says where a session is, and how old it is first', () => {
    const w = suggestionWords(send({ live: true, startedAt: NOW - 3 * 3_600_000 }), ctx());
    expect(w).toEqual({ icon: 'send', title: 'Send to “Fix it”', why: `Started ${agoPhrase(NOW - 3 * 3_600_000, NOW)}. It gets this as its next message.`, where: 'work · api' });
  });

  it('says how a sent prompt reaches the session', () => {
    expect(suggestionWords(send({ live: false, harness: 'claude', surface: 'cli' }), ctx()).why).toBe('It isn’t running: it resumes in a new terminal with this prompt.');
    expect(suggestionWords(send({ live: false, harness: 'codex', surface: 'cli' }), ctx()).why).toBe('It isn’t running: it resumes in a new terminal with this prompt.');
    expect(suggestionWords(send({ live: false, harness: 'claude', surface: 'app' }), ctx()).why).toBe('It isn’t running: it opens with this prompt ready to send.');
    expect(suggestionWords(send({ live: true, status: 'working' }), ctx()).why).toBe('It’s working, so this waits for its turn to end.');
    expect(suggestionWords(send({ live: true, harness: 'claude', surface: 'app' }), ctx()).why).toBe('It gets this as a message from another session, in the Claude app.');
  });

  it('answers a session that waits on you, and opens one that doesn’t', () => {
    expect(suggestionWords(open({ status: 'needs' }), ctx())).toEqual({ icon: 'external', title: 'Answer “Fix it”', why: 'It’s waiting on you, so Codealong shows it and you answer there.', where: 'work · api' });
    expect(suggestionWords(open({ headline: '`work` asks you · Fix the test' }), ctx())).toEqual({ icon: 'external', title: 'Open “Fix it”', why: 'asks you · Fix the test', where: 'work · api' });
  });

  it('saves a todo titled from what’s typed', () => {
    expect(suggestionWords({ kind: 'todo', key: 'k', lane: at, p: 1 }, ctx())).toEqual({ icon: 'list-check', title: 'Save as a todo: “Fix the login page”', why: 'In P, linked to api. Start it when you’re ready.', where: 'api' });
  });

  it('says how new work starts in each place', () => {
    const free = lane('/api', [], 'api', { reuse: 'free', callsign: 'api', project: 'P' });
    expect(suggestionWords(start(free), ctx({ onModel: ' on Opus' }))).toEqual({ icon: 'plus', title: 'Start in api', why: 'Starts work on Opus in api in Ghostty. It holds no work now.', where: 'P · api' });
    expect(suggestionWords(start(free), ctx({ place: 'background' })).why).toBe('Starts work in api in the background. It holds no work now.');
    expect(suggestionWords(start(free), ctx({ place: 'codealong', account: null })).why).toBe('Codealong runs a session in api. It holds no work now.');
    expect(suggestionWords(start(free), ctx({ place: 'claude-app' })).why).toBe('The Claude app opens a new session in api with this prompt filled in. It holds no work now.');
    expect(suggestionWords(start(free), ctx({ place: 'codex-app' })).why).toBe('The Codex app starts a new thread in api on this prompt. It holds no work now.');
    expect(suggestionWords(start(free), ctx({ place: 'background', behind: true })).why).toBe('Starts work in api in Ghostty, behind your windows. It holds no work now.');
  });

  // So the start ⌘⏎ makes can be read against the suggestions above it.
  it('says a start as the start row says the same start', () => {
    const main = lane('/api', [], 'api', { reuse: 'free', callsign: 'api', project: 'P', origin: 'main' });
    const said = suggestionWords(start(main), ctx({ place: 'background', onModel: ' on Opus' }));
    const row = startHappens({ lane: main, account: 'work', place: 'background', mac: null, reading: false, newWorktree: false, onModel: ' on Opus', prompted: true, terminal: 'Ghostty', behind: false });
    expect(said).toEqual({ icon: 'plus', title: 'Start in api (main)', why: 'Starts work on Opus in api in the background. It holds no work now.', where: 'P · api' });
    expect(row).toBe('Starts work on Opus in api in the background. Attach a terminal any time.');
    expect([startTitleWords(main, false), startWhereWords(main)]).toEqual([said.title, said.where]);
  });

  it('makes a new worktree only where the harness does', () => {
    const main = lane('/api', [], 'api', { reuse: 'free', callsign: 'api', project: 'P', origin: 'main' });
    expect(suggestionWords(start(main, true), ctx({ place: 'background' }))).toEqual({ icon: 'plus', title: 'Start in a new worktree of api', why: 'Claude Code makes a new worktree of api and starts work there in the background.', where: 'P · api' });
    expect(suggestionWords(start(main, true), ctx({ makesWorktree: false }))).toEqual({ icon: 'plus', title: 'Start in api (main)', why: 'Starts work in api in Ghostty. It holds no work now.', where: 'P · api' });
  });

  it('says what the worktree holds, and warns first of a session that may be on it', () => {
    const with_ = (reuse: Lane['reuse'], origin: Lane['origin'] = 'script') => lane('/api', [], 'api', { reuse, origin, callsign: 'api', project: 'P' });
    expect(suggestionWords(start(with_('landed')), ctx()).why).toBe('Starts work in api in Ghostty. Its last work landed.');
    expect(suggestionWords(start(with_('open')), ctx()).why).toBe('Starts work in api in Ghostty. It joins the work under way there.');
    expect(suggestionWords(start(with_('open', 'main')), ctx()).why).toBe('Starts work in api in Ghostty.');
    expect(suggestionWords(start(with_(undefined)), ctx()).why).toBe('Starts work in api in Ghostty.');
    expect(suggestionWords(start(with_('free')), ctx({ duplicate: { session: { title: 'Login work' } } })).why).toBe('“Login work” may already be on this. Starts work in api in Ghostty. It holds no work now.');
  });
});

describe('alreadyOnWords', () => {
  it('names the session that may already be doing it, or nothing', () => {
    expect(alreadyOnWords({ lane: { callsign: 'api' }, session: { title: 'Login work', account: 'work' } })).toBe(' “Login work” (work · api) may already be doing this.');
    expect(alreadyOnWords(null)).toBe('');
  });
});

describe('resolveSuggestedAgent', () => {
  const ghostty = { place: 'terminal' as const, label: 'Ghostty' };
  const accounts = [
    fx.startAccount({ name: 'claude', places: [ghostty], models: [{ id: 'opus', label: 'Opus' }] }),
    fx.startAccount({ name: 'claude2', places: [ghostty], models: [{ id: 'opus', label: 'Opus' }, { id: 'sonnet', label: 'Sonnet' }] }),
    fx.startAccount({ name: 'codex', harness: 'codex', places: [ghostty], models: [{ id: 'gpt-5', label: 'GPT-5' }] }),
  ];

  it('attaches Jev’s top agent pick to start suggestions and resolves it to an account with room', () => {
    const r = route({
      action: 'start',
      actions: [['start', 1]],
      lanes: [{ id: '/api', p: 1 }],
      agents: [{ account: 'claude', model: 'opus', p: 0.8 }],
    });
    expect(suggestions(r, b)[0]).toMatchObject({ kind: 'start', agent: 'claude:opus' });
    // claude is near its limit, so resolveSuggestedAgent picks claude2 on opus.
    const u = [fx.usage({ account: 'claude', meters: [fx.meter({ key: 'week', percent: 95, severity: 'critical' })] }), fx.usage({ account: 'claude2', meters: [fx.meter({ key: 'week', percent: 20 })] })];
    expect(resolveSuggestedAgent('claude:opus', accounts, [], u, fx.NOW)).toEqual({ account: 'claude2', model: 'opus' });
    // A routing group without a model resolves through pickFromGroup.
    const rGroup = route({
      action: 'start',
      actions: [['start', 1]],
      lanes: [{ id: '/api', p: 1 }],
      agents: [{ account: 'fast', model: null, p: 0.9 }],
    });
    expect(suggestions(rGroup, b)[0]).toMatchObject({ kind: 'start', agent: 'fast' });
    const groups = [{ name: 'fast', strategy: 'usage' as const, members: [{ account: 'claude2', model: 'sonnet' }] }];
    expect(resolveSuggestedAgent('fast', accounts, groups, u, fx.NOW)).toEqual({ account: 'claude2', model: 'sonnet' });
  });
});

