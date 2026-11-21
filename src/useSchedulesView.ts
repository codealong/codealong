// The Schedules view's own state in the main window (App.tsx), and what's done to schedules from
// its rows, its panel and its menus: the schedule selected and a session selected under it, whose
// panel shows meanwhile; the schedules listing every session of theirs; the finished ones folded
// away; the schedule its dialog makes or changes (ui/ScheduleDialog.tsx); and the schedules the
// harnesses keep themselves, which are read while the view shows. What the view lists is worked
// out in boardLists.ts and schedules.ts; how it's sorted and grouped (its View menu) is kept with
// the other lists' in App.tsx.

import { useEffect, useMemo, useState } from 'react';
import { harnessSchedules, openHarnessLink, scheduleOp } from './api';
import { harnessSchedulesIn, scheduleAtWork, scheduleSessions, schedulesIn, sessionItemOf } from './boardLists';
import { showContextMenu } from './contextMenu';
import type { Placed } from './lanes';
import type { ListGroupBy } from './groups';
import * as menus from './menus';
import { ALL_PROJECTS } from './quick';
import { errorText, type Report } from './report';
import { blankSchedule, firstSchedule, newScheduleFolder, scheduleListing, scheduleOf, type ScheduleSort } from './schedules';
import type { Board, HarnessSchedule, Lane, Schedule, ScheduleItem } from './types';

/** What the window has for it. */
export interface SchedulesViewInput {
  view: string;
  board: Board | null;
  /** The project the window shows, and the list's find. */
  current: string;
  query: string;
  /** Where each session is listed: the views' own, and every one the window has. */
  homes: ReadonlyMap<string, Placed>;
  allHomes: ReadonlyMap<string, Placed>;
  /** The worktree of the row selected, which a new schedule starts in. */
  selectedLane: Lane | null;
  /** How the view lists them (its View menu): sorted, and grouped by project, repo or worktree,
   *  in custom or alphabetical order, with the groups opened past their fold. */
  scheduleSort: ScheduleSort;
  scheduleGroup: ListGroupBy;
  groupOrder?: readonly string[];
  openScheduleGroups: ReadonlySet<string>;
  report: Report;
  setToast: (message: string) => void;
  /** Opens one of a worktree's targets where it lives. */
  open: (lane: Lane, targetId: string | null | undefined) => void;
}

export function useSchedulesView(input: SchedulesViewInput) {
  const { view, board, current, query, homes, allHomes, selectedLane, scheduleSort, scheduleGroup, groupOrder, openScheduleGroups, report, setToast, open } = input;
  // The Schedules view's selected schedule, and the one its dialog makes or changes; a session
  // selected under it, whose panel shows meanwhile; and the schedules listing every session of
  // theirs, not only the latest run's and those at work.
  const [scheduleId, setScheduleId] = useState<string | null>(null);
  const [scheduleSession, setScheduleSession] = useState<string | null>(null);
  const [schedulesAll, setSchedulesAll] = useState<ReadonlySet<string>>(() => new Set());
  // The Schedules view's finished schedules are folded away until asked for.
  const [showFinished, setShowFinished] = useState(false);
  // The schedules the harnesses keep themselves, read while the Schedules view shows: they're read
  // from records and files, not sent with the board.
  const [harnessItems, setHarnessItems] = useState<HarnessSchedule[]>([]);
  const [scheduleEdit, setScheduleEdit] = useState<Schedule | null>(null);
  // The Schedules view's schedules (the project's, as the find narrows them), the one selected (the
  // first it lists, until one is picked), and the session selected under it while it's still listed there.
  const projectSchedules = useMemo(() => schedulesIn(board?.schedules ?? [], board?.lanes ?? [], current, ''), [board?.schedules, board?.lanes, current]);
  const shownSchedules = useMemo(() => schedulesIn(projectSchedules, board?.lanes ?? [], ALL_PROJECTS, query), [projectSchedules, board?.lanes, query]);
  const shownHarnessSchedules = useMemo(() => harnessSchedulesIn(harnessItems, allHomes, board?.lanes ?? [], current, query), [harnessItems, allHomes, board?.lanes, current, query]);
  // A harness's schedule selected, by its key, which no schedule of Codealong's has.
  const selectedHarness: HarnessSchedule | null = shownHarnessSchedules.find((h) => h.key === scheduleId) ?? null;
  // As the view lists them (its View menu): sorted, and grouped by worktree or repo, the pinned
  // ones first, as the other lists are.
  const scheduleHomes = view === 'schedules' ? allHomes : null;
  const scheduleListed = useMemo(
    () =>
      scheduleListing(shownSchedules, scheduleSort, {
        by: scheduleGroup,
        lanes: board?.lanes ?? [],
        order: groupOrder,
        opened: openScheduleGroups,
        whole: !!query,
        // The one picked never folds away, wherever it goes as it's shown from a card, paused, or
        // sorted or found otherwise; nor one with a session at work under it, which takes a look
        // through every session, so only while Schedules shows.
        awake: (x) => x.id === scheduleId || (!!scheduleHomes && scheduleAtWork(x, scheduleHomes)),
      }),
    [shownSchedules, scheduleSort, scheduleGroup, board?.lanes, groupOrder, openScheduleGroups, query, scheduleId, scheduleHomes],
  );
  const selectedSchedule: ScheduleItem | null = selectedHarness ? null : (shownSchedules.find((x) => x.id === scheduleId) ?? firstSchedule(scheduleListed));
  const scheduleSessionShown = !scheduleSession
    ? null
    : selectedHarness
      ? selectedHarness.session === scheduleSession
        ? (allHomes.get(scheduleSession) ?? null)
        : null
      : selectedSchedule
        ? (scheduleSessions(selectedSchedule, allHomes, true).rows.find((p) => p.session.key === scheduleSession) ?? null)
        : null;
  useEffect(() => {
    if (view !== 'schedules') return;
    let live = true;
    const read = () =>
      void harnessSchedules()
        .then((items) => live && setHarnessItems(items))
        // Read again in half a minute; meanwhile the list shows the last it read.
        .catch(() => {});
    read();
    const every = window.setInterval(read, 30_000);
    return () => {
      live = false;
      window.clearInterval(every);
    };
  }, [view]);
  // In the Schedules view, that session is what the window's keys act on, as a row selected in a
  // list is elsewhere (mainKeys.ts): its worktree's shortcuts and a new session's place go by it.
  const scheduleRow = view === 'schedules' && scheduleSessionShown ? sessionItemOf(scheduleSessionShown) : null;

  // What's done to schedules from the view's rows, panel and menus.
  const sessionTitle = (key: string) => homes.get(key)?.session.title ?? null;
  const scheduleDone = (what: Promise<unknown>) => void what.catch((e: unknown) => setToast(errorText(e)));
  const runSchedule = (x: ScheduleItem) => scheduleDone(scheduleOp({ op: 'run', id: x.id }).then(() => setToast(`“${x.name}” runs in a few seconds.`)));
  // A harness's schedule opens where it lives, by its link, else in its session.
  const openHarness = (h: HarnessSchedule) => {
    const p = h.session ? allHomes.get(h.session) : undefined;
    if (h.link) scheduleDone(openHarnessLink(h.harness, h.link));
    else if (p) open(p.lane, p.session.openTarget);
  };
  const pauseSchedule = (x: ScheduleItem, paused: boolean) => scheduleDone(scheduleOp({ op: 'pause', id: x.id, paused }));
  const newSchedule = () => setScheduleEdit(blankSchedule(newScheduleFolder(board?.lanes ?? [], current, selectedLane, board?.pinnedWorktrees ?? [])));
  const scheduleMenu = (x: ScheduleItem, at?: { x: number; y: number }) =>
    void showContextMenu(
      menus.scheduleMenu(x, {
        report,
        run: () => runSchedule(x),
        edit: () => setScheduleEdit(scheduleOf(x)),
        pause: (paused) => pauseSchedule(x, paused),
        remove: () => scheduleDone(scheduleOp({ op: 'remove', id: x.id }).then(() => setToast(`Deleted “${x.name}”.`))),
      }),
      at,
    ).catch((e: unknown) => setToast(String(e)));

  return {
    setScheduleId,
    setScheduleSession,
    schedulesAll,
    setSchedulesAll,
    showFinished,
    setShowFinished,
    scheduleEdit,
    setScheduleEdit,
    projectSchedules,
    shownSchedules,
    shownHarnessSchedules,
    scheduleListed,
    selectedHarness,
    selectedSchedule,
    scheduleSessionShown,
    scheduleRow,
    sessionTitle,
    runSchedule,
    openHarness,
    pauseSchedule,
    newSchedule,
    scheduleMenu,
  };
}
