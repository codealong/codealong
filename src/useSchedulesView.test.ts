// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, engine, mountHook, settle, type Mounted } from './testing/dom';
import { menusAsked, menuWords, pick } from './testing/menus';
import { sessionHomes } from './lanes';
import { blankSchedule } from './schedules';
import * as fx from './testing/fixtures';
import type { HarnessSchedule, ScheduleItem } from './types';
import { useSchedulesView, type SchedulesViewInput } from './useSchedulesView';

// The Schedules view's own state in the main window (useSchedulesView.ts): what it selects, the
// harnesses' own schedules it reads while it shows, and what its rows, panel and menus do.

const today = fx.session({ key: 'claude:northwind:today', title: 'Triage, today' });
const before = fx.session({ key: 'claude:northwind:before', title: 'Triage, before', archived: true });
const loop = fx.session({ key: 'claude:northwind:loop', title: 'Watching the deploy', openTarget: 's|claude:northwind:loop|focus' });
const app = fx.lane({ id: '/r/northwind/app', path: '/r/northwind/app', project: 'Northwind', sessions: [today, before, loop] });
const site = fx.lane({ id: '/r/atlas/site', path: '/r/atlas/site', project: 'Atlas' });
const homes = sessionHomes([app, site]);

const run = (key: string, at: number) => ({ at, why: 'Weekdays at 9:00', outcome: 'started' as const, said: '', session: key });
const item = (id: string, folder: string, over: Partial<ScheduleItem> = {}): ScheduleItem => ({ ...blankSchedule(folder), id, name: id, whenWords: 'Weekdays at 9:00', runs: [], asked: 0, ...over });
const triage = item('Triage', app.path, { runs: [run(today.key, 2), run(before.key, 1)] });
const digest = item('Digest', app.path);
const deploys = item('Deploys', site.path);
const kept: HarnessSchedule = { key: 'claude:northwind:loop-1', harness: 'claude', account: 'northwind', id: 'loop-1', kind: 'Loop', name: 'Deploy loop', prompt: 'Check the deploy', whenWords: 'Every 10 minutes', paused: false, lasts: 'While its session runs', session: loop.key };

let did: string[] = [];
beforeEach(() => {
  did = [];
  // The harnesses keep none, unless a test says otherwise.
  engine.answer('harness_schedules', []);
});
afterEach(() => {
  vi.useRealTimers();
});

/** The window, in Schedules on Northwind, but for what a test says. */
function has(over: Partial<SchedulesViewInput> = {}): SchedulesViewInput {
  return {
    view: 'schedules',
    board: fx.board({ lanes: [app, site], schedules: [triage, digest, deploys] }),
    current: 'Northwind',
    query: '',
    homes,
    allHomes: homes,
    selectedLane: null,
    scheduleSort: 'next',
    scheduleGroup: 'none',
    openScheduleGroups: new Set(),
    report: { error: (m) => void did.push(`error: ${m}`), info: (m) => void did.push(`info: ${m}`) },
    setToast: (m) => void did.push(`say “${m}”`),
    open: (lane, target) => void did.push(`open ${target} of ${lane.callsign}`),
    ...over,
  };
}
type View = Mounted<ReturnType<typeof useSchedulesView>, SchedulesViewInput>;
/** The window once what it read as it opened has come. */
async function mount(over: Partial<SchedulesViewInput> = {}): Promise<View> {
  const w = mountHook(useSchedulesView, has(over));
  await settle();
  return w;
}

describe('what the Schedules view lists and selects', () => {
  it('lists the project’s schedules, as the find narrows them, and selects the first until one is picked', async () => {
    const w = await mount();
    expect(w.current.projectSchedules.map((x) => x.id)).toEqual(['Triage', 'Digest']);
    expect(w.current.shownSchedules.map((x) => x.id)).toEqual(['Triage', 'Digest']);
    expect(w.current.selectedSchedule?.id).toBe('Triage');
    act(() => w.current.setScheduleId('Digest'));
    expect(w.current.selectedSchedule?.id).toBe('Digest');
    // The find leaves the one picked out: the first it lists is selected, and the project's own
    // schedules, which the sidebar counts, are as they were.
    w.rerender(has({ query: 'triage' }));
    expect([w.current.shownSchedules.map((x) => x.id), w.current.selectedSchedule?.id, w.current.projectSchedules.length]).toEqual([['Triage'], 'Triage', 2]);
    w.rerender(has({ current: 'Atlas' }));
    expect(w.current.selectedSchedule?.id).toBe('Deploys');
    w.rerender(has({ board: null }));
    expect([w.current.shownSchedules, w.current.selectedSchedule]).toEqual([[], null]);
  });

  it('lists them as the View menu says, sorted and grouped, and selects the first as it lists them', async () => {
    const listed = (w: View) => {
      const { sections, grouped } = w.current.scheduleListed;
      return grouped ? [...grouped.groups, ...grouped.apart.flatMap((s) => s.groups)].map((g) => `${g.lane?.callsign ?? 'none'}: ${g.rows.map((x) => x.id).join(', ')}`) : sections.flatMap((s) => s.rows.map((x) => x.id));
    };
    const w = await mount({ current: 'All projects', scheduleSort: 'name' });
    expect(listed(w)).toEqual(['Deploys', 'Digest', 'Triage']);
    expect(w.current.selectedSchedule?.id).toBe('Deploys');
    // Grouped by worktree with custom order: the first listed is that group's.
    w.rerender(has({ current: 'All projects', scheduleSort: 'name', scheduleGroup: 'worktree', groupOrder: [app.id] }));
    expect(listed(w)).toEqual(['app: Digest, Triage', 'site: Deploys']);
    expect(w.current.selectedSchedule?.id).toBe('Digest');
    // One picked stays the one selected, wherever the list has it.
    act(() => w.current.setScheduleId('Deploys'));
    expect(w.current.selectedSchedule?.id).toBe('Deploys');
  });

  it('shows a session selected under a schedule while that schedule lists it, as the row the window’s keys act on', async () => {
    const w = await mount();
    expect([w.current.scheduleSessionShown, w.current.scheduleRow]).toEqual([null, null]);
    act(() => w.current.setScheduleSession(today.key));
    expect(w.current.scheduleSessionShown).toEqual({ lane: app, session: today });
    expect(w.current.scheduleRow).toMatchObject({ kind: 'agent', key: `${app.id}::${today.key}`, session: today });
    // One of an earlier run, which the schedule lists only when asked for all of them, shows too.
    act(() => w.current.setScheduleSession(before.key));
    expect(w.current.scheduleSessionShown?.session.key).toBe(before.key);
    // Another schedule selected doesn't list it.
    act(() => w.current.setScheduleId('Digest'));
    expect([w.current.scheduleSessionShown, w.current.scheduleRow]).toEqual([null, null]);
    // In another view it's no row of the window's.
    act(() => w.current.setScheduleId('Triage'));
    w.rerender(has({ view: 'sessions' }));
    expect([w.current.scheduleSessionShown?.session.key, w.current.scheduleRow]).toEqual([before.key, null]);
  });

  it('says a session’s title where the views list it', async () => {
    const w = await mount({ homes: sessionHomes([site]) });
    expect([w.current.sessionTitle(today.key), (await mount()).current.sessionTitle(today.key)]).toEqual([null, 'Triage, today']);
  });
});

describe('the schedules the harnesses keep themselves', () => {
  it('are read while the view shows, again every half minute, and not in another view', async () => {
    vi.useFakeTimers();
    engine.answer('harness_schedules', [kept]);
    const w = mountHook(useSchedulesView, has({ view: 'sessions' }));
    await act(() => vi.advanceTimersByTimeAsync(60_000));
    expect([engine.sent('harness_schedules'), w.current.shownHarnessSchedules]).toEqual([[], []]);
    w.rerender(has());
    await act(() => vi.advanceTimersByTimeAsync(0));
    expect(engine.sent('harness_schedules')).toHaveLength(1);
    expect(w.current.shownHarnessSchedules.map((h) => h.name)).toEqual(['Deploy loop']);
    await act(() => vi.advanceTimersByTimeAsync(30_000));
    await act(() => vi.advanceTimersByTimeAsync(30_000));
    expect(engine.sent('harness_schedules')).toHaveLength(3);
    // Another view: no more reads, and the list keeps the last it read.
    w.rerender(has({ view: 'prs' }));
    await act(() => vi.advanceTimersByTimeAsync(60_000));
    expect(engine.sent('harness_schedules')).toHaveLength(3);
    expect(w.current.shownHarnessSchedules).toHaveLength(1);
  });

  it('keeps the last it read when a read fails', async () => {
    vi.useFakeTimers();
    engine.answer('harness_schedules', [kept]);
    const w = mountHook(useSchedulesView, has());
    await act(() => vi.advanceTimersByTimeAsync(0));
    engine.answer('harness_schedules', () => {
      throw new Error('The server isn’t answering.');
    });
    await act(() => vi.advanceTimersByTimeAsync(30_000));
    expect([engine.sent('harness_schedules').length, w.current.shownHarnessSchedules.length, did]).toEqual([2, 1, []]);
  });

  it('takes nothing from a read that comes back after the view has gone', async () => {
    let answer: (items: HarnessSchedule[]) => void = () => {};
    engine.answer('harness_schedules', () => new Promise<HarnessSchedule[]>((resolve) => (answer = resolve)));
    const w = await mount();
    w.rerender(has({ view: 'sessions' }));
    answer([kept]);
    await settle();
    w.rerender(has());
    expect(w.current.shownHarnessSchedules).toEqual([]);
  });

  it('select by their key, with the session one wakes shown under it, and open where they live', async () => {
    engine.answer('harness_schedules', [kept, { ...kept, key: 'codex:atlas:nightly', id: 'nightly', name: 'Nightly', session: undefined, link: 'codex://automations/nightly', harness: 'codex' }]);
    const w = await mount({ current: 'All projects' });
    act(() => w.current.setScheduleId(kept.key));
    expect([w.current.selectedHarness?.name, w.current.selectedSchedule]).toEqual(['Deploy loop', null]);
    // Only the session it wakes shows under it.
    act(() => w.current.setScheduleSession(today.key));
    expect(w.current.scheduleSessionShown).toBeNull();
    act(() => w.current.setScheduleSession(loop.key));
    expect(w.current.scheduleSessionShown).toEqual({ lane: app, session: loop });
    // By its link where it has one, else in its session.
    w.current.openHarness(w.current.shownHarnessSchedules[0]);
    w.current.openHarness(w.current.shownHarnessSchedules[1]);
    await settle();
    expect(did).toEqual(['open s|claude:northwind:loop|focus of app']);
    expect(engine.sent('open_harness_link')).toEqual([{ harness: 'codex', url: 'codex://automations/nightly' }]);
  });
});

describe('what’s done to a schedule', () => {
  it('runs it now and says so, pauses it, and says why when the engine turns something down', async () => {
    const w = await mount();
    w.current.runSchedule(triage);
    w.current.pauseSchedule(triage, true);
    await settle();
    expect(engine.sent('schedule_op')).toEqual([{ op: { op: 'run', id: 'Triage' } }, { op: { op: 'pause', id: 'Triage', paused: true } }]);
    expect(did).toEqual(['say ““Triage” runs in a few seconds.”']);
    did = [];
    engine.answer('schedule_op', () => {
      throw new Error('No such schedule.');
    });
    w.current.runSchedule(triage);
    await settle();
    expect(did).toEqual(['say “No such schedule.”']);
  });

  it('opens the dialog on a new one in the worktree selected, or on one to change', async () => {
    const w = await mount({ selectedLane: app });
    act(() => w.current.newSchedule());
    expect(w.current.scheduleEdit).toMatchObject({ id: '', run: { folder: app.path } });
    act(() => w.current.setScheduleEdit(null));
    expect(w.current.scheduleEdit).toBeNull();
  });

  it('has a menu that runs, edits, pauses and deletes it', async () => {
    const w = await mount();
    w.current.scheduleMenu(triage);
    await settle();
    const [menu] = menusAsked();
    expect(menuWords(menu)).toEqual(['Run now', 'Edit…', 'Pause', '-', 'Copy schedule ID', '-', 'Delete schedule']);
    pick(menu, 'Edit…');
    // The schedule as it's saved, without what the board adds to say how it's going.
    expect(w.current.scheduleEdit).toMatchObject({ id: 'Triage', name: 'Triage' });
    expect(w.current.scheduleEdit).not.toHaveProperty('runs');
    pick(menu, 'Pause');
    pick(menu, 'Delete schedule');
    await settle();
    expect(engine.sent('schedule_op')).toEqual([{ op: { op: 'pause', id: 'Triage', paused: true } }, { op: { op: 'remove', id: 'Triage' } }]);
    expect(did).toEqual(['say “Deleted “Triage”.”']);
  });
});

describe('what the view keeps while the window is open', () => {
  it('lists every session of the schedules asked to, and folds the finished ones away until asked', async () => {
    const w = await mount();
    expect([w.current.schedulesAll.size, w.current.showFinished]).toEqual([0, false]);
    act(() => w.current.setSchedulesAll(new Set(['Triage'])));
    act(() => w.current.setShowFinished(true));
    expect([[...w.current.schedulesAll], w.current.showFinished]).toEqual([['Triage'], true]);
  });
});
