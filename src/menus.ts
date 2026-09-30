// Right-click menus for what Codealong lists (sessions, worktrees, PRs, todos), each built in one
// place so it reads the same wherever it's right-clicked: the main window's lists and panels, the
// floating bar's pills, the peek, the quick panel's rows, cards on the desktop. A window says
// what it can do (`SessionCan` and the others), and an item shows when the window can do it and
// the thing allows it, always in the same order. What works the same from every window (mark
// seen, snooze, close, archive, fork, pin to the desktop, copy) is done here, and says how it
// went through the window's `Report`: a toast in the main window, a note on the bar.

import { useEffect, useState } from 'react';
import { closeSession, desktopPins, forkSession, onDesktopPins, openUrl, pinToDesktop, sessionCancel, showQuick, unpinFromDesktop } from './api';
import { archiveAhead, seenAhead } from './ahead';
import type { MenuAction, MenuSpec } from './contextMenu';
import { shortOf } from './devices';
import { folderMenu, openInMenu, opensInEditor, type AppIcons, type Apps } from './editors';
import { renames, sessionPlace } from './harness';
import { closable, seenable, stoppable } from './lanes';
import { snoozeMenu } from './snooze';
import type { DesktopPinned, Lane, Pr, Session, Started } from './types';
import { errorText, type Report } from './report';

/** The editors installed here and their icons, for Open folder in and Open in. */
export interface Editors {
  apps: Apps;
  icons: AppIcons;
}

/** Groups of items, with a line between the ones that have any. */
function sections(...groups: MenuSpec[]): MenuSpec {
  return groups.filter((g) => g.length > 0).flatMap((g, i): MenuSpec => (i === 0 ? g : ['separator', ...g]));
}

function copyItem(label: string, text: string | null | undefined, what: string, report: Report): MenuSpec {
  if (!text) return [];
  const copy = () =>
    navigator.clipboard.writeText(text).then(
      () => report.info(`Copied the ${what}.`),
      () => report.error(`Couldn't copy the ${what}.`),
    );
  return [{ label, run: () => void copy() }];
}

/** Pin to desktop, or Remove from desktop once it's there: a card over every window (desktop.rs). */
export function desktopItem(ref: DesktopPinned['ref'], key: string, pinned: ReadonlySet<string>, report: Report): MenuAction {
  const on = pinned.has(`${ref}:${key}`);
  const act = () => (on ? unpinFromDesktop({ ref, key }) : pinToDesktop({ ref, key }));
  return { label: on ? 'Remove from desktop' : 'Pin to desktop', run: () => void act().catch((e: unknown) => report.error(errorText(e))) };
}

/** What's pinned to the desktop, as `ref:key`, kept up to date, for Pin to desktop in a
 *  window's menus. */
export function useDesktopPins(): ReadonlySet<string> {
  const [pins, setPins] = useState<ReadonlySet<string>>(() => new Set());
  useEffect(() => {
    const take = (list: DesktopPinned[]) => setPins(new Set(list.map((p) => `${p.ref}:${p.key}`)));
    void desktopPins().then(take);
    return onDesktopPins(take);
  }, []);
  return pins;
}

/** Where a session opens, as its worktree says it (Respond in Ghostty, Resume in Ghostty), or,
 *  since worktrees list that for their first few sessions only, from where it runs. */
export function openLabel(lane: Lane, s: Session, terminal: string): string {
  return lane.targets.find((t) => t.id === s.openTarget)?.label ?? `Open in ${sessionPlace(s, terminal)}`;
}

/** Opens a worktree's PR on GitHub. */
function prItem(lane: Lane, report: Report): MenuSpec {
  const pr = lane.pr;
  return pr ? [{ label: `Open PR #${pr.number} on GitHub`, run: () => void openUrl(pr.url).catch((e: unknown) => report.error(errorText(e))) }] : [];
}

/** New session here…: the quick panel on the worktree, from any window. The panel starts
 *  sessions on this device, so another device's worktree needs a window that says otherwise. */
function newSessionItem(lane: Lane, run: (() => void) | undefined): MenuSpec {
  if (!run && shortOf(lane.id)) return [];
  return [{ label: 'New session here…', run: run ?? (() => void showQuick({ laneId: lane.id, project: lane.project })) }];
}

export interface SessionCan {
  report: Report;
  /** Opens one of its worktree's targets: the session where it lives, the folder in an editor. */
  open: (lane: Lane, target: string) => void;
  /** The terminal app's name, for Open in Ghostty where the worktree doesn't name the target. */
  terminal: string;
  /** Starts the menu with the session's name and worktree, where what was clicked doesn't say
   *  (a pill on the bar). */
  heading?: boolean;
  /** Shows it in the main window, from the other windows. */
  reveal?: () => void;
  /** Answers it in this window: the peek from the bar, the quick panel's box. */
  reply?: () => void;
  editors?: Editors;
  /** New session here…, when it's more than the quick panel on the worktree. */
  newSession?: () => void;
  /** Hears that a fork started, to select it once it shows. */
  forked?: (started: Started) => void;
  /** Add to todo and Remove from todo, where the todos are (the main window). */
  todos?: MenuSpec;
  /** Asks for a new name, where this window asks (a sheet, the peek, the quick panel's box). */
  rename?: () => void;
  /** Pins it to this window's own list (the main window's Pinned). */
  pin?: MenuAction;
  /** What's on the desktop, as `ref:key`, for Pin to desktop. Without it there's no such item. */
  desktop?: ReadonlySet<string>;
  /** Archives or unarchives it, where that does more (an undo in the main window). */
  archive?: (archived: boolean) => void;
  /** It left the list it was picked from, and how: the peek goes away, the quick panel's queue
   *  goes on to the next. */
  left?: (done: 'marked seen' | 'snoozed' | 'archived') => void;
  /** Add to canvas, where there's a canvas. */
  canvas?: MenuSpec;
}

/**
 * A session's menu: opening it (where it lives, its folder, its PR), showing or answering it in
 * Codealong, starting from it; the window's own (todos); what it is to you (its turn stopped,
 * its name, seen, pinned, snoozed, closed, archived); copying it.
 */
export function sessionMenu(lane: Lane, s: Session, can: SessionCan): MenuSpec {
  const { report } = can;
  const here = !shortOf(s.key);
  const busy = <T>(label: string, work: Promise<T>) => (report.busy ? report.busy(label, work) : work);
  const settle = (done: 'marked seen' | 'snoozed' | 'archived') => (act: Promise<unknown>) => {
    can.left?.(done);
    act.catch((e: unknown) => report.error(errorText(e)));
  };
  const fork = () =>
    busy(`Forking ${s.title}…`, forkSession(s.key))
      .then((started) => {
        if (started?.message) report.info(started.message);
        if (started) can.forked?.(started);
      })
      .catch((e: unknown) => report.error(errorText(e)));
  const stop = () =>
    busy(`Stopping ${s.title}…`, sessionCancel(s.key))
      .then((said) => said && report.info(said))
      .catch((e: unknown) => report.error(errorText(e)));
  const close = () =>
    busy(`Closing ${s.title}…`, closeSession(s.key))
      .then((said) => said && report.info(said))
      .catch((e: unknown) => report.error(errorText(e)));
  const archive = can.archive ?? ((archived: boolean) => (archived ? settle('archived')(archiveAhead(s.key, true)) : void archiveAhead(s.key, false).catch((e: unknown) => report.error(errorText(e)))));
  return sections(
    can.heading ? [{ label: `${s.title} · ${lane.callsign}`, enabled: false, run: () => {} }] : [],
    [
      { label: openLabel(lane, s, can.terminal), run: () => can.open(lane, s.openTarget) },
      ...(can.editors && opensInEditor(lane) ? folderMenu(can.editors.apps, can.editors.icons, (id) => can.open(lane, id)) : []),
      ...prItem(lane, report),
      ...(can.reveal ? [{ label: 'Show in Codealong', run: can.reveal }] : []),
      ...(can.reply ? [{ label: 'Reply here', run: can.reply }] : []),
      ...newSessionItem(lane, can.newSession),
      // Forks open in a terminal, where only sessions that resume in one can go.
      ...(s.resumeCommand ? [{ label: 'Fork in terminal', run: () => void fork() }] : []),
    ],
    can.todos ?? [],
    [
      // The session carries on, waiting for you.
      ...(stoppable(s) ? [{ label: 'Stop turn', run: () => void stop() }] : []),
      ...(can.rename && renames(s) ? [{ label: 'Rename…', run: can.rename }] : []),
      ...(seenable(s) ? [{ label: 'Mark as seen', run: () => settle('marked seen')(seenAhead([s.key])) }] : []),
      ...(s.archived ? [] : [...(can.pin ? [can.pin] : []), ...(can.desktop ? [desktopItem('session', s.key, can.desktop, report)] : [])]),
      // Snoozes keep sessions off the floating bar, which shows this device's only.
      ...(s.archived || !here ? [] : [snoozeMenu([s.key], Date.now(), { until: s.snoozedUntil, onDone: settle('snoozed') })]),
      ...(closable(s) ? [{ label: s.attachCommand ? 'End background job' : 'Close terminal', run: () => void close() }] : []),
      { label: s.archived ? 'Unarchive' : s.harness === 'codex' ? 'Archive in Codex' : 'Archive', run: () => archive(!s.archived) },
    ],
    [
      ...copyItem('Copy session ID', s.id, 'session ID', report),
      ...copyItem('Copy resume command', s.resumeCommand, 'resume command', report),
      ...(s.visitor ? [] : (can.canvas ?? [])),
    ],
  );
}

export interface LaneCan {
  report: Report;
  /** Opens one of its targets: its terminal, an editor, Finder, its app in the browser. */
  open: (lane: Lane, target: string) => void;
  /** Shows it in the main window, from the other windows. */
  reveal?: () => void;
  editors?: Editors;
  /** New session here…, when it's more than the quick panel on the worktree. */
  newSession?: () => void;
  /** Set preview URL…, where there's a dialog for it. */
  setPreview?: () => void;
  /** Add to todo and Remove from todo, where the todos are. */
  todos?: MenuSpec;
  /** Pins it to this window's own list. */
  pin?: MenuAction;
  /** What's on the desktop, as `ref:key`, for Pin to desktop. */
  desktop?: ReadonlySet<string>;
  canvas?: MenuSpec;
}

/** A worktree's menu: its default open, then its editors, Finder and app, its PR, showing it
 *  and starting a session in it; the window's own; pins; copying its path, branch and links. */
export function laneMenu(lane: Lane, can: LaneCan): MenuSpec {
  const { report } = can;
  const main = lane.targets.find((t) => t.id === lane.defaultTarget);
  const other = (id: string): MenuSpec => {
    const t = lane.targets.find((x) => x.id === id);
    return t && id !== lane.defaultTarget ? [{ label: t.label, enabled: !t.disabled, run: () => can.open(lane, id) }] : [];
  };
  const defaultTarget = lane.defaultTarget;
  return sections(
    [
      ...(defaultTarget ? [{ label: main?.label ?? 'Open', run: () => can.open(lane, defaultTarget) }] : []),
      // Open in <yours>, then the other editors installed here.
      ...other('l|editor'),
      ...(can.editors && opensInEditor(lane) ? openInMenu(can.editors.apps, can.editors.icons, (id) => can.open(lane, id)) : []),
      ...other('l|finder'),
      ...other('l|preview'),
      ...prItem(lane, report),
      ...(can.reveal ? [{ label: 'Show in Codealong', run: can.reveal }] : []),
      ...newSessionItem(lane, can.newSession),
      ...(can.setPreview && !lane.device ? [{ label: lane.preview?.set ? 'Change preview URL…' : 'Set preview URL…', run: can.setPreview }] : []),
    ],
    can.todos ?? [],
    [...(can.pin ? [can.pin] : []), ...(can.desktop ? [desktopItem('worktree', lane.id, can.desktop, report)] : [])],
    [
      ...copyItem('Copy path', lane.path, 'worktree path', report),
      ...copyItem('Copy branch name', lane.branch, 'branch name', report),
      ...copyItem('Copy PR link', lane.pr?.url, 'PR link', report),
      ...copyItem('Copy preview URL', lane.preview?.url, 'preview URL', report),
      ...(can.canvas ?? []),
    ],
  );
}

export interface PrCan {
  report: Report;
  /** Opens it on GitHub, where that does more (the quick panel closes first). */
  open?: () => void;
  /** Opens the session that last worked on it, where it lives. */
  session?: { lane: Lane; session: Session; open: () => void };
  /** Review with…'s choices: the Codex app, or a session on a review prompt. */
  review?: MenuAction[];
  /** Shows it in the main window, from the other windows. */
  reveal?: () => void;
  /** Approves it, when you're asked to review it and it's open. */
  approve?: () => void;
  /** Shows the worktree it's checked out in. */
  showLane?: { lane: Lane; run: () => void };
  canvas?: MenuSpec;
}

/** A PR's menu: opening it, its session, reviewing it, showing it; approving it; its worktree
 *  and copying its link and branch. */
export function prMenu(pr: Pick<Pr, 'url' | 'number'> & { head?: string | null }, can: PrCan): MenuSpec {
  const { report } = can;
  const s = can.session;
  return sections(
    [
      { label: 'Open on GitHub', run: can.open ?? (() => void openUrl(pr.url).catch((e: unknown) => report.error(errorText(e)))) },
      ...(s ? [{ label: s.session.live ? 'Open its session' : 'Open its latest session', run: s.open }] : []),
      ...(can.review ? [{ label: 'Review with', items: can.review }] : []),
      ...(can.reveal ? [{ label: 'Show in Codealong', run: can.reveal }] : []),
    ],
    can.approve ? [{ label: `Approve #${pr.number}`, run: can.approve }] : [],
    [
      ...(can.showLane ? [{ label: `Show worktree ${can.showLane.lane.callsign}`, run: can.showLane.run }] : []),
      ...copyItem('Copy link', pr.url, 'PR link', report),
      ...copyItem('Copy branch name', pr.head, 'branch name', report),
      ...(can.canvas ?? []),
    ],
  );
}

export interface TodoCan {
  report: Report;
  /** Start session…: the quick panel on its prompt. */
  start: () => void;
  /** Its session, the running one else the latest, to open where it lives. */
  session?: { lane: Lane; session: Session; open: () => void };
  /** Shows it in the main window, from the other windows. */
  reveal?: () => void;
  /** Its status, labels and project, where the todos are kept (the main window). */
  edit?: MenuSpec;
  /** What's on the desktop, as `ref:key`, for Pin to desktop. */
  desktop?: ReadonlySet<string>;
  /** Copy as prompt: its prompt, attachments as their paths. */
  copyPrompt?: () => void;
  canvas?: MenuSpec;
  /** Delete todo. */
  remove?: () => void;
}

/** A todo's menu: starting a session for it, opening its session, showing it; the window's own
 *  (status, labels, project); pinning it; copying it; deleting it. */
export function todoMenu(id: string, can: TodoCan): MenuSpec {
  const { report } = can;
  const s = can.session;
  return sections(
    [
      { label: 'Start session…', run: can.start },
      ...(s ? [{ label: `Open ${s.session.account}: ${s.session.title.slice(0, 48)}`, run: s.open }] : []),
      ...(can.reveal ? [{ label: 'Show in Codealong', run: can.reveal }] : []),
    ],
    can.edit ?? [],
    can.desktop ? [desktopItem('todo', id, can.desktop, report)] : [],
    [...(can.copyPrompt ? [{ label: 'Copy as prompt', run: can.copyPrompt }] : []), ...copyItem('Copy todo ID', id, 'todo ID', report), ...(can.canvas ?? [])],
    can.remove ? [{ label: 'Delete todo', run: can.remove }] : [],
  );
}
