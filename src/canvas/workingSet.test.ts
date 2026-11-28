import { describe, expect, it } from 'vitest';
import type { BoardDoc } from '../boards/model';
import { memberTest, subjectsOf } from '../boards/rules';
import { sessionItemsOf } from '../boardLists';
import * as fx from '../testing/fixtures';
import { NOW } from '../testing/fixtures';
import type { Lane, ScheduleItem, Session } from '../types';
import { entitiesOf, overlaps, refId } from './model';
import {
  actionOf,
  buildWorkingSet,
  detachNode,
  emptyPrefs,
  extractDeadline,
  groupNodes,
  pinNodeAt,
  pullInNode,
  relaxLayout,
  renameCluster,
  shelveNode,
  summaryOf,
} from './workingSet';

const HOUR = 3_600_000;
const DAY = 24 * HOUR;

const mkSession = (key: string, fields: Partial<Session> = {}): Session =>
  fx.session({ key, id: fields.id ?? key.split(':').pop() ?? key, ...fields });

const mkLane = (id: string, sessions: Session[], fields: Partial<Lane> = {}): Lane =>
  fx.lane({ id, repo: 'app', branch: 'main', sessions, lastActivity: NOW - 10_000, ...fields });

const mkSchedule = (overrides: Partial<ScheduleItem> = {}): ScheduleItem => ({
  id: 's-1',
  name: 'Scan watch',
  paused: false,
  when: { kind: 'watch', command: 'scan', every: '10m', mode: 'value' },
  onlyIf: '',
  run: { to: 'new', session: '', folder: '/repo', worktree: false, account: 'claude', prompt: 'Evaluate scan results', freshAfter: '' },
  quiet: false,
  overlap: 'skip',
  catchUp: 'once',
  notes: false,
  createdAt: NOW - 5 * DAY,
  updatedAt: NOW - HOUR,
  whenWords: 'Every 10m',
  runs: [],
  asked: 0,
  ...overrides,
});

describe('actionOf, extractDeadline and summaryOf', () => {
  it('separates urgent blockers, human handoffs with deadlines, decisions, working/scheduled holds and pure FYI reports', () => {
    const needs = mkSession('s:needs', { status: 'needs', need: 'approval' });
    expect(actionOf(needs, [], [], NOW)).toMatchObject({ tier: 'urgent', label: 'Approve', rank: 0 });

    const limit = mkSession('s:limit', { status: 'error', limit: { meter: 'weekly', at: NOW - 1000, resetsAt: NOW + HOUR, moveTo: null } });
    expect(actionOf(limit, [], [], NOW)).toMatchObject({ tier: 'urgent', label: 'Limit hit', rank: 0 });

    const handoff = mkSession('s:handoff', {
      status: 'stopped',
      insight: { ending: 'handoff', finished: 0, note: 'Ship #204 by Sun 11-29 21:00Z before freeze.' },
    });
    expect(actionOf(handoff, [], [], NOW)).toMatchObject({ tier: 'handoff', label: 'Your step', deadline: 'by Sun 11-29', rank: 1 });

    const offer = mkSession('s:offer', { status: 'unread', insight: { ending: 'offer', finished: 0, note: 'Pick rollout strategy A or B.' } });
    expect(actionOf(offer, [], [], NOW)).toMatchObject({ tier: 'decision', label: 'Decision', rank: 2 });

    const fyi = mkSession('s:fyi', { status: 'unread', insight: { ending: 'report', finished: 0, note: 'Pure FYI confirming merge.' } });
    expect(actionOf(fyi, [], [], NOW)).toMatchObject({ tier: 'quiet', label: 'FYI', rank: 4 });

    const scheduled = mkSession('s:sched', { status: 'idle' });
    const timers = [{ scheduleId: 'sc-1', name: 'Hold check', at: NOW + 3 * HOUR, queued: false, words: '19:00' }];
    expect(actionOf(scheduled, [], timers, NOW)).toMatchObject({ tier: 'working', label: 'Scheduled', rank: 3 });
  });

  it('prefers insight.note over raw excerpt for the card summary', () => {
    const s = mkSession('s:1', {
      excerpt: 'You are right, I recorded it…',
      insight: { note: 'Publish 4 northwind packets by Fri Dec 4 and confirm reviewers.' },
    });
    expect(summaryOf(s, NOW)).toBe('Publish 4 northwind packets by Fri Dec 4 and confirm reviewers.');
    expect(extractDeadline(s.insight?.note)).toBe('by Fri Dec 4');
  });
});

describe('buildWorkingSet', () => {
  it('cuts out quiet/archived noise, rescues stopped handoffs, absorbs one-off checkpoints and collapses superseded schedule runs', () => {
    const urgentHelper = mkSession('claude:claude:aa11bb22-0000', {
      id: 'aa11bb22-0000',
      title: 'kestrel-214-northwind-rollout',
      status: 'needs',
      need: 'approval',
      helper: { parent: 'claude:claude:archived-parent', report: true },
    });
    const handoffMain = mkSession('claude:claude:cc33dd44-0000', {
      id: 'cc33dd44-0000',
      title: 'Northwind rollout handoff',
      status: 'unread',
      insight: { ending: 'handoff', finished: 0, note: 'Waiting for your publish approval on aa11bb22.' },
    });
    const stoppedHandoff = mkSession('claude:claude:ee55ff66-0000', {
      id: 'ee55ff66-0000',
      title: 'canary job 90010002',
      status: 'stopped',
      lastActivity: NOW - 2 * DAY,
      insight: { ending: 'handoff', finished: 0, note: 'Ship #204 by Sun 11-29.' },
    });
    const oldIdle = mkSession('claude:claude:old-idle-0000', {
      id: 'old-idle-0000',
      title: 'Ancient cleanup',
      status: 'idle',
      lastActivity: NOW - 10 * DAY,
    });
    const reviewHelper = mkSession('cursor:cursor:rev-0000', {
      id: 'rev-0000',
      title: 'Review PR #218',
      status: 'idle',
      helper: { parent: 'claude:claude:cc33dd44-0000', pr: 'northwind/atlas#218' },
    });
    const workerPatch = mkSession('claude:claude:11223344-0000', {
      id: '11223344-0000',
      title: 'atlas worker pool patch',
      status: 'idle',
    });
    const latestAudit = mkSession('claude:claude:55667788-0000', {
      id: '55667788-0000',
      title: 'Nightly audit: 90010003',
      status: 'unread',
      insight: { ending: 'report', finished: 0, note: 'Run covered by ee55ff66.' },
    });
    const oldAudit1 = mkSession('claude:claude:old-audit-1', {
      id: 'old-audit-1',
      title: 'Nightly audit: 90010002',
      status: 'stopped',
      lastActivity: NOW - 3 * DAY,
      insight: { ending: 'report', note: 'Spawned ee55ff66.' },
    });
    const oldAudit2 = mkSession('claude:claude:old-audit-2', {
      id: 'old-audit-2',
      title: 'Nightly audit: 90010001',
      status: 'stopped',
      lastActivity: NOW - 5 * DAY,
    });

    const lanes = [
      mkLane('atlas', [handoffMain, stoppedHandoff, oldIdle, workerPatch, latestAudit, oldAudit1, oldAudit2], { repo: 'atlas' }),
      mkLane('kestrel', [urgentHelper, reviewHelper], { repo: 'kestrel' }),
    ];
    const schedules: ScheduleItem[] = [
      mkSchedule({
        id: 'sc-once',
        name: 'Atlas node check 19:00',
        once: true,
        when: { kind: 'time', rule: 'once' },
        whenWords: 'Once at 19:00',
        nextAt: NOW + 4 * HOUR,
        run: { to: 'session', session: workerPatch.key, folder: '/repo', worktree: false, account: 'claude', prompt: 'Check nodes', freshAfter: '' },
        thread: workerPatch.key,
      }),
      mkSchedule({
        id: 'sc-audit',
        name: 'Nightly audit',
        runs: [
          { at: NOW - HOUR, why: 'scan', outcome: 'started', said: '', session: latestAudit.key },
          { at: NOW - 3 * DAY, why: 'scan', outcome: 'started', said: '', session: oldAudit1.key },
          { at: NOW - 5 * DAY, why: 'scan', outcome: 'started', said: '', session: oldAudit2.key },
        ],
      }),
    ];

    const graph = buildWorkingSet({
      entities: entitiesOf(null, lanes, [], () => true),
      schedules,
      prs: [],
      todos: [],
      labels: new Map(),
      now: NOW,
      prefs: emptyPrefs(),
    });

    const ids = new Set(graph.nodes.map((n) => n.id));
    // Urgent helper, handoffs, scheduled checkpoint session, latest audit + schedule hub are admitted
    expect(ids.has(refId('session', urgentHelper.key))).toBe(true);
    expect(ids.has(refId('session', handoffMain.key))).toBe(true);
    expect(ids.has(refId('session', stoppedHandoff.key))).toBe(true);
    expect(ids.has(refId('session', workerPatch.key))).toBe(true);
    expect(ids.has('schedule:sc-audit')).toBe(true);
    // Old idle, idle review helper, and superseded to=new schedule runs are folded away
    expect(ids.has(refId('session', oldIdle.key))).toBe(false);
    expect(ids.has(refId('session', reviewHelper.key))).toBe(false);
    expect(ids.has(refId('session', oldAudit1.key))).toBe(false);
    expect(ids.has(refId('session', oldAudit2.key))).toBe(false);
    expect(graph.folded.collapsedRuns).toBe(2);
    expect(graph.folded.foldedHelpers).toBe(1);
    expect(graph.folded.quietSessions.map((p) => p.session.key)).toContain(oldIdle.key);

    // One-off schedule absorbed as a timer chip on workerPatch
    const workerNode = graph.nodes.find((n) => n.id === refId('session', workerPatch.key));
    expect(workerNode?.kind === 'session' && workerNode.timers.length).toBe(1);

    // Cross-repo mention edge connects Northwind handoff to urgent kestrel helper
    expect(
      graph.edges.some(
        (e) =>
          e.kind === 'mention' &&
          ((e.from === refId('session', handoffMain.key) && e.to === refId('session', urgentHelper.key)) ||
            (e.to === refId('session', handoffMain.key) && e.from === refId('session', urgentHelper.key))),
      ),
    ).toBe(true);

    // Urgent approval sorts first in Tab actionOrder, ahead of handoffs
    expect(graph.actionOrder[0]).toBe(refId('session', urgentHelper.key));

    // Layout guarantees zero overlapping cards
    const rects = [...graph.boxes.values()];
    for (let i = 0; i < rects.length; i++) {
      for (let j = i + 1; j < rects.length; j++) {
        expect(overlaps(rects[i], rects[j])).toBe(false);
      }
    }
  });

  it('respects user pinning, manual grouping, detaching, renaming, shelving and pull-in', () => {
    const a = mkSession('claude:a:11111111', { id: '11111111', title: 'Alpha rollout', status: 'working' });
    const b = mkSession('claude:a:22222222', { id: '22222222', title: 'Beta billing', status: 'working' });
    const quiet = mkSession('claude:a:33333333', { id: '33333333', title: 'Quiet archive', status: 'idle', lastActivity: NOW - 9 * DAY });
    const e = entitiesOf(null, [mkLane('l1', [a, b, quiet])], [], () => true);
    const idA = refId('session', a.key);
    const idB = refId('session', b.key);
    const idQ = refId('session', quiet.key);

    let prefs = groupNodes(emptyPrefs(), idA, idB);
    const grouped = buildWorkingSet({ entities: e, schedules: [], prs: [], todos: [], labels: new Map(), now: NOW, prefs });
    expect(grouped.clusters.length).toBe(1);
    expect(grouped.clusters[0].nodeIds.sort()).toEqual([idA, idB].sort());

    prefs = renameCluster(prefs, grouped.clusters[0].id, 'Release Prep');
    const renamed = buildWorkingSet({ entities: e, schedules: [], prs: [], todos: [], labels: new Map(), now: NOW, prefs });
    expect(renamed.clusters[0].title).toBe('Release Prep');

    prefs = detachNode(prefs, idB, { x: 800, y: 400 });
    const detached = buildWorkingSet({ entities: e, schedules: [], prs: [], todos: [], labels: new Map(), now: NOW, prefs });
    expect(detached.boxes.get(idB)).toMatchObject({ x: 800, y: 400 });

    prefs = pinNodeAt(prefs, idA, { x: 100, y: 200 });
    prefs = shelveNode(prefs, idB);
    prefs = pullInNode(prefs, idQ);
    const updated = buildWorkingSet({ entities: e, schedules: [], prs: [], todos: [], labels: new Map(), now: NOW, prefs });
    expect(updated.nodes.map((n) => n.id)).toContain(idQ);
    expect(updated.nodes.map((n) => n.id)).not.toContain(idB);
    expect(updated.boxes.get(idA)).toMatchObject({ x: 100, y: 200 });

    prefs = relaxLayout(prefs);
    expect(prefs.pinnedPos).toEqual({});
  });

  it('scopes the Map cleanly across Everything, focused rule, and hand-curated boards', () => {
    const kestrelNeeds = mkSession('claude:a:kest1111', { id: 'kest1111', title: 'Kestrel rollout', status: 'needs', need: 'approval' });
    const kestrelIdle = mkSession('claude:a:kest2222', { id: 'kest2222', title: 'Kestrel notes', status: 'idle', lastActivity: NOW - 2 * DAY });
    const kestrelDone = mkSession('claude:a:kest4444', { id: 'kest4444', title: 'Closed kestrel cleanup', status: 'stopped', lastActivity: NOW - DAY, insight: { finished: 1 } });
    const kestrelOld = mkSession('claude:a:kest3333', { id: 'kest3333', title: 'Old kestrel audit', status: 'stopped', lastActivity: NOW - 20 * DAY });
    const atlasWork = mkSession('claude:a:atls1111', { id: 'atls1111', title: 'Atlas billing refactor', status: 'working' });
    const atlasIdle = mkSession('claude:a:atls2222', { id: 'atls2222', title: 'Old atlas script', status: 'idle', lastActivity: NOW - 15 * DAY });

    const lanes = [
      mkLane('l-kestrel', [kestrelNeeds, kestrelIdle, kestrelDone, kestrelOld], { repo: 'kestrel', repoSlug: 'northwind/kestrel' }),
      mkLane('l-atlas', [atlasWork, atlasIdle], { repo: 'atlas', repoSlug: 'northwind/atlas' }),
    ];
    const atlasScan = mkSchedule({
      id: 'sc-atlas',
      name: 'Atlas canary',
      worktree: 'l-atlas',
      runs: [
        { at: NOW - HOUR, why: 'tick', outcome: 'started', said: '', session: atlasWork.key },
        { at: NOW - 2 * DAY, why: 'tick', outcome: 'started', said: '', session: atlasIdle.key },
      ],
    });
    const entities = entitiesOf(null, lanes, [], () => true);
    const subjects = subjectsOf({ todoItems: [], todos: [], sessionItems: sessionItemsOf(lanes), lanes, prs: [] });

    const runBoard = (board: BoardDoc) => {
      const member = memberTest(board, subjects, NOW);
      const shows = new Set<string>();
      for (const s of [kestrelNeeds, kestrelIdle, kestrelDone, kestrelOld, atlasWork, atlasIdle]) {
        const id = refId('session', s.key);
        if (member(id) && (board.added.includes(id) || s.status === 'needs' || s.status === 'working')) shows.add(id);
      }
      return buildWorkingSet({
        entities,
        schedules: [atlasScan],
        prs: [],
        todos: [],
        labels: new Map(),
        now: NOW,
        prefs: emptyPrefs(),
        boardScope: { board, member, shows },
      });
    };

    // 1. Everything board: smart auto-curation + honors board.removed and board.added
    const everythingBoard: BoardDoc = {
      id: 'b-all',
      name: 'Working set',
      project: 'Northwind',
      rule: {},
      added: [refId('session', kestrelOld.key)],
      removed: [refId('session', atlasWork.key)],
    };
    const gAll = runBoard(everythingBoard);
    const allIds = new Set(gAll.nodes.map((n) => n.id));
    expect(allIds.has(refId('session', kestrelNeeds.key))).toBe(true);
    expect(allIds.has(refId('session', kestrelOld.key))).toBe(true); // explicitly added
    expect(allIds.has(refId('session', atlasWork.key))).toBe(false); // explicitly removed
    expect(allIds.has(refId('session', kestrelIdle.key))).toBe(false); // quiet auto-filtered
    expect(gAll.folded.shelvedSessions.map((p) => p.session.key)).toEqual([atlasWork.key]);

    // 2. Focused rule board (repo = northwind/kestrel): excludes atlas sessions & unrelated atlas schedule hub
    const ruleBoard: BoardDoc = {
      id: 'b-kestrel',
      name: 'Kestrel',
      project: 'Northwind',
      rule: { repos: ['northwind/kestrel'] },
      added: [],
      removed: [],
    };
    const gRule = runBoard(ruleBoard);
    const ruleIds = new Set(gRule.nodes.map((n) => n.id));
    expect(ruleIds.has(refId('session', kestrelNeeds.key))).toBe(true);
    expect(ruleIds.has(refId('session', kestrelIdle.key))).toBe(true); // within 7d and not finished on focused topic board
    expect(ruleIds.has(refId('session', kestrelDone.key))).toBe(false); // finished session stays out of focused topic board
    expect(ruleIds.has(refId('session', kestrelOld.key))).toBe(false); // >7d quiet stays in Pull in
    expect(ruleIds.has(refId('session', atlasWork.key))).toBe(false);
    expect(ruleIds.has('schedule:sc-atlas')).toBe(false); // unrelated schedule stays off focused board
    // Finished & older matching sessions sort ahead of unrelated project sessions in Pull in
    expect(gRule.folded.quietSessions.slice(0, 2).map((p) => p.session.key)).toEqual([kestrelDone.key, kestrelOld.key]);

    // 3. Hand-curated board (rule: null): keeps added idle/stopped sessions visible, hides everything else
    const handBoard: BoardDoc = {
      id: 'b-hand',
      name: 'Northwind Rollout',
      project: 'Northwind',
      rule: null,
      added: [refId('session', kestrelOld.key), refId('session', atlasIdle.key)],
      removed: [],
    };
    const gHand = runBoard(handBoard);
    const handIds = [...gHand.nodes.map((n) => n.id)].sort();
    expect(handIds).toEqual([refId('session', atlasIdle.key), refId('session', kestrelOld.key)].sort());
    expect(gHand.folded.quietSessions.map((p) => p.session.key)).toContain(kestrelNeeds.key);
  });
});
