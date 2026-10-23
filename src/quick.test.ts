import { afterEach, describe, expect, it } from 'vitest';
import { agoPhrase } from './format';
import { ALL_PROJECTS, attachmentsNote, attentionRows, draftCodec, macUnavailable, nextDraft, primaryRunningWords, queueRows, recallList, startTitle, todosStayNote, withPrompt, findPrs, findSessions, findTodos, heldSaid, laneGroups, projectCounts, readyTodos, recentRows, sendTargets, shortcutFromKey, shortcutKeys, shortcutLabel, stillSaid, TAKEN, takenFor, titleMarks, worktreeCount, deviceHits } from './quick';
import { setPlatform } from './platform';
import * as fx from './testing/fixtures';
import type { Board, Label, Lane, PrItem, Session, Todo } from './types';

const NOW = 1_790_200_000_000;

const session = (key: string, status: Session['status'], at: number, extra: Partial<Session> = {}): Session => fx.session({ key, status, statusSince: at, lastActivity: at, title: key, ...extra });
const lane = (id: string, sessions: Session[], extra: Partial<Lane> = {}): Lane => fx.lane({ id, sessions, branch: id, lastActivity: 0, ...extra });
const board = (lanes: Lane[], todos: Todo[] = [], labels: Label[] = []): Board => fx.board({ lanes, todos, labels });
const todo = (id: string, title: string, extra: Partial<Todo> = {}): Todo => fx.todo({ id, project: 'Acme', title, createdAt: NOW - 1000, updatedAt: NOW - 1000, ...extra });

describe('queueRows', () => {
  const b = board([
    lane('app2', [session('n1', 'needs', NOW - 90), session('w', 'working', NOW), session('u1', 'unread', NOW - 10)]),
    lane('app3', [session('n2', 'needs', NOW - 50), session('e', 'error', NOW - 70), session('i', 'idle', NOW)]),
  ]);
  const queue = queueRows(b, ALL_PROJECTS);
  const keys = (rows: { key: string }[]) => rows.map((r) => r.key);

  it('works through what needs you, the longest wait first, then finished turns, never working ones', () => {
    expect(keys(queue)).toEqual(['n1', 'e', 'n2', 'u1']);
    expect(keys(queueRows(board([lane('app2', [session('w', 'working', NOW)])]), ALL_PROJECTS))).toEqual([]);
  });

  it('puts the PRs waiting on your review after what needs you, the longest wait first, snoozed ones left out', () => {
    const review = (number: number, over: Partial<PrItem> = {}) => fx.prItem({ number, key: `acme/app#${number}`, project: 'Acme', state: 'open', reviewRequested: true, updatedAt: NOW - number, ...over });
    // #7 is still being reviewed by Codealong's agent: it waits on you once that's done.
    const reviewing = { stage: 'reviewing' as const, head: 'abc1234', at: NOW - 10, rounds: 1, holds: true };
    const prs = [review(3, { requestedAt: NOW - 30 }), review(1, { requestedAt: NOW - 90 }), review(2, { snoozedUntil: NOW + 1000 }), review(4, { reviewRequested: false }), review(5, { state: 'merged' }), review(6, { project: 'Globex', requestedAt: NOW - 60 }), review(7, { requestedAt: NOW - 120, autoReview: reviewing })];
    const b2 = fx.board({ lanes: b.lanes, prs });
    expect(keys(queueRows(b2, ALL_PROJECTS))).toEqual(['n1', 'e', 'n2', 'acme/app#1', 'acme/app#6', 'acme/app#3', 'u1']);
    expect(keys(queueRows(b2, 'Acme'))).toEqual(['n1', 'e', 'n2', 'acme/app#1', 'acme/app#3', 'u1']);
    expect(queueRows(b2, ALL_PROJECTS).map((r) => r.kind)).toEqual(['session', 'session', 'session', 'review', 'review', 'review', 'session']);
  });
});

describe('attentionRows', () => {
  it('lists what needs you, then finished turns, each the longest wait first', () => {
    const b = board([
      lane('app2', [session('w', 'working', NOW), session('u1', 'unread', NOW - 10), session('i', 'idle', NOW)]),
      lane('app3', [session('n', 'needs', NOW - 90), session('e', 'error', NOW - 5), session('u2', 'unread', NOW - 1)]),
    ]);
    const { rows, counts } = attentionRows(b, ALL_PROJECTS);
    expect(rows.map((r) => [r.key, r.group])).toEqual([
      ['n', 'needs'],
      ['e', 'needs'],
      ['u1', 'unread'],
      ['u2', 'unread'],
    ]);
    expect(counts).toEqual({ needs: 2, reviews: 0, unread: 2, working: 1 });
  });

  it('lists a helper only while it waits on you, as the floating bar does', () => {
    const helps = { helper: { parent: 'boss' } };
    const b = board([lane('app2', [session('boss', 'idle', NOW, { startedAt: NOW }), session('busy', 'working', NOW, { startedAt: NOW, ...helps }), session('asks', 'needs', NOW - 5, { startedAt: NOW, ...helps })])]);
    expect(attentionRows(b, ALL_PROJECTS).rows.map((r) => r.key)).toEqual(['asks']);
    expect(attentionRows(b, ALL_PROJECTS).counts.working).toBe(0);
    expect(recentRows(b, ALL_PROJECTS, new Set(), NOW).rows.map((r) => r.key)).toEqual(['boss', 'asks']);
  });

  it('leaves out sessions snoozed until their time, as the floating bar does, and lists them with the recent ones', () => {
    const later = { snoozedUntil: NOW + 60_000, startedAt: NOW - 100 };
    const b = board([lane('app2', [session('n', 'needs', NOW - 90), session('zn', 'needs', NOW - 95, later), session('zu', 'unread', NOW - 10, later), session('zw', 'working', NOW - 5, later)])]);
    const { rows, counts } = attentionRows(b, ALL_PROJECTS);
    expect(rows.map((r) => r.key)).toEqual(['n']);
    // One that runs still counts with Working.
    expect(counts).toEqual({ needs: 1, reviews: 0, unread: 0, working: 1 });
    expect(queueRows(b, ALL_PROJECTS).map((r) => r.key)).toEqual(['n']);
    expect(recentRows(b, ALL_PROJECTS, new Set(rows.map((r) => r.key)), NOW).rows.map((r) => r.key)).toEqual(['zw', 'zu', 'zn']);
    expect(projectCounts(b, [ALL_PROJECTS])).toEqual({ [ALL_PROJECTS]: { needs: 1, unread: 0 } });
    // With nothing else waiting, the running one is what's listed.
    const quiet = board([lane('app2', [session('zn', 'needs', NOW - 95, later), session('zw', 'working', NOW - 5, later)])]);
    expect(attentionRows(quiet, ALL_PROJECTS).rows.map((r) => r.key)).toEqual(['zw']);
  });

  it('lists the working sessions when nothing waits on you, the longest running first', () => {
    const b = board([lane('app2', [session('w2', 'working', NOW), session('w1', 'working', NOW - 5), session('i', 'idle', NOW)])]);
    expect(attentionRows(b, ALL_PROJECTS).rows.map((r) => r.key)).toEqual(['w1', 'w2']);
  });

  it('keeps to the project, leaves out visitors and archived sessions, and stops at the limit', () => {
    const b = board([
      lane('app2', [session('a', 'needs', NOW), session('v', 'needs', NOW, { visitor: 'worked here' }), session('x', 'unread', NOW, { archived: true })]),
      lane('globex', [session('s', 'needs', NOW)], { project: 'Globex' }),
      lane('stray', [session('h', 'needs', NOW)], { project: 'Elsewhere' }),
    ]);
    expect(attentionRows(b, 'Acme').rows.map((r) => r.key)).toEqual(['a']);
    // All projects has every project's, Elsewhere's too; Elsewhere only its own.
    expect(attentionRows(b, ALL_PROJECTS).rows.map((r) => r.key).sort()).toEqual(['a', 'h', 's']);
    expect(attentionRows(b, 'Elsewhere').rows.map((r) => r.key)).toEqual(['h']);
    const many = board([lane('app2', Array.from({ length: 12 }, (_, i) => session(`n${i}`, 'needs', NOW - i)))]);
    expect(attentionRows(many, ALL_PROJECTS).rows).toHaveLength(9);
    expect(attentionRows(many, ALL_PROJECTS).counts.needs).toBe(12);
  });
});

describe('findSessions', () => {
  const b = board([
    lane('app2', [
      session('s1', 'idle', NOW - 100, { title: 'Fix the login flake' }),
      session('s2', 'working', NOW - 50, { title: 'Login page redesign', account: 'claude2' }),
      session('s3', 'unread', NOW - 10, { title: 'Upgrade deps' }),
    ]),
    lane('login-flow', [session('s4', 'needs', NOW - 1000, { title: 'Tests for checkout' })], { branch: 'fix/login-flow' }),
  ]);

  it('needs every word somewhere: title, account, worktree, branch, repo or project', () => {
    expect(findSessions(b, ALL_PROJECTS, 'login', NOW).map((r) => r.key)).toEqual(['s2', 's1', 's4']);
    expect(findSessions(b, ALL_PROJECTS, 'login claude2', NOW).map((r) => r.key)).toEqual(['s2']);
    expect(findSessions(b, ALL_PROJECTS, 'LOGIN   flake', NOW).map((r) => r.key)).toEqual(['s1']);
    expect(findSessions(b, ALL_PROJECTS, 'acme upgrade', NOW).map((r) => r.key)).toEqual(['s3']);
    expect(findSessions(b, ALL_PROJECTS, 'nothing like it', NOW)).toEqual([]);
    expect(findSessions(b, ALL_PROJECTS, '   ', NOW)).toEqual([]);
  });

  it('puts titles with every word first, then the latest', () => {
    // s4 needs you but has "login" only in its branch; the titles that say it come first.
    expect(findSessions(b, ALL_PROJECTS, 'login', NOW)[2].key).toBe('s4');
    // Among titles with the word, the latest first.
    expect(findSessions(b, ALL_PROJECTS, 'login', NOW).slice(0, 2).map((r) => r.key)).toEqual(['s2', 's1']);
  });

  it('adds sessions found by what was said, after the ones that match otherwise', () => {
    const said = new Map([
      ['s3', { key: 's3', snippet: 'The login retries are in auth.ts', you: false, at: NOW }],
      ['s1', { key: 's1', snippet: 'login again', you: true, at: NOW }],
    ]);
    const rows = findSessions(b, ALL_PROJECTS, 'login', NOW, 9, said);
    expect(rows.map((r) => r.key)).toEqual(['s2', 's1', 's4', 's3']);
    // A session its title already found keeps its usual line.
    expect(rows[1].said).toBeUndefined();
    expect(rows[3].said).toEqual({ text: 'The login retries are in auth.ts', you: false, marks: [[4, 9]] });
  });

  it('ranks what was said by how well it matched', () => {
    const said = new Map([
      ['s4', { key: 's4', snippet: 'a checkout, and a class of tests', you: false, at: NOW, score: 4 }],
      ['s1', { key: 's1', snippet: 'against our class C status', you: true, at: NOW, score: 7 }],
    ]);
    // s4 needs you, but s1 said the words as typed.
    expect(findSessions(b, ALL_PROJECTS, 'class c', NOW, 9, said).map((r) => r.key)).toEqual(['s1', 's4']);
  });

  const HOUR = 60 * 60 * 1000;
  const DAY = 24 * HOUR;
  const aged = (key: string, age: number, title: string, status: Session['status'] = 'stopped') => session(key, status, NOW - age, { title });
  const mention = (key: string, score: number) => [key, { key, snippet: 'the harbor watch', you: false, at: NOW, score }] as const;

  it('puts the latest first among sessions that match as closely, whatever waits on you', () => {
    const b2 = board([lane('app2', [aged('asks', 8 * DAY, 'Login question', 'needs'), aged('open', 3 * DAY, 'Login idle', 'idle'), aged('done', 2 * HOUR, 'Login done')])]);
    expect(findSessions(b2, ALL_PROJECTS, 'login', NOW).map((r) => r.key)).toEqual(['done', 'open', 'asks']);
  });

  it('lets a recent session that matches less closely pass an old one', () => {
    const b2 = board([
      lane('app2', [aged('two-days', 2 * DAY, 'Harbor watch import'), aged('week', 7 * DAY, 'Harbor watch alerts'), aged('ten-days', 10 * DAY, 'Harbor watch export'), aged('busy', 0, 'Checkout tests', 'working'), aged('hour', HOUR, 'Upgrade deps')]),
      lane('harbor-watch', [aged('branch', HOUR, 'Release notes')]),
    ]);
    const said = new Map([mention('busy', 4), mention('hour', 4)]);
    // The branch an hour ago passes every title; a title two days old stays ahead of a session
    // working now that only mentions the words, and one ten days old falls behind a mention an
    // hour old.
    expect(findSessions(b2, ALL_PROJECTS, 'harbor watch', NOW, 9, said).map((r) => r.key)).toEqual(['branch', 'two-days', 'busy', 'week', 'hour', 'ten-days']);
  });

  it('puts a recent mention before an old one that said the words as typed', () => {
    const b2 = board([lane('app2', [aged('old', 21 * DAY, 'Checkout tests'), aged('new', HOUR, 'Upgrade deps')])]);
    const said = new Map([mention('old', 7), mention('new', 4)]);
    expect(findSessions(b2, ALL_PROJECTS, 'harbor watch', NOW, 9, said).map((r) => r.key)).toEqual(['new', 'old']);
  });

  it('takes a short word among others only whole', () => {
    // Every session's account is claude, which has a c; none has the word c.
    expect(findSessions(b, ALL_PROJECTS, 'login c', NOW)).toEqual([]);
    expect(findSessions(b, ALL_PROJECTS, 'c', NOW).length).toBe(4);
    expect(titleMarks('Class C audit', ['class', 'c'])).toEqual([
      [0, 5],
      [6, 7],
    ]);
  });

  it('looks through archived sessions only when asked, listing them after the others that match as closely', () => {
    const withGone = board([
      lane('app2', [
        session('s1', 'idle', NOW - 100, { title: 'Fix the login flake' }),
        session('gone', 'needs', NOW, { title: 'Login copy', archived: true }),
        session('gone2', 'idle', NOW - 5, { title: 'Old login work', archived: true }),
        session('mention', 'unread', NOW - 1, { title: 'Upgrade deps' }),
      ]),
    ]);
    const said = new Map([['mention', { key: 'mention', snippet: 'after the login change', you: false, at: NOW }]]);
    expect(findSessions(withGone, ALL_PROJECTS, 'login', NOW, 9, said).map((r) => r.key)).toEqual(['s1', 'mention']);
    // An archived title comes after the other, though it's the latest, and before the session
    // that only mentions the word.
    expect(findSessions(withGone, ALL_PROJECTS, 'login', NOW, 9, said, 'with').map((r) => r.key)).toEqual(['s1', 'gone', 'gone2', 'mention']);
    expect(findSessions(withGone, ALL_PROJECTS, 'login', NOW, 9, said, 'only').map((r) => r.key)).toEqual(['gone', 'gone2']);
  });

  it('puts the session whose title or name is the words first, then titles and names that have them', () => {
    const named = board([
      lane('app2', [
        session('quoted', 'needs', NOW, { title: 'Fix the kestrel import' }),
        session('longer', 'working', NOW - 10, { title: 'Harbor watch for northwind' }),
        session('by-name', 'idle', NOW - 20, { title: 'Release notes', name: 'Nightly harbor watch' }),
        // The words split between its title and its name: what describes it has them.
        session('split', 'unread', NOW - 5, { title: 'Harbor cleanup', name: 'Night watch' }),
        session('exact', 'stopped', NOW - 9000, { title: 'Harbor  Watch', archived: true }),
      ]),
    ]);
    const said = new Map([['quoted', { key: 'quoted', snippet: 'ask the harbor watch first', you: false, at: NOW }]]);
    expect(findSessions(named, ALL_PROJECTS, 'harbor watch', NOW, 9, said, 'with').map((r) => r.key)).toEqual(['exact', 'longer', 'by-name', 'split', 'quoted']);
    // A name that is the words is as close as a title that is.
    expect(findSessions(named, ALL_PROJECTS, 'nightly harbor watch', NOW, 9, said, 'with')[0].key).toBe('by-name');
  });

  it('finds sessions in other projects than the one picked, after its own, Elsewhere’s too', () => {
    const HOUR = 3600e3;
    const projects = board([
      lane('app2', [session('mine', 'idle', NOW - 2 * HOUR, { title: 'Login retries' }), session('mention', 'unread', NOW - 1, { title: 'Upgrade deps' })]),
      lane('globex', [session('theirs', 'idle', NOW - 10, { title: 'Login page' })], { project: 'Globex' }),
      lane('stray', [session('stray', 'idle', NOW - 20, { title: 'Login stray' })], { project: 'Elsewhere' }),
      // A folder elsewhere with only archived sessions is out of sight, but they're still found.
      lane('gone', [session('gone', 'idle', NOW - 30, { title: 'Login gone', archived: true })], { project: 'Elsewhere', hidden: true }),
    ]);
    const said = new Map([['mention', { key: 'mention', snippet: 'after the login change', you: true, at: NOW, score: 7 }]]);
    const found = findSessions(projects, 'Acme', 'login', NOW, 9, said);
    // Globex's is newer, but Acme is the project picked; a title elsewhere still comes before a
    // mention in Acme.
    expect(found.map((r) => r.key)).toEqual(['mine', 'theirs', 'stray', 'mention']);
    // Its row carries its worktree, which says its project.
    expect(found[1].lane.project).toBe('Globex');
    expect(findSessions(projects, ALL_PROJECTS, 'login', NOW, 9, said).map((r) => r.key)).toEqual(['theirs', 'stray', 'mine', 'mention']);
    expect(findSessions(projects, 'Elsewhere', 'login', NOW, 9, said).map((r) => r.key)).toEqual(['stray', 'theirs', 'mine', 'mention']);
    expect(findSessions(projects, ALL_PROJECTS, 'login gone', NOW, 9, new Map(), 'with').map((r) => r.key)).toEqual(['gone']);
    expect(findSessions(projects, 'Globex', 'retries', NOW).map((r) => r.key)).toEqual(['mine']);
  });

  it('marks where the words are in the title', () => {
    expect(findSessions(b, ALL_PROJECTS, 'login flake', NOW)[0].marks).toEqual([
      [8, 13],
      [14, 19],
    ]);
  });
});

describe('sendTargets', () => {
  const b = board([
    lane('app2', [
      session('old', 'stopped', NOW - 9000, { title: 'Fix the login flake', lastPromptAt: NOW - 9000 }),
      session('busy', 'working', NOW - 1, { title: 'Login page redesign', lastPromptAt: NOW - 500 }),
      session('done', 'unread', NOW - 10, { title: 'Upgrade deps', lastPromptAt: NOW - 100 }),
      session('helper', 'working', NOW, { title: 'Review the login change', lastPromptAt: NOW, helper: { parent: 'busy' } }),
      session('gone', 'idle', NOW, { title: 'Old login work', lastPromptAt: NOW, archived: true }),
    ]),
    lane('other', [session('elsewhere', 'idle', NOW - 50, { title: 'Checkout tests', lastPromptAt: NOW - 50 })], { project: 'Globex' }),
  ]);

  it('lists Jev’s picks first, in its order with how likely each is, then the rest by your last prompt', () => {
    const picks = [
      { id: 'old', p: 0.6 },
      { id: 'done', p: 0.2 },
    ];
    const { rows, count } = sendTargets(b, ALL_PROJECTS, '', picks, NOW);
    expect(rows.map((r) => [r.key, r.p])).toEqual([
      ['old', 0.6],
      ['done', 0.2],
      ['elsewhere', undefined],
      ['busy', undefined],
    ]);
    expect(count).toBe(4);
    expect(rows.every((r) => r.group === 'sendto')).toBe(true);
  });

  it('leaves out helpers that don’t need you, and archived sessions unless asked, last', () => {
    expect(sendTargets(b, ALL_PROJECTS, '', [], NOW).rows.map((r) => r.key)).not.toContain('helper');
    expect(sendTargets(b, ALL_PROJECTS, '', [], NOW, new Map(), 'with').rows.map((r) => r.key)).toEqual(['elsewhere', 'done', 'busy', 'old', 'gone']);
    expect(sendTargets(b, ALL_PROJECTS, '', [], NOW, new Map(), 'only').count).toBe(1);
  });

  it('finds sessions by what’s typed in its box, what was said in them too, Jev’s picks first', () => {
    const said = new Map([['done', { key: 'done', snippet: 'the login retries', you: false, at: NOW }]]);
    expect(sendTargets(b, ALL_PROJECTS, 'login', [], NOW, said).rows.map((r) => r.key)).toEqual(['busy', 'old', 'done']);
    // An archived one whose title has the words comes before one that only mentions them.
    expect(sendTargets(b, ALL_PROJECTS, 'login', [], NOW, said, 'with').rows.map((r) => r.key)).toEqual(['busy', 'old', 'gone', 'done']);
    const picked = sendTargets(b, ALL_PROJECTS, 'login', [{ id: 'done', p: 0.4 }], NOW, said).rows;
    expect(picked.map((r) => r.key)).toEqual(['done', 'busy', 'old']);
    expect(picked[0].said?.text).toBe('the login retries');
  });

  it('doesn’t take a session archived since Jev answered for a pick', () => {
    // The panel keeps Jev's answer while the board changes under it.
    const stale = [{ id: 'gone', p: 0.7 }];
    const empty = sendTargets(b, ALL_PROJECTS, '', stale, NOW, new Map(), 'with').rows;
    expect(empty.map((r) => r.key)).toEqual(['elsewhere', 'done', 'busy', 'old', 'gone']);
    expect(empty[4].p).toBeUndefined();
    expect(sendTargets(b, ALL_PROJECTS, 'login', stale, NOW, new Map(), 'with').rows.map((r) => r.key)).toEqual(['busy', 'old', 'gone']);
  });

  it('keeps to the project, and lists at most the limit while counting them all', () => {
    expect(sendTargets(b, 'Globex', '', [], NOW).rows.map((r) => r.key)).toEqual(['elsewhere']);
    // Words typed find sessions as a search does, in the other projects too.
    expect(sendTargets(b, 'Globex', 'login', [], NOW).rows.map((r) => r.key)).toEqual(['busy', 'old']);
    const few = sendTargets(b, ALL_PROJECTS, '', [], NOW, new Map(), 'without', 2);
    expect(few.rows.length).toBe(2);
    expect(few.count).toBe(4);
  });
});

describe('findTodos', () => {
  const lanes = [lane('app2', [session('s1', 'working', NOW, { title: 'Wire the MFA check', account: 'claude2' })])];
  const labels: Label[] = [{ id: 'l1', name: 'security', color: 'red' }];
  const todos = [
    todo('t1', 'Turn off MFA fallbacks', { updatedAt: NOW - 50 }),
    todo('t2', 'Rotate keys', { description: 'After the MFA work lands', status: 'in-progress', updatedAt: NOW - 500 }),
    todo('t3', 'Audit admin roles', { labels: ['l1'] }),
    todo('t4', 'Old MFA notes', { status: 'done', doneAt: NOW, updatedAt: NOW }),
    todo('t5', 'MFA for Globex', { project: 'Globex' }),
    todo('t6', 'Ship the MFA page', { waiting: { note: 'design review', since: NOW } }),
    todo('t7', 'Clean up', { sessions: ['s1'] }),
    todo('t8', 'Archived MFA idea', { archivedAt: NOW }),
  ];
  const b = board(lanes, todos, labels);

  it('needs every word in the title, description, labels, project, or linked worktrees and sessions', () => {
    expect(findTodos(b, 'Acme', 'security').map((r) => r.key)).toEqual(['todo:t3']);
    expect(findTodos(b, 'Acme', 'claude2 wire').map((r) => r.key)).toEqual(['todo:t7']);
    expect(findTodos(b, 'Acme', 'app2').map((r) => r.key)).toEqual(['todo:t7']);
    expect(findTodos(b, 'Acme', 'nothing like it')).toEqual([]);
  });

  it('puts titles with every word first, then in progress, ready, waiting and done, and keeps to the project', () => {
    // t2 has MFA only in its description and t7 only in its session's title, so they come after
    // the titles, in progress before ready. Archived t8 isn't found.
    expect(findTodos(b, 'Acme', 'mfa').map((r) => r.key)).toEqual(['todo:t1', 'todo:t6', 'todo:t4', 'todo:t2', 'todo:t7']);
    expect(findTodos(b, ALL_PROJECTS, 'mfa').map((r) => r.key)).toContain('todo:t5');
    expect(findTodos(b, 'Globex', 'mfa').map((r) => r.key)).toEqual(['todo:t5']);
  });

  it('says which are waiting and where the words are', () => {
    const [first, waiting] = findTodos(b, 'Acme', 'mfa');
    expect(first.waiting).toBe(false);
    expect(waiting.waiting).toBe(true);
    expect(first.marks).toEqual([[9, 12]]);
    expect(first.item.todo.id).toBe('t1');
    expect(findTodos(b, 'Acme', 'clean')[0].item.sessions.map((x) => x.session.key)).toEqual(['s1']);
  });
});

describe('recentRows', () => {
  const HOUR = 60 * 60 * 1000;
  it('lists sessions started in the last day that the panel doesn’t already show, the latest active first', () => {
    const b = board([
      lane('app2', [
        session('old', 'idle', NOW - HOUR, { startedAt: NOW - 30 * HOUR }),
        session('i1', 'idle', NOW - 3 * HOUR, { startedAt: NOW - 5 * HOUR }),
        session('i2', 'stopped', NOW - HOUR, { startedAt: NOW - 2 * HOUR }),
        session('u', 'unread', NOW, { startedAt: NOW - HOUR }),
        session('gone', 'idle', NOW, { startedAt: NOW - HOUR, archived: true }),
        session('guess', 'idle', NOW - 2 * HOUR, { startedAt: null }),
      ]),
    ]);
    const { rows, count } = recentRows(b, ALL_PROJECTS, new Set(['u']), NOW);
    expect(rows.map((r) => [r.key, r.group])).toEqual([
      ['i2', 'recent'],
      ['guess', 'recent'],
      ['i1', 'recent'],
    ]);
    expect(count).toBe(3);
    expect(recentRows(b, ALL_PROJECTS, new Set(), NOW, 2)).toMatchObject({ count: 4, rows: [{ key: 'u' }, { key: 'i2' }] });
  });
});

describe('readyTodos', () => {
  const lanes = [lane('app2', [session('s1', 'idle', NOW)]), lane('app3', [])];
  const todos = [
    todo('t1', 'Started', { status: 'in-progress', sessions: ['s1'] }),
    todo('t2', 'First ready', { worktrees: ['app3'] }),
    todo('t3', 'Marked waiting', { waiting: { note: 'a reply', since: NOW } }),
    todo('t4', 'Blocked', { dependsOn: ['t2'] }),
    todo('t5', 'Second ready', { dependsOn: ['t6'] }),
    todo('t6', 'Finished', { status: 'done', doneAt: NOW }),
    todo('t7', 'Globex’s', { project: 'Globex' }),
    todo('t8', 'Third ready'),
    todo('t9', 'Fourth ready'),
  ];
  const b = board(lanes, todos);

  it('lists the first few todos ready to start, in your order, and counts them all', () => {
    const { rows, count } = readyTodos(b, 'Acme');
    expect(rows.map((r) => r.key)).toEqual(['todo:t2', 'todo:t5', 'todo:t8']);
    expect(rows.every((r) => r.group === 'ready' && !r.waiting)).toBe(true);
    expect(count).toBe(4);
    // Where each would start, for its row and for ⏎.
    expect(rows[0].item.lane?.id).toBe('app3');
  });

  it('keeps to the project, and takes a limit', () => {
    expect(readyTodos(b, 'Globex').rows.map((r) => r.key)).toEqual(['todo:t7']);
    expect(readyTodos(b, ALL_PROJECTS, 2)).toMatchObject({ count: 5, rows: [{ key: 'todo:t2' }, { key: 'todo:t5' }] });
    expect(readyTodos(board(lanes), ALL_PROJECTS)).toEqual({ rows: [], count: 0 });
  });
});

describe('titleMarks', () => {
  it('merges words that overlap, in title order', () => {
    expect(titleMarks('Login page redesign', ['page', 'log'])).toEqual([
      [0, 3],
      [6, 10],
    ]);
    expect(titleMarks('abcdef', ['bcd', 'cde'])).toEqual([[1, 5]]);
    expect(titleMarks('abc', ['x'])).toEqual([]);
  });
});

describe('projectCounts', () => {
  it('counts what needs you and finished turns per project', () => {
    const b = board([lane('a', [session('n', 'needs', NOW), session('u', 'unread', NOW), session('w', 'working', NOW)]), lane('s', [session('e', 'error', NOW)], { project: 'Globex' })]);
    expect(projectCounts(b, ['Acme', 'Globex', ALL_PROJECTS])).toEqual({
      Acme: { needs: 1, unread: 1 },
      Globex: { needs: 1, unread: 0 },
      [ALL_PROJECTS]: { needs: 2, unread: 1 },
    });
  });
});

describe('laneGroups', () => {
  it('puts pinned worktrees first, then repos by their latest activity, each main checkout first', () => {
    const lanes = [
      lane('a1', [], { repo: 'a', lastActivity: 5 }),
      lane('a', [], { repo: 'a', origin: 'main', lastActivity: 1 }),
      lane('b1', [], { repo: 'b', lastActivity: 9 }),
      lane('p', [], { repo: 'b', lastActivity: 0 }),
    ];
    expect(laneGroups(lanes, ['p', 'gone']).map((g) => [g.label, g.lanes.map((l) => l.id)])).toEqual([
      ['Pinned', ['p']],
      ['b', ['b1']],
      ['a', ['a']],
      // A repo's worktrees that are neither its main checkout nor reusable come after every
      // repo's, to be folded away; b's main checkout isn't listed, so b1 stays where it was.
      ['a', ['a1']],
    ]);
    expect(laneGroups(lanes, []).map((g) => g.more ?? false)).toEqual([false, false, true]);
    // Across projects, a repo's heading names its project.
    expect(laneGroups([lane('x', [], { repo: 'r' }), lane('y', [], { repo: 'r', project: 'Globex' })], []).map((g) => g.label).sort()).toEqual(['Acme · r', 'Globex · r']);
  });

  it('puts every main checkout and reusable worktree before the rest, the rest by repo in the same order', () => {
    const lanes = [
      lane('a', [], { repo: 'a', origin: 'main', lastActivity: 1 }),
      lane('a-old', [], { repo: 'a', lastActivity: 2 }),
      lane('a-reuse', [], { repo: 'a', reuse: 'free', lastActivity: 3 }),
      lane('a-new', [], { repo: 'a', lastActivity: 8 }),
      lane('b', [], { repo: 'b', origin: 'main', reuse: 'open', lastActivity: 5 }),
      lane('b1', [], { repo: 'b', lastActivity: 9 }),
    ];
    expect(laneGroups(lanes, []).map((g) => [g.label, g.more ?? false, g.lanes.map((l) => l.id)])).toEqual([
      ['b', false, ['b']],
      ['a', false, ['a', 'a-reuse']],
      ['b', true, ['b1']],
      ['a', true, ['a-new', 'a-old']],
    ]);
    // A repo whose main checkout is pinned still has its other worktrees folded away.
    expect(laneGroups(lanes, ['b']).map((g) => [g.label, g.more ?? false, g.lanes.map((l) => l.id)])).toEqual([
      ['Pinned', false, ['b']],
      ['a', false, ['a', 'a-reuse']],
      ['b', true, ['b1']],
      ['a', true, ['a-new', 'a-old']],
    ]);
  });
});

describe('shortcuts', () => {
  const key = (code: string, mods: Partial<Record<'metaKey' | 'ctrlKey' | 'altKey' | 'shiftKey', boolean>> = {}) => ({ code, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...mods });

  it('shows config.toml’s shortcuts the way macOS writes them', () => {
    expect(shortcutLabel('Option+Shift+Space')).toBe('⌥⇧Space');
    expect(shortcutLabel('Option+Cmd+/')).toBe('⌥⌘/');
    expect(shortcutLabel('Cmd+Ctrl+K')).toBe('⌃⌘K');
    expect(shortcutLabel('alt+`')).toBe('⌥`');
    expect(shortcutLabel('CommandOrControl+Shift+KeyP')).toBe('⇧⌘P');
    expect(shortcutLabel('ctrl+alt+Digit1')).toBe('⌃⌥1');
    expect(shortcutLabel('F18')).toBe('F18');
    expect(shortcutLabel('')).toBeNull();
    expect(shortcutLabel(undefined)).toBeNull();
    expect(shortcutLabel('Option+A+B')).toBeNull();
  });

  it('says a shortcut one key at a time for its keycaps', () => {
    expect(shortcutKeys('Option+Cmd+/')).toEqual(['⌥', '⌘', '/']);
    expect(shortcutKeys('Option+Shift+Space')).toEqual(['⌥', '⇧', 'Space']);
    expect(shortcutKeys('F18')).toEqual(['F18']);
    expect(shortcutKeys('')).toBeNull();
  });

  it('records a key press as config.toml writes it', () => {
    expect(shortcutFromKey(key('Space', { altKey: true, shiftKey: true }))).toEqual({ shortcut: 'Option+Shift+Space' });
    expect(shortcutFromKey(key('Slash', { altKey: true, metaKey: true }))).toEqual({ shortcut: 'Option+Cmd+/' });
    expect(shortcutFromKey(key('KeyK', { metaKey: true, ctrlKey: true }))).toEqual({ shortcut: 'Ctrl+Cmd+K' });
    expect(shortcutFromKey(key('Digit1', { ctrlKey: true, altKey: true }))).toEqual({ shortcut: 'Ctrl+Option+1' });
    expect(shortcutFromKey(key('F18'))).toEqual({ shortcut: 'F18' });
    expect(shortcutFromKey(key('Space', { metaKey: true }))).toEqual({ shortcut: 'Cmd+Space' });
    // What the label shows for a recorded one is what TAKEN is keyed by.
    expect(TAKEN[shortcutLabel('Cmd+Space')!]).toBe('Spotlight');
    expect(TAKEN[shortcutLabel((shortcutFromKey(key('Space', { altKey: true })) as { shortcut: string }).shortcut)!]).toContain('ChatGPT');
  });

  it('waits while only modifiers are down, and turns away keys that would take typing or menus', () => {
    expect(shortcutFromKey(key('AltLeft', { altKey: true }))).toBeNull();
    expect(shortcutFromKey(key('KeyK'))).toHaveProperty('error');
    expect(shortcutFromKey(key('KeyK', { shiftKey: true }))).toHaveProperty('error');
    expect(shortcutFromKey(key('KeyK', { metaKey: true }))).toHaveProperty('error');
    expect(shortcutFromKey(key('Tab', { metaKey: true, shiftKey: true }))).toHaveProperty('error');
    expect(shortcutFromKey(key('IntlBackslash', { altKey: true }))).toHaveProperty('error');
    // ⇧⌘ with a key is allowed: apps like Maccy ship on ⇧⌘C.
    expect(shortcutFromKey(key('KeyC', { metaKey: true, shiftKey: true }))).toEqual({ shortcut: 'Shift+Cmd+C' });
  });

  describe('on Linux', () => {
    afterEach(() => setPlatform(null));

    it('shows them as its keys are, in words: Cmd is Super, and CmdOrCtrl Ctrl', () => {
      setPlatform('linux');
      expect(shortcutLabel('Ctrl+Alt+/')).toBe('Ctrl+Alt+/');
      expect(shortcutLabel('Option+Cmd+/')).toBe('Alt+Super+/');
      expect(shortcutLabel('CommandOrControl+Shift+KeyP')).toBe('Ctrl+Shift+P');
      expect(shortcutLabel('Super+Enter')).toBe('Super+Enter');
      expect(shortcutLabel('Ctrl+Alt+Up')).toBe('Ctrl+Alt+↑');
      expect(shortcutKeys('Ctrl+Alt+/')).toEqual(['Ctrl', 'Alt', '/']);
      expect(heldSaid(key('ControlLeft', { ctrlKey: true, altKey: true }))).toBe('Ctrl+Alt');
    });

    it('records a key press with Linux’s names and its desktop’s keys turned away', () => {
      setPlatform('linux');
      expect(shortcutFromKey(key('Slash', { ctrlKey: true, altKey: true }))).toEqual({ shortcut: 'Ctrl+Alt+/' });
      expect(shortcutFromKey(key('KeyK', { ctrlKey: true, shiftKey: true }))).toEqual({ shortcut: 'Ctrl+Shift+K' });
      expect(shortcutFromKey(key('KeyK', { ctrlKey: true }))).toEqual({ error: 'Ctrl+K is a shortcut in most apps. Add Alt.' });
      expect(shortcutFromKey(key('Tab', { altKey: true }))).toHaveProperty('error');
      expect(shortcutFromKey(key('KeyK'))).toEqual({ error: 'Use Ctrl or Alt with it, or it would take a key you type.' });
      expect(shortcutFromKey(key('OSLeft'))).toBeNull();
      expect(takenFor(shortcutLabel('Alt+Space')!)).toBe('the window menu');
      expect(takenFor(shortcutLabel('Cmd+Space')!)).toBe('GNOME’s input source switching');
    });

    it('leaves macOS’s as they were', () => {
      setPlatform('mac');
      expect(heldSaid(key('ControlLeft', { ctrlKey: true, altKey: true }))).toBe('⌃⌥');
      expect(takenFor(shortcutLabel('Cmd+Space')!)).toBe('Spotlight');
    });
  });
});

describe('prompt history', () => {
  it('keeps the newest first, once each, and drops blanks', () => {
    expect(withPrompt(['b', 'a'], 'a')).toEqual(['a', 'b']);
    expect(withPrompt(['a'], '  ')).toEqual(['a']);
    expect(withPrompt(['c', 'b', 'a'], 'd', 3)).toEqual(['d', 'c', 'b']);
  });
  it('puts the draft before past prompts unless it is the last one', () => {
    expect(recallList('draft', ['a'])).toEqual(['draft', 'a']);
    expect(recallList('a', ['a', 'b'])).toEqual(['a', 'b']);
    expect(recallList(null, ['a'])).toEqual(['a']);
  });
  it('makes what is typed the draft, and keeps the last one when the input is emptied', () => {
    const kept = { text: 'fix the bar', todos: ['t1'] };
    expect(nextDraft('half a thought', [], kept)).toEqual({ text: 'half a thought', todos: [] });
    expect(nextDraft('', [], kept)).toBe(kept);
    expect(nextDraft('  ', ['t2'], null)).toBeNull();
  });
  it('keeps the draft across restarts, and nothing for no draft', () => {
    const d = { text: 'fix the bar', todos: ['t1'] };
    expect(draftCodec.parse(draftCodec.format(d)!)).toEqual(d);
    expect(draftCodec.format(null)).toBeNull();
    expect(draftCodec.format({ text: ' ', todos: [] })).toBeNull();
    expect(draftCodec.parse('{"text":"x","todos":["a",3]}')).toEqual({ text: 'x', todos: ['a'] });
    expect(draftCodec.parse('{"text":"x"}')).toEqual({ text: 'x', todos: [] });
    expect(draftCodec.parse('{"todos":[]}')).toBeUndefined();
    expect(draftCodec.parse('"x"')).toBeUndefined();
  });
  it('keeps Improve Codealong’s pictures with the draft written over them', () => {
    const shots = [{ kind: 'image' as const, path: 'images/aaa.png', name: 'Main window' }];
    expect(nextDraft('the bar is too tall', [], null, shots)).toEqual({ text: 'the bar is too tall', todos: [], shots });
    expect(nextDraft('', [], null, shots)).toBeNull();
    const d = { text: 'the bar is too tall', todos: [], shots };
    expect(draftCodec.parse(draftCodec.format(d)!)).toEqual(d);
    // A draft without pictures is written as it was before they were kept.
    expect(draftCodec.format(nextDraft('x', [], null, []))).toBe('{"text":"x","todos":[]}');
    const odd = '{"text":"x","shots":[{"kind":"image","path":"images/b.png","name":"Bar"},{"kind":"image","path":"/etc/passwd","name":"x"},{"kind":"file","path":"files/c/d","name":"d"},3]}';
    expect(draftCodec.parse(odd)).toEqual({ text: 'x', todos: [], shots: [{ kind: 'image', path: 'images/b.png', name: 'Bar' }] });
  });
});

describe('findPrs', () => {
  const pr = (number: number, title: string, extra: Partial<PrItem> = {}): PrItem => fx.prItem({ number, title, url: '', author: 'someone', updatedAt: number, ...extra });
  const withPrs = (prs: PrItem[]): Board => fx.board({ prs });

  it('finds PRs by number, title, branch or author, titles first, then open ones', () => {
    const b = withPrs([pr(1, 'Fix the login page', { state: 'merged' }), pr(2, 'Login retries'), pr(3, 'Docs', { head: 'login-docs' }), pr(4, 'Other')]);
    expect(findPrs(b, ALL_PROJECTS, 'login').map((r) => r.pr.number)).toEqual([2, 1, 3]);
    expect(findPrs(b, ALL_PROJECTS, '#4').map((r) => r.pr.number)).toEqual([4]);
    expect(findPrs(b, ALL_PROJECTS, 'someone docs').map((r) => r.pr.number)).toEqual([3]);
    expect(findPrs(b, ALL_PROJECTS, 'login')[0].marks).toEqual([[0, 5]]);
  });

  it('puts the ones that are your move first, and keeps to the project', () => {
    const b = withPrs([pr(1, 'Page one'), pr(2, 'Page two', { reviewRequested: true }), pr(3, 'Page three', { project: 'Other' })]);
    expect(findPrs(b, ALL_PROJECTS, 'page').map((r) => r.pr.number)).toEqual([2, 3, 1]);
    expect(findPrs(b, 'Acme', 'page').map((r) => r.pr.number)).toEqual([2, 1]);
    expect(findPrs(b, 'Acme', '')).toEqual([]);
  });

  it('finds a PR by its number only in its own project', () => {
    const b = withPrs([pr(1055, 'Page one', { project: 'Other' }), pr(2, 'Page two')]);
    expect(findPrs(b, 'Acme', '1055')).toEqual([]);
    expect(findPrs(b, 'Acme', '#2').map((r) => r.pr.number)).toEqual([2]);
    expect(findPrs(b, 'Other', '#1055').map((r) => r.pr.number)).toEqual([1055]);
    expect(findPrs(b, ALL_PROJECTS, '1055').map((r) => r.pr.number)).toEqual([1055]);
  });
});

describe('stillSaid', () => {
  const hit = (key: string, snippet: string) => ({ key, snippet, you: false, at: 0 });
  const hits = [hit('a', '…the Quick panel opens slowly…'), hit('b', 'the bar resizes'), hit('c', 'Codealong quick fix')];

  it('keeps the hits whose line has every word typed now', () => {
    expect(stillSaid(hits, ['quick']).map((h) => h.key)).toEqual(['a', 'c']);
    expect(stillSaid(hits, ['quick', 'panel']).map((h) => h.key)).toEqual(['a']);
    // A word typed further along narrows what's kept; one no line has keeps nothing.
    expect(stillSaid(hits, ['quick', 'fixes'])).toEqual([]);
    expect(stillSaid(hits, [])).toEqual(hits);
  });
});

describe('deviceHits', () => {
  const hit = (key: string) => ({ key, snippet: 'the webhook', you: false, at: 0 });
  it('puts every device’s hits in one list, leaving out the ones that couldn’t be asked', () => {
    const found = deviceHits([
      { device: 'b1', name: 'Studio', found: { hits: [hit('@b1:claude:claude:a')], building: false, off: false }, error: null },
      { device: 'c2', name: 'Air', found: null, error: 'It didn’t answer.' },
      { device: 'd3', name: 'Mini', found: { hits: [hit('@d3:codex:codex:b')], building: true, off: false }, error: null },
    ]);
    expect(found.hits.map((h) => h.key)).toEqual(['@b1:claude:claude:a', '@d3:codex:codex:b']);
    expect(found.building).toBe(true);
    expect(deviceHits([])).toEqual({ hits: [], building: false });
  });
});

describe('what the start row and the Mac menu say', () => {
  it('titles the start row by what it starts for', () => {
    expect(startTitle(3, 'Start in api')).toBe('Start in api for these 3 todos');
    expect(startTitle(1, 'Start in api')).toBe('Start in api for this todo');
    expect(startTitle(0, 'Start in api')).toBe('Start in api');
    expect(startTitle(0, null)).toBe('Start a new session');
    expect(startTitle(1, null)).toBe('Start a new session for this todo');
  });

  it('adds what goes with the prompt, and what isn’t linked on another Mac', () => {
    expect(attachmentsNote(1)).toBe(' The attachment goes as a file path the agent opens.');
    expect(attachmentsNote(2)).toBe(' Attachments go as file paths the agent opens.');
    expect(todosStayNote(1)).toBe(' Todos are this Mac’s, so it isn’t linked to the session.');
    expect(todosStayNote(2)).toBe(' Todos are this Mac’s, so they aren’t linked to the session.');
  });

  it('says first that a todo’s primary session is running, so a new one works alongside it', () => {
    const at = { lane: { callsign: 'app-2' }, session: { title: 'Fix the header', account: 'claude2' } };
    expect(primaryRunningWords([], 1)).toBe('');
    expect(primaryRunningWords([{ todo: { title: 'Header wraps' }, at }], 1)).toBe('“Fix the header” (claude2 · app-2) is this todo’s primary session and is running: this one would work alongside it. ');
    expect(primaryRunningWords([{ todo: { title: 'Header wraps' }, at }], 2)).toBe('“Fix the header” (claude2 · app-2) is “Header wraps”’s primary session and is running: this one would work alongside it. ');
    expect(primaryRunningWords([{ todo: { title: '' }, at }, { todo: { title: 'B' }, at }], 2)).toBe('“Fix the header” (claude2 · app-2) and 1 more are running as these todos’ primary sessions: this one would work alongside them. ');
  });

  it('counts a Mac’s worktrees in the project', () => {
    expect(worktreeCount(1, 'Acme')).toBe('1 worktree in Acme');
    expect(worktreeCount(0, ALL_PROJECTS)).toBe('0 worktrees');
  });

  it('says why another Mac can’t start a session now', () => {
    expect(macUnavailable({ name: 'Studio', online: true, lastSeen: null, remoteControl: true }, NOW)).toEqual({ away: null, off: null, title: undefined });
    expect(macUnavailable({ name: 'Studio', online: false, lastSeen: null, remoteControl: true }, NOW)).toEqual({ away: 'Away', off: null, title: undefined });
    expect(macUnavailable({ name: 'Studio', online: false, lastSeen: NOW - 7_200_000, remoteControl: false }, NOW).away).toBe(`Away: last seen ${agoPhrase(NOW - 7_200_000, NOW)}`);
    expect(macUnavailable({ name: 'Studio', online: true, lastSeen: null, remoteControl: false }, NOW)).toEqual({
      away: null,
      off: 'Remote control is off there',
      title: 'Remote control is off on Studio; turn it on there in Settings › Devices to start sessions from here.',
    });
  });
});
