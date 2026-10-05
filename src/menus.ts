// Right-click menus for what Codealong lists (sessions, worktrees, PRs, todos), each built in one
// place so it reads the same wherever it's right-clicked: the main window's lists and panels, the
// floating bar's pills, the peek, the quick panel's rows, cards on the desktop. A window says
// what it can do (`SessionCan` and the others), and an item shows when the window can do it and
// the thing allows it, always in the same order. What works the same from every window (mark
// seen, snooze, close, archive, fork, move, pin to the desktop, copy) is done here, and says how
// it went through the window's `Report`: a toast in the main window, a note on the bar.

import { useEffect, useState } from 'react';
import { closeSession, desktopPins, expandAttachments, forkSession, getAccounts, moveSession, onDesktopPins, openUrl, pinToDesktop, sessionCancel, setReusable, showQuick, unpinFromDesktop } from './api';
import { archiveAhead, seenAhead } from './ahead';
import type { MenuAction, MenuGroup, MenuSpec } from './contextMenu';
import { shortOf } from './devices';
import { dueChoices } from './due';
import { folderMenu, openInMenu, opensInEditor, type AppIcons, type Apps } from './editors';
import { MOVES_ACCOUNTS, renames } from './harness';
import { chromeLabel, openLabel } from './open';
import { closable, seenable, stoppable } from './lanes';
import { QUEUE_CHORDS } from './queue';
import { snoozeChordIndex, snoozeMenu } from './snooze';
import { STATUSES, STATUS_WORD, todosPrompt, waitingChange, type TodoOp } from './todos';
import type { AccountInfo, DesktopPinned, Label, Lane, Pr, Session, Todo } from './types';
import { errorText, type Report } from './report';

/** The editors installed here and their icons, for Open folder in and Open in. */
export interface Editors {
  apps: Apps;
  icons: AppIcons;
}

// The accounts in config.toml, for Move to: a menu is built at once, so they're read ahead, as a
// window loads this, and again as a session's menu is built (a minute apart at most), for the
// next one to have an account added or signed in since.
let accounts: AccountInfo[] = [];
let accountsRead = -Infinity;

function readAccounts(): void {
  if (Date.now() - accountsRead < 60_000) return;
  accountsRead = Date.now();
  getAccounts().then(
    (list) => (accounts = list),
    () => {},
  );
}

readAccounts();

/** Groups of items, with a line between the ones that have any. */
function sections(...groups: MenuSpec[]): MenuSpec {
  return groups.filter((g) => g.length > 0).flatMap((g, i): MenuSpec => (i === 0 ? g : ['separator', ...g]));
}

function copyItem(label: string, text: string | null | undefined, what: string, report: Report): MenuAction | undefined {
  if (!text) return undefined;
  const copy = () =>
    navigator.clipboard.writeText(text).then(
      () => report.info(`Copied the ${what}.`),
      () => report.error(`Couldn't copy the ${what}.`),
    );
  return { label, run: () => void copy() };
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

/** Opens a worktree's PR on GitHub. */
function prItem(lane: Lane, report: Report): MenuAction | undefined {
  const pr = lane.pr;
  return pr ? { label: `Open PR #${pr.number} on GitHub`, run: () => void openUrl(pr.url).catch((e: unknown) => report.error(errorText(e))) } : undefined;
}

/** Reuse for new work, checked while the worktree takes one piece of work after another: Jev
 *  suggests new work only in reusable worktrees. Only a repo's worktree in one of your projects
 *  can be (board::can_reuse), and only this device's is marked here. */
function reuseItem(lane: Lane, report: Report): MenuAction | undefined {
  if (lane.device || lane.project === 'Elsewhere' || lane.origin === 'folder') return undefined;
  const on = lane.reuse != null;
  return { label: 'Reuse for new work', checked: on, run: () => void setReusable(lane.id, !on).catch((e: unknown) => report.error(errorText(e))) };
}

/** New session here…: the quick panel on the worktree, from any window. The panel starts
 *  sessions on this device, so another device's worktree needs a window that says otherwise. */
function newSessionItem(lane: Lane, run: (() => void) | undefined): MenuAction | undefined {
  if (!run && shortOf(lane.id)) return undefined;
  return { label: 'New session here…', run: run ?? (() => void showQuick({ laneId: lane.id, project: lane.project })) };
}

/** How a session left the list of what waits on you. */
export type Left = 'marked seen' | 'snoozed' | 'closed' | 'archived';

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
   *  and the notch's go on to the next. */
  left?: (done: Left) => void;
  /** Add to canvas, where there's a canvas. */
  canvas?: MenuSpec;
  /** Shows the main window's keys beside the actions they run (`KEYS`), or its chords (`CHORDS`)
   *  where letters alone do nothing (Sessions). */
  keys?: boolean | 'chords';
}

/** A session's actions by name, each once: the menu lays them out (`sessionMenu`), a header
 *  shows a few as buttons (SessionHeader.tsx), and the main window's keys run some (`KEYS`). An
 *  action is missing when the window can't do it or the session doesn't allow it, so the rule for
 *  when each applies is here and nowhere else. */
export interface SessionActions {
  open: MenuAction;
  folder?: MenuAction | MenuGroup;
  pr?: MenuAction;
  chrome?: MenuAction;
  reveal?: MenuAction;
  reply?: MenuAction;
  newSession?: MenuAction;
  fork?: MenuAction;
  move?: MenuAction | MenuGroup;
  /** The move a usage limit's stop suggests: to the account with room (`Session.limit`). */
  moveOn?: MenuAction;
  stop?: MenuAction;
  rename?: MenuAction;
  seen?: MenuAction;
  pin?: MenuAction;
  desktop?: MenuAction;
  snooze?: MenuGroup;
  close?: MenuAction;
  archive: MenuAction;
  copyId: MenuAction;
  copyResume?: MenuAction;
}

export function sessionActions(lane: Lane, s: Session, can: SessionCan): SessionActions {
  const { report } = can;
  const here = !shortOf(s.key);
  const busy = <T>(label: string, work: Promise<T>) => (report.busy ? report.busy(label, work) : work);
  const said = (label: string, work: Promise<string | null | undefined | void>) =>
    busy(label, work)
      .then((words) => words && report.info(words))
      .catch((e: unknown) => report.error(errorText(e)));
  const settle = (done: Left) => (act: Promise<unknown>) => {
    can.left?.(done);
    act.catch((e: unknown) => report.error(errorText(e)));
  };
  const fork = () =>
    busy(`Forking ${s.title}…`, forkSession(s.key))
      .then((started) => {
        if (started?.message) report.info(started.message);
      })
      .catch((e: unknown) => report.error(errorText(e)));
  // Moving opens a terminal on this Mac, as a fork does, and closes the session's own.
  const moveTo = here && s.resumeCommand && MOVES_ACCOUNTS.includes(s.harness) ? accounts.filter((a) => a.harness === s.harness && a.name !== s.account && a.signedIn).map((a) => a.name) : [];
  const move = (to: string) =>
    busy(`Moving ${s.title} to ${to}…`, moveSession(s.key, to))
      .then((started) => {
        can.left?.('archived');
        if (started?.message) report.info(started.message);
      })
      .catch((e: unknown) => report.error(errorText(e)));
  const moveItem = (to: string, label: string): MenuAction => ({ label, run: () => void move(to) });
  // A session a usage limit stopped is told to go on where it moves (actions.rs), so it says so.
  const goesOn = s.limit && s.status !== 'working' ? ' and go on' : '';
  const suggested = s.limit?.moveTo && moveTo.includes(s.limit.moveTo) && goesOn ? s.limit.moveTo : null;
  const archive = can.archive ?? ((archived: boolean) => (archived ? settle('archived')(archiveAhead(s.key, true)) : void archiveAhead(s.key, false).catch((e: unknown) => report.error(errorText(e)))));
  return {
    open: { label: openLabel(lane, s, can.terminal), run: () => can.open(lane, s.openTarget) },
    folder: can.editors && opensInEditor(lane) ? item(folderMenu(can.editors.apps, can.editors.icons, (id) => can.open(lane, id), { name: can.terminal, key: s.key })) : undefined,
    pr: prItem(lane, report),
    // Its tab in Chrome, in whichever profile's window has it; Chrome here, so not for a session
    // on another device.
    chrome: here && s.chrome ? { label: chromeLabel(s.chrome), run: () => can.open(lane, `s|${s.key}|chrome`) } : undefined,
    reveal: can.reveal && { label: 'Show in Codealong', run: can.reveal },
    reply: can.reply && { label: 'Reply here', run: can.reply },
    newSession: newSessionItem(lane, can.newSession),
    // Forks open in a terminal, where only sessions that resume in one can go.
    fork: s.resumeCommand ? { label: 'Fork in terminal', run: () => void fork() } : undefined,
    move:
      moveTo.length > 1
        ? { label: `Move${goesOn} to`, items: moveTo.map((to) => moveItem(to, to === suggested ? `${to} (has room)` : to)) }
        : moveTo.length === 1
          ? moveItem(moveTo[0], `Move to ${moveTo[0]}${goesOn}`)
          : undefined,
    moveOn: suggested ? moveItem(suggested, `Move to ${suggested} and go on`) : undefined,
    // The session carries on, waiting for you.
    stop: stoppable(s) ? { label: 'Stop turn', run: () => void said(`Stopping ${s.title}…`, sessionCancel(s.key)) } : undefined,
    rename: can.rename && renames(s) ? { label: 'Rename…', run: can.rename } : undefined,
    seen: seenable(s) ? { label: 'Mark as seen', run: () => settle('marked seen')(seenAhead([s.key])) } : undefined,
    pin: s.archived ? undefined : can.pin,
    desktop: !s.archived && can.desktop ? desktopItem('session', s.key, can.desktop, report) : undefined,
    // Snoozes keep sessions off the floating bar, which shows this device's only.
    snooze: s.archived || !here ? undefined : snoozeMenu([s.key], Date.now(), { until: s.snoozedUntil, onDone: settle('snoozed'), chords: !!can.keys }),
    // Closed, it's done with for now, as one marked seen is: a window working through what
    // waits on you goes on.
    close: closable(s)
      ? {
          label: s.attachCommand ? 'End background job' : 'Close terminal',
          run: () => {
            can.left?.('closed');
            void said(`Closing ${s.title}…`, closeSession(s.key));
          },
        }
      : undefined,
    archive: { label: s.archived ? 'Unarchive' : s.harness === 'codex' ? 'Archive in Codex' : 'Archive', run: () => archive(!s.archived) },
    copyId: copyItem('Copy session ID', s.id, 'session ID', report) as MenuAction,
    copyResume: copyItem('Copy resume command', s.resumeCommand, 'resume command', report),
  };
}

/** The order of a session's menu, in groups with a line between: opening it (where it lives,
 *  its folder, its PR, its tabs in Chrome), showing or answering it in Codealong and starting from it; the window's
 *  todos; what it is to you (its turn, its name, seen, pinned, snoozed, closed, archived);
 *  copying it. */
const SESSION_MENU = [
  ['heading'],
  ['open', 'folder', 'pr', 'chrome', 'reveal', 'reply', 'newSession', 'fork', 'move'],
  ['todos'],
  ['stop', 'rename', 'seen', 'pin', 'desktop', 'snooze', 'close', 'archive'],
  ['copyId', 'copyResume', 'canvas'],
] as const;

/** A session's menu, as every window has it. */
export function sessionMenu(lane: Lane, s: Session, can: SessionCan): MenuSpec {
  readAccounts();
  const extras = {
    heading: can.heading ? [{ label: `${s.title} · ${lane.callsign}`, enabled: false, run: () => {} }] : [],
    todos: can.todos ?? [],
    canvas: s.visitor ? [] : (can.canvas ?? []),
  };
  return laidOut(SESSION_MENU, sessionActions(lane, s, can), extras, can.keys);
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
  /** Shows the main window's keys beside the actions they run (`KEYS`). */
  keys?: boolean;
}

/** A worktree's actions by name, as `SessionActions` are a session's. */
export interface LaneActions {
  open?: MenuAction;
  editor?: MenuAction;
  openIn?: MenuGroup;
  finder?: MenuAction;
  preview?: MenuAction;
  pr?: MenuAction;
  reveal?: MenuAction;
  newSession?: MenuAction;
  setPreview?: MenuAction;
  reuse?: MenuAction;
  pin?: MenuAction;
  desktop?: MenuAction;
  copyPath?: MenuAction;
  copyBranch?: MenuAction;
  copyPr?: MenuAction;
  copyPreview?: MenuAction;
}

export function laneActions(lane: Lane, can: LaneCan): LaneActions {
  const { report } = can;
  const target = (id: string | null | undefined, label?: string) => {
    const t = lane.targets.find((x) => x.id === id);
    return id && t ? { label: label ?? t.label, enabled: !t.disabled, run: () => can.open(lane, id) } : undefined;
  };
  // The default open first, whatever it is; the others where they aren't it.
  const other = (id: string) => (id === lane.defaultTarget ? undefined : target(id));
  const main = lane.defaultTarget;
  return {
    open: main ? { label: lane.targets.find((t) => t.id === main)?.label ?? 'Open', run: () => can.open(lane, main) } : undefined,
    // Open in <yours>, then the other editors installed here.
    editor: other('l|editor'),
    openIn: can.editors && opensInEditor(lane) ? (item(openInMenu(can.editors.apps, can.editors.icons, (id) => can.open(lane, id))) as MenuGroup | undefined) : undefined,
    finder: other('l|finder'),
    preview: other('l|preview'),
    pr: prItem(lane, report),
    reveal: can.reveal && { label: 'Show in Codealong', run: can.reveal },
    newSession: newSessionItem(lane, can.newSession),
    setPreview: can.setPreview && !lane.device ? { label: lane.preview?.set ? 'Change preview URL…' : 'Set preview URL…', run: can.setPreview } : undefined,
    reuse: reuseItem(lane, report),
    pin: can.pin,
    desktop: can.desktop ? desktopItem('worktree', lane.id, can.desktop, report) : undefined,
    copyPath: copyItem('Copy path', lane.path, 'worktree path', report),
    copyBranch: copyItem('Copy branch name', lane.branch, 'branch name', report),
    copyPr: copyItem('Copy PR link', lane.pr?.url, 'PR link', report),
    copyPreview: copyItem('Copy preview URL', lane.preview?.url, 'preview URL', report),
  };
}

/** The order of a worktree's menu: opening it, its PR, showing it and starting in it; the
 *  window's todos; whether it's reused; pins; copying its path, branch and links. */
const LANE_MENU = [
  ['open', 'editor', 'openIn', 'finder', 'preview', 'pr', 'reveal', 'newSession', 'setPreview'],
  ['todos'],
  ['reuse'],
  ['pin', 'desktop'],
  ['copyPath', 'copyBranch', 'copyPr', 'copyPreview', 'canvas'],
] as const;

/** A worktree's menu, as every window has it. */
export function laneMenu(lane: Lane, can: LaneCan): MenuSpec {
  return laidOut(LANE_MENU, laneActions(lane, can), { todos: can.todos ?? [], canvas: can.canvas ?? [] }, can.keys);
}

/** The main window's keys for actions, shown beside them in its menus. */
export const KEYS: Partial<Record<keyof SessionActions | keyof LaneActions, string>> = { seen: 'E', pin: 'P', archive: 'A', rename: 'R' };
/** The same actions' chords, which work in every view and from Sessions' box, where letters alone
 *  do nothing: ⌘E marks seen (and goes on, where a window works through what waits on you: it's
 *  the queue's key, queue.ts), ⌃⌘ and the letter runs the rest. Closing a session's terminal has
 *  a chord and no letter of its own: it's not one to press by mistake. */
export const CHORDS: typeof KEYS = { seen: QUEUE_CHORDS.seen[0], pin: 'Ctrl+Cmd+P', archive: 'Ctrl+Cmd+A', rename: 'Ctrl+Cmd+R', close: 'Ctrl+Cmd+W' };
// Snoozing has a chord for each of its times, ⌃⌘1 to ⌃⌘4 (snooze.ts `SNOOZE_CHORDS`), which its
// menu shows beside them. The keys for getting from one session to the next are queue.ts
// `QUEUE_CHORDS`, written the same way.
const CTRL_CMD = 'Ctrl+Cmd+';

/** Whether ⌃⌘ with this key is one of the chords: an action's letter, or a snooze's digit. */
export function chordLetter(key: string): boolean {
  return Object.values(CHORDS).includes(CTRL_CMD + key.toUpperCase()) || snoozeChordIndex(key) >= 0;
}

/** The time ⌃⌘ with a digit snoozes a session to (snooze.ts `SNOOZE_CHORDS`), as its Snooze menu
 *  has it now; none for one that can't be snoozed, or a key that isn't one of them. */
function snoozeChorded(actions: SessionActions | LaneActions, key: string): MenuAction | null {
  const snooze = 'snooze' in actions ? actions.snooze : undefined;
  const it = snooze?.items[snoozeChordIndex(key)];
  return it && it !== 'separator' && !('items' in it) ? it : null;
}

/** A chord as the keys' own signs, for a button that says it: `Ctrl+Cmd+W` is ⌃⌘W, and
 *  `Alt+Shift+Down` ⌥⇧↓. */
export function chordSigns(chord: string): string {
  const signs: Record<string, string> = { Ctrl: '⌃', Cmd: '⌘', Alt: '⌥', Shift: '⇧', Up: '↑', Down: '↓', Esc: 'esc' };
  return chord
    .split('+')
    .map((k) => signs[k] ?? k)
    .join('');
}

/** The action a key runs among a session's or worktree's, by the key as typed, when it applies
 *  to them now: the letter alone (`KEYS`), or with `chord` the letter pressed with ⌃⌘ (`CHORDS`). */
export function keyed(actions: SessionActions | LaneActions, key: string, chord = false): { id: string; action: MenuAction } | null {
  const snooze = chord ? snoozeChorded(actions, key) : null;
  if (snooze) return { id: 'snooze', action: snooze };
  const letter = key.toUpperCase();
  const id = chord ? Object.keys(CHORDS).find((name) => CHORDS[name as keyof typeof CHORDS] === CTRL_CMD + letter) : Object.keys(KEYS).find((name) => KEYS[name as keyof typeof KEYS] === letter);
  const it = id ? (actions as Record<string, Item | undefined>)[id] : undefined;
  return id && it && !('items' in it) ? { id, action: it } : null;
}

/** The action a chord runs among a session's (`CHORDS`), when it applies to the session now:
 *  for the windows that show one session with a box to answer it, where letters are typed. */
export function chorded(actions: SessionActions, e: Pick<KeyboardEvent, 'key' | 'metaKey' | 'ctrlKey' | 'altKey' | 'shiftKey'>): MenuAction | null {
  if (!e.metaKey || e.altKey || e.shiftKey) return null;
  const snooze = e.ctrlKey ? snoozeChorded(actions, e.key) : null;
  if (snooze) return snooze;
  const chord = `${e.ctrlKey ? 'Ctrl+' : ''}Cmd+${e.key.toUpperCase()}`;
  const id = Object.keys(CHORDS).find((name) => CHORDS[name as keyof typeof CHORDS] === chord);
  const it = id ? (actions as unknown as Record<string, Item | undefined>)[id] : undefined;
  return it && !('items' in it) ? it : null;
}

type Item = MenuAction | MenuGroup;

/** A menu from a layout: each group's actions that are there (or the window's extras in their
 *  places), a line between groups that have any, and with `keys` each action's key beside it. */
function laidOut<A extends object>(layout: readonly (readonly string[])[], actions: A, extras: Record<string, MenuSpec>, keys?: boolean | 'chords'): MenuSpec {
  const byName = actions as Record<string, Item | undefined>;
  const group = (names: readonly string[]): MenuSpec =>
    names.flatMap((name): MenuSpec => {
      if (name in extras) return extras[name];
      const it = byName[name];
      if (!it) return [];
      const key = keys ? (keys === 'chords' ? CHORDS : KEYS)[name as keyof typeof KEYS] : undefined;
      return [key && !('items' in it) ? { ...it, accelerator: key } : it];
    });
  return sections(...layout.map(group));
}

/** An item that may not be there, as a list of it or of nothing. */
function some<T>(x: T | undefined): T[] {
  return x ? [x] : [];
}

/** The one item a helper that returns a list of none or one gave. */
function item<T extends MenuSpec[number]>(spec: T[]): Exclude<T, 'separator'> | undefined {
  const [first] = spec;
  return first === 'separator' ? undefined : (first as Exclude<T, 'separator'>);
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
  /** Snooze review (snooze.ts `reviewSnoozeMenu`), when it waits on your review. */
  snooze?: MenuGroup;
  /** Shows the worktree it's checked out in. */
  showLane?: { lane: Lane; run: () => void };
  /** Pin PR or Unpin PR, where PRs are listed (the main window). */
  pin?: MenuAction;
  /** What's on the desktop, as `ref:key`, for Pin to desktop: for a PR the board lists, which
   *  has a key. */
  desktop?: ReadonlySet<string>;
  canvas?: MenuSpec;
}

/** A PR's menu: opening it, its session, reviewing it, showing it; approving it and snoozing your
 *  review; pinning it; its worktree and copying its link and branch. */
export function prMenu(pr: Pick<Pr, 'url' | 'number'> & { head?: string | null; key?: string }, can: PrCan): MenuSpec {
  const { report } = can;
  const s = can.session;
  return sections(
    [
      { label: 'Open on GitHub', run: can.open ?? (() => void openUrl(pr.url).catch((e: unknown) => report.error(errorText(e)))) },
      ...(s ? [{ label: s.session.live ? 'Open its session' : 'Open its latest session', run: s.open }] : []),
      ...(can.review ? [{ label: 'Review with', items: can.review }] : []),
      ...(can.reveal ? [{ label: 'Show in Codealong', run: can.reveal }] : []),
    ],
    [...(can.approve ? [{ label: `Approve #${pr.number}`, run: can.approve }] : []), ...some(can.snooze)],
    [...some(can.pin), ...(can.desktop && pr.key ? [desktopItem('pr', pr.key, can.desktop, report)] : [])],
    [
      ...(can.showLane ? [{ label: `Show worktree ${can.showLane.lane.callsign}`, run: can.showLane.run }] : []),
      ...some(copyItem('Copy link', pr.url, 'PR link', report)),
      ...some(copyItem('Copy branch name', pr.head, 'branch name', report)),
      ...(can.canvas ?? []),
    ],
  );
}

/** How a window changes todos, for the items that do: status, waiting, due date, labels and
 *  project. An item shows only where the window has what it needs. */
export interface TodoEdit {
  /** Runs a change: through the main window's store, which shows it at once, or straight to the
   *  engine (`todoOp`), whose board shows it. */
  run: (op: TodoOp) => void;
  /** Every todo, for Mark as waiting and Stop waiting: what a todo waits on is other todos. */
  todos?: Todo[];
  /** The labels to put on a todo or take off, for Labels. */
  labels?: Label[];
  /** The projects a todo can be in, for Move to project. */
  projects?: string[];
  /** Pick a date…: the todo panel's date picker. */
  pickDate?: (t: Todo) => void;
  /** Says what stopping waits changed, with the changes that put it back (the main window's
   *  undo). Without it, the window's report says it. */
  stopped?: (said: string, undo: TodoOp[]) => void;
}

/** Mark as waiting or Stop waiting (`waitingChange`), from a menu or a key. */
export function toggleWaiting(ts: Todo[], edit: TodoEdit, report: Report) {
  const w = edit.todos && waitingChange(ts, edit.todos);
  if (!w) return;
  for (const op of w.ops) edit.run(op);
  if (w.said) {
    if (edit.stopped) edit.stopped(w.said, w.undo);
    else report.info(w.said);
  }
}

/** Todo, In progress and Done, the one they're all at checked. */
function statusItems(ts: Todo[], edit: TodoEdit): MenuSpec {
  return STATUSES.map((st) => ({ label: STATUS_WORD[st], checked: ts.every((t) => t.status === st), run: () => ts.forEach((t) => edit.run({ op: 'update', id: t.id, patch: { status: st } })) }));
}

function waitingItems(ts: Todo[], edit: TodoEdit, report: Report): MenuSpec {
  const w = edit.todos && waitingChange(ts, edit.todos);
  return w ? [{ label: w.label, run: () => toggleWaiting(ts, edit, report) }] : [];
}

/** The Due date submenu: the days `dueChoices` offers, any day in the panel's picker (for one
 *  todo), and taking it off. A day is checked when every todo is due then. */
function dueItem(ts: Todo[], edit: TodoEdit): MenuGroup {
  const set = (due: string | null) => ts.forEach((t) => edit.run({ op: 'update', id: t.id, patch: { due } }));
  const same = ts.every((t) => t.due === ts[0].due) ? ts[0].due : null;
  const pick = edit.pickDate;
  return {
    label: 'Due date',
    items: [
      ...dueChoices(Date.now()).map((c) => ({ label: c.label, checked: same === c.due, run: () => set(c.due) })),
      ...(ts.length === 1 && pick ? [{ label: 'Pick a date…', run: () => pick(ts[0]) }] : []),
      ...(ts.some((t) => t.due) ? ['separator' as const, { label: ts.length === 1 ? 'Remove due date' : 'Remove due dates', run: () => set(null) }] : []),
    ],
  };
}

/** Labels: each one, checked where the todo wears it, putting it on or taking it off. */
function labelsItems(t: Todo, edit: TodoEdit): MenuSpec {
  const labels = edit.labels;
  if (!labels) return [];
  const items = labels.map((l) => ({
    label: l.name,
    checked: t.labels.includes(l.id),
    run: () => edit.run({ op: 'update', id: t.id, patch: { labels: t.labels.includes(l.id) ? t.labels.filter((x) => x !== l.id) : [...t.labels, l.id] } }),
  }));
  return [{ label: 'Labels', items }];
}

/** Move to project: the projects, the one it's in checked. */
function projectItems(t: Todo, edit: TodoEdit): MenuSpec {
  const projects = edit.projects;
  if (!projects) return [];
  return [{ label: 'Move to project', items: projects.map((p) => ({ label: p, checked: t.project === p, run: () => edit.run({ op: 'update', id: t.id, patch: { project: p } }) })) }];
}

/** Copy as prompt: the prompt the todos would start a session with, attachments as their paths. */
function copyPromptItem(ts: Todo[], report: Report): MenuAction {
  const what = ts.length > 1 ? 'todos as a prompt' : 'todo as a prompt';
  const copy = () =>
    expandAttachments(todosPrompt(ts)).then(
      (text) =>
        navigator.clipboard.writeText(text).then(
          () => report.info(`Copied the ${what}.`),
          () => report.error(`Couldn't copy the ${what}.`),
        ),
      (e: unknown) => report.error(errorText(e)),
    );
  return { label: 'Copy as prompt', run: () => void copy() };
}

export interface TodoCan {
  report: Report;
  /** Start session…: the quick panel on its prompt. */
  start: () => void;
  /** Its session, the running one else the latest, to open where it lives. */
  session?: { lane: Lane; session: Session; open: () => void };
  /** Shows it in the main window, from the other windows. */
  reveal?: () => void;
  /** Changing its status, waiting, due date, labels and project, as far as the window can. */
  edit?: TodoEdit;
  /** What's on the desktop, as `ref:key`, for Pin to desktop. */
  desktop?: ReadonlySet<string>;
  canvas?: MenuSpec;
  /** Delete todo, where it can be put back (the main window's undo). */
  remove?: () => void;
}

/** A todo's menu: starting a session for it, opening its session, showing it; its status,
 *  waiting, due date, labels and project; pinning it; copying it; deleting it. */
export function todoMenu(t: Todo, can: TodoCan): MenuSpec {
  const { report, edit } = can;
  const s = can.session;
  return sections(
    [
      { label: 'Start session…', run: can.start },
      ...(s ? [{ label: `Open ${s.session.account}: ${s.session.title.slice(0, 48)}`, run: s.open }] : []),
      ...(can.reveal ? [{ label: 'Show in Codealong', run: can.reveal }] : []),
    ],
    edit ? [...statusItems([t], edit), ...waitingItems([t], edit, report), 'separator', dueItem([t], edit), ...labelsItems(t, edit), ...projectItems(t, edit)] : [],
    can.desktop ? [desktopItem('todo', t.id, can.desktop, report)] : [],
    [copyPromptItem([t], report), ...some(copyItem('Copy todo ID', t.id, 'todo ID', report)), ...(can.canvas ?? [])],
    can.remove ? [{ label: 'Delete todo', run: can.remove }] : [],
  );
}

/** The menu of several todos picked together: one session for them all; their status, waiting
 *  and due date; copying them; deleting them. */
export function todosMenu(ts: Todo[], can: Pick<TodoCan, 'report' | 'start' | 'edit' | 'remove'>): MenuSpec {
  const { report, edit } = can;
  return sections(
    [{ label: `Start one session for ${ts.length} todos…`, run: can.start }],
    edit ? [...statusItems(ts, edit), ...waitingItems(ts, edit, report), dueItem(ts, edit)] : [],
    [copyPromptItem(ts, report), ...some(copyItem('Copy todo IDs', ts.map((t) => t.id).join(' '), 'todo IDs', report))],
    can.remove ? [{ label: `Delete ${ts.length} todos`, run: can.remove }] : [],
  );
}

/** The todo panel's ⋯ menu: what the panel has no control for. */
export function todoPanelMenu(t: Todo, can: Pick<TodoCan, 'report' | 'edit' | 'remove'>): MenuSpec {
  const { report, edit } = can;
  return sections([...(edit ? projectItems(t, edit) : []), copyPromptItem([t], report), ...some(copyItem('Copy todo ID', t.id, 'todo ID', report))], can.remove ? [{ label: 'Delete todo', run: can.remove }] : []);
}
