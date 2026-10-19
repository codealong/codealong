// Right-click menus for what Codealong lists (sessions, worktrees, PRs, todos), each built in one
// place so it reads the same wherever it's right-clicked: the main window's lists and panels, the
// floating bar's pills, the peek, the quick panel's rows, cards on the desktop. A window says
// what it can do (`SessionCan` and the others), and an item shows when the window can do it and
// the thing allows it, always in the same order. What works the same from every window (mark
// seen, snooze, quiet, close, mark done, archive, fork, move, pin to the desktop, copy) is done here, and
// says how it went through the window's `Report`: a toast in the main window, a note on the bar.

import { useEffect, useState } from 'react';
import { closeSession, desktopPins, expandAttachments, forkSession, getAccounts, inPreview, moveSession, moveSessionTo, onDesktopPins, openUrl, ownSessions, pinToDesktop, resumeSession, sessionCancel, setPrNudge, setPrPrimary, setReusable, showQuick, unpinFromDesktop } from './api';
import { archiveAhead, doneAhead, quietAhead, seenAhead, unreadAhead } from './ahead';
import type { MenuAction, MenuGroup, MenuImage, MenuSpec } from './contextMenu';
import { shortOf } from './devices';
import { dueChoices } from './due';
import { count } from './format';
import { folderMenu, openInMenu, opensInEditor, terminalTarget, type AppIcons, type Apps } from './editors';
import { HARNESS_WORD, MOVES_ACCOUNTS, MOVES_FOLDERS, renames } from './harness';
import { chromeLabel, openLabel } from './open';
import { closable, exitsItsOwnWay, seenable, stoppable, unreadable } from './lanes';
import { chordPressed, QUEUE_CHORDS, type Key } from './queue';
import { reviewsSnoozeMenu, SNOOZE_CHORDS, snoozeChordIndex, snoozeMenu } from './snooze';
import { rememberResumed, resumeAccount, resumedIn } from './start';
import { accountOf, startAccountsNow } from './startAccounts';
import { reviewChange, STATUSES, STATUS_WORD, todosPrompt, waitingChange, type TodoOp } from './todos';
import type { AccountInfo, DesktopPinned, Label, Lane, Pr, PrItem, Session, StartAccount, StartPlace, Started, Todo } from './types';
import { chordKeys as platformKeys, chordSigns as platformSigns, held as modsHeld } from './platform';
import { mayApprove } from './prStatus';
import { errorText, type Report } from './report';

/** The editors installed here and their icons, for Open folder in and Open in. */
export interface Editors {
  apps: Apps;
  icons: AppIcons;
}

// The accounts in config.toml, for Move to: a menu is built at once, so they're read ahead, as a
// window loads this, and again as a session's actions are built for its menu or its panel (a
// minute apart at most), for the next to have an account added or signed in since.
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

/** Clean up worktree…, where the window has a dialog to confirm it in (`cleanUp`): enabled for
 *  the ones that can go with nothing in them lost (board.rs `cleanup`), else shown dimmed with
 *  why. Not shown for a worktree whose git isn't read yet, nor one on a Mac whose Codealong
 *  doesn't say. */
export function cleanUpItem(lanes: Lane[], cleanUp: ((lanes: Lane[]) => void) | undefined): MenuAction | undefined {
  const known = lanes.filter((l) => l.cleanup);
  if (!cleanUp || known.length === 0) return undefined;
  const ok = known.filter((l) => l.cleanup?.ok);
  if (ok.length > 0) return { label: lanes.length === 1 ? 'Clean up worktree…' : `Clean up ${count(ok.length, 'worktree')}…`, run: () => cleanUp(ok) };
  const why = lanes.length === 1 ? known[0].cleanup?.why : 'none can go';
  return { label: `${lanes.length === 1 ? 'Clean up worktree' : 'Clean up worktrees'} (${why})`, enabled: false, run: () => {} };
}

/** New session here…: the quick panel on the worktree, from any window. The panel starts
 *  sessions on this device, so another device's worktree needs a window that says otherwise. */
function newSessionItem(lane: Lane, run: (() => void) | undefined): MenuAction | undefined {
  if (!run && shortOf(lane.id)) return undefined;
  return { label: 'New session here…', run: run ?? (() => void showQuick({ laneId: lane.id, project: lane.project })) };
}

/** How a session, or a PR waiting on your review, left the list of what waits on you: the strip
 *  says it on the one gone on to (queue.ts `wentWords`). */
export type Left = 'marked seen' | 'snoozed' | 'quieted' | 'closed' | 'done' | 'archived' | 'approved' | 'commented on' | 'changes requested' | 'review started' | 'merged';

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
  /** Answers it where this window does: the peek from the bar, the waiting window from the others. */
  reply?: () => void;
  editors?: Editors;
  /** New session here…, when it's more than the quick panel on the worktree. */
  newSession?: () => void;
  /** Add to todo and Remove from todo, where the todos are (the main window). */
  todos?: MenuSpec;
  /** Asks for a new name, where this window asks (a sheet, the peek, the waiting window). */
  rename?: () => void;
  /** The board's worktrees, for Move to worktree, where the window has the board. */
  worktrees?: Lane[];
  /** Pins it to this window's own list (the main window's Pinned). */
  pin?: MenuAction;
  /** What's on the desktop, as `ref:key`, for Pin to desktop. Without it there's no such item. */
  desktop?: ReadonlySet<string>;
  /** Archives or unarchives it, where that does more (an undo in the main window). */
  archive?: (archived: boolean) => void;
  /** Says what Mark as done did with a way back, where the window has one (the main window's
   *  Undo, which unarchives it). Elsewhere the session leaving the list says it was done, and
   *  `report` says only what didn't go as asked: a terminal left open, say. */
  undoDone?: (message: string) => void;
  /** It left the list it was picked from, and how: the peek goes away, the waiting window's queue
   *  and the notch's go on to the next. */
  left?: (done: Left) => void;
  /** It's moving, and `copy` gives the key of the session that goes on in its place (start.rs
   *  `move_to`), or null when there's none: it moved as itself, or the move failed. The window
   *  keeps it in view meanwhile and then shows the copy (the main window's Sessions). Without
   *  this, a session that leaves a copy has left the list, as an archived one has. */
  moving?: (copy: Promise<string | null>) => void;
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
  /** A session on this Mac that isn't running: each place its account's sessions go on in, on
   *  the account's own model (its box picks another). */
  resume?: MenuGroup;
  /** A new terminal in its folder, on this Mac (⌃`): the menu has it as the first of `folder`. */
  terminal?: MenuAction;
  folder?: MenuAction | MenuGroup;
  pr?: MenuAction;
  chrome?: MenuAction;
  reveal?: MenuAction;
  reply?: MenuAction;
  newSession?: MenuAction;
  fork?: MenuAction;
  move?: MenuAction | MenuGroup;
  /** To another worktree on this Mac, or a chat of its own, as itself. */
  moveFolder?: MenuGroup;
  /** The move a usage limit's stop suggests: to the account with room (`Session.limit`). */
  moveOn?: MenuAction;
  /** The accounts `move` goes to, for a window that asks which first (MoveAccount.tsx). */
  moveAccounts?: AccountMoves;
  /** A turn that was cut off goes on where its account's last session did, told to carry on
   *  (`Session.cutOff`, start.rs `CARRY_ON`). */
  carryOn?: MenuAction;
  stop?: MenuAction;
  rename?: MenuAction;
  seen?: MenuAction;
  /** Your turn again, until it's seen or prompted: for one whose turn you've seen. */
  unread?: MenuAction;
  pin?: MenuAction;
  desktop?: MenuAction;
  snooze?: MenuGroup;
  /** Quiet, checked while it is: its turns don't wait on you, only what needs you does, until
   *  it's unchecked (board.rs `apply_quiet`). */
  quiet?: MenuAction;
  /** A helper (Session.helper) made a session of your own, for good: listed and waiting on you
   *  as any of yours. */
  own?: MenuAction;
  close?: MenuAction;
  /** Done with it: it ends as far as Codealong can end it, and is marked seen and archived. */
  done?: MenuAction;
  archive: MenuAction;
  copyId: MenuAction;
  copyResume?: MenuAction;
}

/** Where a session can move to under another account of its harness, as Move to has them. */
export interface AccountMoves {
  /** The other accounts of its harness, signed in and on. */
  to: string[];
  /** The one a usage limit's stop has room on (`moveOn`). */
  suggested: string | null;
  /** The copy is told to carry on where a usage limit stopped it (actions.rs `move_session`). */
  goesOn: boolean;
  run: (to: string) => void;
}

export function sessionActions(lane: Lane, s: Session, can: SessionCan): SessionActions {
  readAccounts();
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
  // A session that moves out is archived, and the copy that goes on in its place is what a window
  // that follows it shows (`moving`); elsewhere, or with no copy, it has left the list. `leaves`
  // says whether it goes even when the move names no copy. Moving, it offers no other move
  // (`movedHere`).
  const moved = (label: string, work: () => Promise<Started | null>, leaves: boolean) => {
    if (movedHere.has(s.key)) return;
    movedHere.add(s.key);
    const copy = busy(label, work()).then((started) => {
      const key = started?.key && started.key !== s.key ? started.key : null;
      if (!key && !leaves) movedHere.delete(s.key);
      if ((leaves || key) && !(key && can.moving)) can.left?.('archived');
      if (started?.message) report.info(started.message);
      return key;
    });
    can.moving?.(copy.catch(() => null));
    copy.catch((e: unknown) => {
      movedHere.delete(s.key);
      report.error(errorText(e));
    });
  };
  const movable = here && !movedHere.has(s.key);
  // Moving opens a terminal on this Mac, as a fork does, and closes the session's own.
  const moveTo = movable && s.resumeCommand && MOVES_ACCOUNTS.includes(s.harness) ? accounts.filter((a) => a.harness === s.harness && a.name !== s.account && a.signedIn && !a.disabled).map((a) => a.name) : [];
  const move = (to: string) => moved(`Moving ${s.title} to ${to}…`, () => moveSession(s.key, to), true);
  const moveItem = (to: string, label: string): MenuAction => ({ label, run: () => move(to) });
  // Another folder: it's closed where it runs and goes on there as itself, so it stays on the
  // lists it's on, unless its harness copied it.
  const moveFolder = (to: Lane) => moved(`Moving ${s.title} to ${to.startsChats ? 'a chat of its own' : to.callsign}…`, () => moveSessionTo(s.key, to.id), false);
  // One that isn't running goes on where its account's sessions can (start.rs `resume`), as its
  // box offers, where there's more than the terminal Open resumes it in.
  const resumeTo = here && !s.live && !s.hosted && s.surface === 'cli' ? resumePlaces(s) : [];
  const cutOff = s.cutOff != null && s.status !== 'working';
  const carryTo = carriesOnIn(s);
  const resume = (to: StartAccount['places'][number]) =>
    busy(`Resuming ${s.title}…`, resumeSession(s.key, { place: to.place, model: null, effort: null }))
      .then((sent) => {
        rememberResumed(s.account, to.place);
        report.info(sent.message);
      })
      .catch((e: unknown) => report.error(errorText(e)));
  // Only a session Codealong can close and open again in a terminal moves: not an app's, nor one
  // Codealong runs itself.
  const movesFolders = movable && !!s.resumeCommand && MOVES_FOLDERS.includes(s.harness) && s.surface === 'cli' && !s.hosted && !s.visitor;
  // A session a usage limit stopped is told to go on where it moves (actions.rs), so it says so.
  const goesOn = s.limit && s.status !== 'working' ? ' and go on' : '';
  const suggested = s.limit?.moveTo && moveTo.includes(s.limit.moveTo) && goesOn ? s.limit.moveTo : null;
  const archive = can.archive ?? ((archived: boolean) => (archived ? settle('archived')(archiveAhead(s.key, true)) : void archiveAhead(s.key, false).catch((e: unknown) => report.error(errorText(e)))));
  // Work under way holds it, and Mark as done again soon after ends that too. A session that's
  // working is held here; one that left a command running in the background, by the engine.
  const holds = (why: string) => {
    holding.set(s.key, Date.now());
    report.info(`${why} ${DONE_AGAIN}`);
  };
  const done = () => {
    const stop = held(s.key, Date.now());
    if (!stop && s.status === 'working') return holds(`“${s.title}” is still working.`);
    can.left?.('done');
    busy(`Wrapping up ${s.title}…`, doneAhead(s.key, stop))
      .then((r) => {
        if (!r.done) holds(r.message);
        else if (can.undoDone) can.undoDone(r.message);
        else if (r.noted) report.info(r.message);
      })
      .catch((e: unknown) => report.error(errorText(e)));
  };
  return {
    open: { label: openLabel(lane, s, can.terminal), run: () => can.open(lane, s.openTarget) },
    resume: resumeTo.length > 1 ? { label: cutOff ? 'Carry on in' : 'Resume in', items: resumeTo.map((to) => ({ label: to.label, run: () => void resume(to) })) } : undefined,
    terminal: opensInEditor(lane) ? { label: `Open folder in ${can.terminal}`, run: () => can.open(lane, terminalTarget(s.key)) } : undefined,
    folder: can.editors && opensInEditor(lane) ? item(folderMenu(can.editors.apps, can.editors.icons, (id) => can.open(lane, id), { name: can.terminal, key: s.key, accelerator: can.keys ? CHORDS.terminal : undefined })) : undefined,
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
    moveFolder: movesFolders ? folderMoves(lane, can.worktrees ?? [], (to) => void moveFolder(to)) : undefined,
    moveOn: suggested ? moveItem(suggested, `Move to ${suggested} and go on`) : undefined,
    moveAccounts: moveTo.length > 0 ? { to: moveTo, suggested, goesOn: !!goesOn, run: move } : undefined,
    carryOn: carryTo ? { label: 'Carry on', run: () => void resume(carryTo) } : undefined,
    // The session carries on, waiting for you.
    stop: stoppable(s) ? { label: 'Stop turn', run: () => void said(`Stopping ${s.title}…`, sessionCancel(s.key)) } : undefined,
    rename: can.rename && renames(s) ? { label: 'Rename…', run: can.rename } : undefined,
    seen: seenable(s) ? { label: 'Mark as seen', run: () => settle('marked seen')(seenAhead([s.key])) } : undefined,
    // It joins what waits on you rather than leaving it, so a window working through that stays.
    unread: unreadable(s) ? { label: 'Mark as unread', run: () => void unreadAhead([s.key]).catch((e: unknown) => report.error(errorText(e))) } : undefined,
    pin: s.archived ? undefined : can.pin,
    desktop: !s.archived && can.desktop ? desktopItem('session', s.key, can.desktop, report) : undefined,
    // On the Mac it runs on, which keeps it, so it's snoozed on every device.
    snooze: s.archived ? undefined : snoozeMenu([s.key], Date.now(), { until: s.snoozedUntil, onDone: settle('snoozed'), chords: !!can.keys }),
    // Kept on the Mac it runs on too. Quieted, a turn that ended leaves what waits on you, and a
    // window working through that goes on; one that needs you stays.
    quiet: s.archived
      ? undefined
      : {
          label: 'Quiet',
          checked: !!s.quiet,
          run: () => {
            const act = quietAhead([s.key], !s.quiet);
            if (!s.quiet && s.status === 'unread') settle('quieted')(act);
            else act.catch((e: unknown) => report.error(errorText(e)));
          },
        },
    // Kept on the Mac it runs on too, for good: its agent can still read it and send to it.
    own: s.helper && !s.archived ? { label: 'Make it my own', run: () => void said(`Making ${s.title} your own…`, ownSessions([s.key]).then(() => `“${s.title}” is a session of your own now.`)) } : undefined,
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
    // On any Mac: one that won't close it (it has remote control off, or an older Codealong)
    // still marks it seen and archives it (actions.rs `done_anyway`). An archived one is done
    // with already.
    done: !s.archived ? { label: 'Mark as done', run: done } : undefined,
    archive: { label: s.archived ? 'Unarchive' : s.harness === 'codex' ? 'Archive in Codex' : 'Archive', run: () => archive(!s.archived) },
    copyId: copyItem('Copy session ID', s.id, 'session ID', report) as MenuAction,
    copyResume: copyItem('Copy resume command', s.resumeCommand, 'resume command', report),
  };
}

/** What Mark as done does to `s`, for the tooltip of a button that runs it (idle.rs `ending`). A
 *  session holding a worktree its harness made for it exits its harness's way, which removes the
 *  worktree or asks you about it; while that can't be, its tab closes and the worktree stays. */
export function doneTitle(s: Session): string {
  const plain = 'Mark as done: it closes where Codealong can close it, and is marked seen and archived';
  if (!s.ownWorktree) return plain;
  const harness = HARNESS_WORD[s.harness];
  const worktree = s.ownWorktree.split('/').filter(Boolean).pop() ?? s.ownWorktree;
  if (exitsItsOwnWay(s)) {
    return `Mark as done: ${harness} exits it, removing the worktree it made for it (${worktree}), or asking in its terminal whether to keep it if anything there would be lost. It’s marked seen and archived`;
  }
  // A background job stops, and Codealong removes its worktree after (the engine's cleanup.rs).
  if (s.attachCommand != null && s.live) {
    return `Mark as done: its background job stops, and the worktree ${harness} made for it (${worktree}) goes too unless something there would be lost: uncommitted files, or commits on no remote. It’s marked seen and archived`;
  }
  return `${plain}. While it’s busy or waiting on you, its terminal just closes and the worktree ${harness} made for it (${worktree}) stays`;
}

/** How long after Mark as done was held by work under way another ends that too. */
export const DONE_HOLD_MS = 15_000;
const DONE_AGAIN = 'Mark it done again to end that too.';
const DONE_AGAIN_MANY = 'Mark them done again to end that too.';
/** Sessions Mark as done was held on, by key, with when: each window's own. */
const holding = new Map<string, number>();
/** Sessions moved from this window, by key, from the click on, unless the move fails or the
 *  session goes on as itself: one moved out goes on as its copy, and a second move would make a
 *  second copy beside it (start.rs `move_to`). Each window's own. */
const movedHere = new Set<string>();

/** Whether Mark as done on `key` now ends work under way too: it was held on it a moment ago.
 *  Asking uses the hold up. */
function held(key: string, now: number): boolean {
  const at = holding.get(key);
  holding.delete(key);
  return at !== undefined && now - at <= DONE_HOLD_MS;
}

/** Where Carry on resumes a session, as its box would with no place picked (start.ts
 *  `resumeChoice`): where its account's sessions last went on from here, else a terminal, else
 *  the first place there is. */
export function carryOnIn(places: StartAccount['places'], last: StartPlace | undefined): StartAccount['places'][number] {
  return places.find((p) => p.place === last) ?? places.find((p) => p.place === 'terminal') ?? places[0];
}

/** Where Carry on resumes `s`, for one of this Mac's whose turn was cut off and that isn't
 *  running; undefined for any other. Resumed with nothing to say, it's told to carry on (start.rs
 *  `resume`): one of its harness's app goes on there, the rest where its box would resume it. */
function carriesOnIn(s: Session): StartAccount['places'][number] | undefined {
  if (s.cutOff == null || s.status === 'working' || shortOf(s.key) || s.live || s.hosted) return undefined;
  if (s.surface === 'app') return resumePlaces(s).find((p) => p.place === 'claude-app' || p.place === 'codex-app');
  const places = s.surface === 'cli' ? resumePlaces(s) : [];
  return places.length > 0 ? carryOnIn(places, resumedIn(s.account)) : undefined;
}

/** Where a session of this Mac's that isn't running can go on: its account's places for that. */
function resumePlaces(s: Pick<Session, 'account' | 'harness'>): StartAccount['places'] {
  const a = accountOf(startAccountsNow(), s);
  return a ? resumeAccount(a).places : [];
}

/** Move to worktree: this Mac's worktrees other than `from`, a submenu for each project (the
 *  session's own first, then in the board's order), each repo's main checkout first and then the
 *  latest, and last a chat of its own, unless it's a chat already. Undefined when there's
 *  nowhere to go. */
export function folderMoves(from: Lane, worktrees: Lane[], run: (to: Lane) => void): MenuGroup | undefined {
  const mine = worktrees.filter((l) => !l.device && l.id !== from.id);
  const projects = [...new Set(mine.filter((l) => l.project !== 'Elsewhere').map((l) => l.project))].sort((a, b) => Number(b === from.project) - Number(a === from.project));
  const entry = (l: Lane): MenuAction => ({ label: l.origin === 'main' ? `${l.callsign} (main checkout)` : l.callsign, run: () => run(l) });
  const projectMenu = (project: string): MenuGroup => {
    const lanes = mine.filter((l) => l.project === project);
    const repos = [...new Set(lanes.map((l) => l.repo))];
    const byRepo = repos.map((repo): MenuSpec => {
      const list = lanes.filter((l) => l.repo === repo).sort((a, b) => Number(b.origin === 'main') - Number(a.origin === 'main') || b.lastActivity - a.lastActivity);
      // A heading says which repo, where the project has more than one.
      return [...(repos.length > 1 ? [{ label: repo, enabled: false, run: () => {} }] : []), ...list.map(entry)];
    });
    return { label: project, items: sections(...byRepo) };
  };
  const chat = from.startsChats ? undefined : mine.find((l) => l.startsChats);
  const items = sections(projects.map(projectMenu), chat ? [{ label: 'A chat of its own', run: () => run(chat) }] : []);
  return items.length > 0 ? { label: 'Move to worktree', items } : undefined;
}

/** The order of a session's menu, in groups with a line between: opening it (where it lives,
 *  its folder, its PR, its tabs in Chrome), showing or answering it in Codealong and starting from it; the window's
 *  todos; what it is to you (its turn, its name, seen, pinned, snoozed, quiet, closed, archived);
 *  copying it. */
const SESSION_MENU = [
  ['heading'],
  ['open', 'resume', 'folder', 'pr', 'chrome', 'reveal', 'reply', 'newSession', 'fork', 'move', 'moveFolder'],
  ['todos'],
  ['stop', 'rename', 'seen', 'unread', 'pin', 'desktop', 'snooze', 'quiet', 'own', 'close', 'done', 'archive'],
  ['copyId', 'copyResume', 'canvas'],
] as const;

/** A session's menu, as every window has it. */
export function sessionMenu(lane: Lane, s: Session, can: SessionCan): MenuSpec {
  const extras = {
    heading: can.heading ? [{ label: `${s.title} · ${lane.callsign}`, enabled: false, run: () => {} }] : [],
    todos: can.todos ?? [],
    canvas: s.visitor ? [] : (can.canvas ?? []),
  };
  return laidOut(SESSION_MENU, sessionActions(lane, s, can), extras, can.keys);
}

/** What a window can do to several sessions picked together (the main window's Sessions), as
 *  `SessionCan` says it for one. */
export interface SessionsCan {
  report: Report;
  /** Pins them to this window's own list, or unpins them. */
  pin?: MenuAction;
  /** Archives these or takes them out of the archive, where that does more (an undo). */
  archive?: (keys: string[], archived: boolean) => void;
  /** Says what Mark as done did with a way back, where the window has one: the ones done. */
  undoDone?: (keys: string[], message: string) => void;
  /** These left the list they were picked from, or what waits on you, and how. */
  left?: (done: Left, keys: string[]) => void;
  keys?: boolean | 'chords';
}

/** What can be done to several sessions at once, by the names `SessionActions` gives them, so
 *  the same keys run them (`keyed`). Each acts on the ones it applies to by `sessionActions`'s
 *  rules for one, says how many in its label, and is missing when it applies to none. */
export interface SessionsActions {
  /** Each whose turn was cut off goes on where Carry on would take it alone, one after another. */
  carryOn?: MenuAction;
  stop?: MenuAction;
  seen?: MenuAction;
  unread?: MenuAction;
  pin?: MenuAction;
  snooze?: MenuGroup;
  /** Checked while every one of them is quiet, which it makes heard again; else it quiets them. */
  quiet?: MenuAction;
  close?: MenuAction;
  done?: MenuAction;
  archive?: MenuAction;
  copyId?: MenuAction;
  /** How many of them each takes. */
  takes: Record<'carryOn' | 'stop' | 'seen' | 'unread' | 'snooze' | 'close' | 'done' | 'archive', number>;
}

/** Their titles, as a sentence lists them. */
function titles(list: Session[]): string {
  const said = list.map((s) => `“${s.title}”`);
  return said.length > 1 ? `${said.slice(0, -1).join(', ')} and ${said[said.length - 1]}` : (said[0] ?? '');
}

/** Does `work` to each in turn (with where it is in `list`), each once the one before is done:
 *  closing a terminal or typing into one goes through the terminal app, which takes one at a
 *  time. Says how each went. */
async function oneByOne<T>(list: Session[], work: (s: Session, i: number) => Promise<T>): Promise<{ s: Session; ok?: T; error?: string }[]> {
  const out: { s: Session; ok?: T; error?: string }[] = [];
  for (const [i, s] of list.entries()) {
    try {
      out.push({ s, ok: await work(s, i) });
    } catch (e) {
      out.push({ s, error: errorText(e) });
    }
  }
  return out;
}

/** What went wrong with some of them, after what went right: `Closed 2 sessions. Couldn't close
 *  “Fix login”: why.` */
function howItWent(right: string, verb: string, out: { s: Session; error?: string }[]): { words: string; wrong: boolean } {
  const wrong = out.filter((o) => o.error).map((o) => `Couldn’t ${verb} “${o.s.title}”: ${(o.error ?? '').replace(/\.$/, '')}.`);
  return { words: [right, ...wrong].filter(Boolean).join(' '), wrong: wrong.length > 0 };
}

export function sessionsActions(picked: { lane: Lane; session: Session }[], can: SessionsCan): SessionsActions {
  const { report } = can;
  const busy = <T>(label: string, work: Promise<T>) => (report.busy ? report.busy(label, work) : work);
  // Each one's own actions say whether one applies to it, so the rules stay `sessionActions`'s.
  const own = picked.map(({ lane, session }) => ({ s: session, a: sessionActions(lane, session, { report, open: () => {}, terminal: '' }) }));
  const those = (name: keyof SessionActions) => own.filter((x) => x.a[name]).map((x) => x.s);
  const keysOf = (list: Session[]) => list.map((s) => s.key);
  const all = own.map((x) => x.s);
  const [stop, seen, unread, snooze, quiet, close, done] = (['stop', 'seen', 'unread', 'snooze', 'quiet', 'close', 'done'] as const).map(those);
  const allQuiet = quiet.length > 0 && quiet.every((s) => s.quiet);
  const carryOn = all.flatMap((s) => {
    const to = carriesOnIn(s);
    return to ? [{ s, to }] : [];
  });
  const toArchive = all.filter((s) => !s.archived);
  const archive = can.archive ?? ((keys: string[], archived: boolean) => void Promise.all(keys.map((k) => archiveAhead(k, archived))).catch((e: unknown) => report.error(errorText(e))));
  const say = (said: { words: string; wrong: boolean }) => said.words && (said.wrong ? report.error(said.words) : report.info(said.words));
  // Work under way holds those it's in, as it holds one: the rest are done, and Mark as done
  // again soon after ends the work too.
  const doneAll = () => {
    const now = Date.now();
    const going = done.map((s) => ({ s, stop: held(s.key, now) }));
    const working = going.filter((g) => !g.stop && g.s.status === 'working').map((g) => g.s);
    for (const s of working) holding.set(s.key, now);
    const asked = going.filter((g) => !working.includes(g.s));
    const still = working.length > 0 ? `${titles(working)} ${working.length === 1 ? 'is' : 'are'} still working.` : '';
    if (asked.length === 0) return report.info(`${still} ${working.length === 1 ? DONE_AGAIN : DONE_AGAIN_MANY}`);
    can.left?.('done', asked.map((g) => g.s.key));
    // They all show as done at once, and the engine ends them one after another.
    let before: Promise<unknown> | undefined;
    const answers = asked.map(({ s, stop }) => {
      const answer = doneAhead(s.key, stop, before);
      before = answer;
      return answer.then(
        (ok) => ({ s, ok }),
        (e: unknown) => ({ s, ok: undefined, error: errorText(e) }),
      );
    });
    void busy(`Wrapping up ${count(asked.length, 'session')}…`, Promise.all(answers)).then((out) => {
      const finished = out.filter((o) => o.ok?.done);
      const heldBack = out.filter((o) => o.ok && !o.ok.done);
      for (const o of heldBack) holding.set(o.s.key, Date.now());
      const waits = working.length + heldBack.length;
      const words = [
        finished.length > 0 ? `Done with ${count(finished.length, 'session')}.` : '',
        ...finished.filter((o) => o.ok?.noted).map((o) => o.ok?.message ?? ''),
        still,
        ...heldBack.map((o) => o.ok?.message ?? ''),
        waits > 0 ? (waits === 1 ? DONE_AGAIN : DONE_AGAIN_MANY) : '',
      ];
      const said = howItWent(words.filter(Boolean).join(' '), 'end', out);
      if (finished.length > 0 && can.undoDone) can.undoDone(keysOf(finished.map((o) => o.s)), said.words);
      else say(said);
    });
  };
  // Each opens where it goes on, as Close closes a terminal, so they go one after another.
  const carryOnAll = () =>
    void busy(
      `Carrying on ${count(carryOn.length, 'session')}…`,
      oneByOne(
        carryOn.map((c) => c.s),
        (s, i) =>
          resumeSession(s.key, { place: carryOn[i].to.place, model: null, effort: null }).then((sent) => {
            rememberResumed(s.account, carryOn[i].to.place);
            return sent;
          }),
      ),
    ).then((out) => {
      // One its app only opened, with the prompt to paste there (claude/app.rs `InApp`), isn't
      // going on yet: its own words say what's left to do.
      const went = out.filter((o) => o.ok && !o.ok.opened).length;
      const opened = out.flatMap((o) => (o.ok?.opened ? [o.ok.message] : []));
      say(howItWent([went > 0 ? `Resumed ${count(went, 'session')} to carry on.` : '', ...opened].filter(Boolean).join(' '), 'carry on', out));
    });
  return {
    carryOn: carryOn.length > 0 ? { label: `Carry on ${count(carryOn.length, 'session')}`, run: carryOnAll } : undefined,
    stop:
      stop.length > 0
        ? { label: `Stop ${count(stop.length, 'turn')}`, run: () => void busy(`Stopping ${count(stop.length, 'turn')}…`, oneByOne(stop, (s) => sessionCancel(s.key))).then((out) => say(howItWent('', 'stop', out))) }
        : undefined,
    seen:
      seen.length > 0
        ? {
            label: `Mark ${count(seen.length, 'session')} as seen`,
            run: () => {
              can.left?.('marked seen', keysOf(seen));
              seenAhead(keysOf(seen)).catch((e: unknown) => report.error(errorText(e)));
            },
          }
        : undefined,
    unread: unread.length > 0 ? { label: `Mark ${count(unread.length, 'session')} as unread`, run: () => void unreadAhead(keysOf(unread)).catch((e: unknown) => report.error(errorText(e))) } : undefined,
    pin: toArchive.length > 0 ? can.pin : undefined,
    // Wake now wakes them all, where any is snoozed.
    snooze:
      snooze.length > 0
        ? snoozeMenu(keysOf(snooze), Date.now(), {
            label: `Snooze ${count(snooze.length, 'session')}`,
            until: Math.max(0, ...snooze.map((s) => s.snoozedUntil ?? 0)) || null,
            onDone: (act) => {
              can.left?.('snoozed', keysOf(snooze));
              act.catch((e: unknown) => report.error(errorText(e)));
            },
            chords: !!can.keys,
          })
        : undefined,
    quiet:
      quiet.length > 0
        ? {
            label: `Quiet ${count(quiet.length, 'session')}`,
            checked: allQuiet,
            run: () => {
              const leaving = allQuiet ? [] : quiet.filter((s) => !s.quiet && s.status === 'unread');
              if (leaving.length > 0) can.left?.('quieted', keysOf(leaving));
              quietAhead(keysOf(quiet), !allQuiet).catch((e: unknown) => report.error(errorText(e)));
            },
          }
        : undefined,
    close:
      close.length > 0
        ? {
            label: `Close ${count(close.length, 'session')}`,
            run: () => {
              can.left?.('closed', keysOf(close));
              void busy(`Closing ${count(close.length, 'session')}…`, oneByOne(close, (s) => closeSession(s.key))).then((out) => {
                const closed = out.filter((o) => !o.error).length;
                say(howItWent(closed > 0 ? `Closed ${count(closed, 'session')}.` : '', 'close', out));
              });
            },
          }
        : undefined,
    done: done.length > 0 ? { label: `Mark ${count(done.length, 'session')} as done`, run: doneAll } : undefined,
    // Any not archived yet are archived; when all are, they come out of it.
    archive:
      toArchive.length > 0
        ? {
            label: `Archive ${count(toArchive.length, 'session')}`,
            run: () => {
              can.left?.('archived', keysOf(toArchive));
              archive(keysOf(toArchive), true);
            },
          }
        : all.length > 0
          ? { label: `Unarchive ${count(all.length, 'session')}`, run: () => archive(keysOf(all), false) }
          : undefined,
    copyId: copyItem('Copy session IDs', all.map((s) => s.id).join(' '), 'session IDs', report),
    takes: { carryOn: carryOn.length, stop: stop.length, seen: seen.length, unread: unread.length, snooze: snooze.length, close: close.length, done: done.length, archive: toArchive.length || all.length },
  };
}

/** The order of the menu of several sessions picked together: carrying on the ones cut off, what
 *  they are to you, then copying them. Opening them, and what's done to one at a time (rename, move, fork), aren't. */
const SESSIONS_MENU = [
  ['carryOn'],
  ['stop', 'seen', 'unread', 'pin', 'snooze', 'quiet', 'close', 'done', 'archive'],
  ['copyId'],
] as const;

/** The menu of several sessions picked together. */
export function sessionsMenu(picked: { lane: Lane; session: Session }[], can: SessionsCan): MenuSpec {
  return laidOut(SESSIONS_MENU, sessionsActions(picked, can), {}, can.keys);
}

/** What a window can do to several worktrees picked together (the main window's Worktrees). */
export interface LanesCan {
  report: Report;
  /** Pins them to this window's own list, or unpins them. */
  pin?: MenuAction;
  /** Asks to remove the ones that can go, where the window has a dialog for it. */
  cleanUp?: (lanes: Lane[]) => void;
  keys?: boolean | 'chords';
}

/** What can be done to several worktrees at once, by the names a worktree's or a session's keys
 *  go by (`keyed`: E marks seen, P pins): marking their sessions seen, as E does on a worktree,
 *  and what a worktree's menu does that makes sense for several. */
export interface LanesActions {
  seen?: MenuAction;
  reuse?: MenuAction;
  cleanUp?: MenuAction;
  pin?: MenuAction;
  copyPath?: MenuAction;
  copyBranch?: MenuAction;
  /** How many of their sessions Mark as seen takes. */
  takes: Record<'seen', number>;
}

export function lanesActions(lanes: Lane[], can: LanesCan): LanesActions {
  const { report } = can;
  const seen = lanes.flatMap((l) => l.sessions.filter((s) => !s.visitor && seenable(s)).map((s) => s.key));
  // Only the worktrees that can be reused take it, as one's menu offers it.
  const reusable = lanes.filter((l) => reuseItem(l, report));
  const reused = reusable.length > 0 && reusable.every((l) => l.reuse != null);
  const setReuse = () => void Promise.all(reusable.map((l) => setReusable(l.id, !reused))).catch((e: unknown) => report.error(errorText(e)));
  const lines = (list: (string | null | undefined)[]) => list.filter((x): x is string => !!x).join('\n');
  return {
    seen: seen.length > 0 ? { label: `Mark ${count(seen.length, 'session')} as seen`, run: () => void seenAhead(seen).catch((e: unknown) => report.error(errorText(e))) } : undefined,
    reuse: reusable.length > 0 ? { label: 'Reuse for new work', checked: reused, run: setReuse } : undefined,
    cleanUp: cleanUpItem(lanes, can.cleanUp),
    pin: can.pin,
    copyPath: copyItem(lanes.length === 1 ? 'Copy path' : 'Copy paths', lines(lanes.map((l) => l.path)), 'worktree paths', report),
    copyBranch: copyItem(lanes.length === 1 ? 'Copy branch name' : 'Copy branch names', lines(lanes.map((l) => l.branch)), 'branch names', report),
    takes: { seen: seen.length },
  };
}

/** The order of the menu of several worktrees picked together, as one worktree's menu has them. */
const LANES_MENU = [['seen'], ['reuse', 'cleanUp'], ['pin'], ['copyPath', 'copyBranch']] as const;

/** The menu of several worktrees picked together. */
export function lanesMenu(lanes: Lane[], can: LanesCan): MenuSpec {
  return laidOut(LANES_MENU, lanesActions(lanes, can), {}, can.keys);
}

/** What a window can do to several PRs picked together (the main window's PRs). */
export interface PrsCan {
  report: Report;
  /** Approves these, each as you, one after another: the ones you may (prStatus.ts `mayApprove`). */
  approve?: (prs: PrItem[]) => void;
  /** Pins them to this window's own list, or unpins them. */
  pin?: MenuAction;
  keys?: boolean | 'chords';
}

/** What can be done to several PRs at once, by the names a row's keys go by (`keyed`: P pins
 *  them), each saying how many where it takes only some. */
export interface PrsActions {
  github?: MenuAction;
  approve?: MenuAction;
  snooze?: MenuGroup;
  pin?: MenuAction;
  copyLink?: MenuAction;
  copyBranch?: MenuAction;
  /** How many of them each takes. */
  takes: Record<'approve' | 'snooze', number>;
}

/** Whether you're asked to review it and it's still open: what Snooze review takes. */
function asksYourReview(p: PrItem): boolean {
  return p.reviewRequested && p.state !== 'merged' && p.state !== 'closed';
}

export function prsActions(prs: PrItem[], can: PrsCan): PrsActions {
  const { report } = can;
  const asked = prs.filter(asksYourReview);
  const approvable = prs.filter(mayApprove);
  const n = (list: unknown[]) => count(list.length, 'PR');
  // One after another, so they open in the list's order.
  const openAll = () => void prs.reduce<Promise<unknown>>((before, p) => before.then(() => openUrl(p.url)), Promise.resolve()).catch((e: unknown) => report.error(errorText(e)));
  const lines = (list: (string | null | undefined)[]) => list.filter((x): x is string => !!x).join('\n');
  return {
    github: prs.length > 0 ? { label: `Open ${n(prs)} on GitHub`, run: openAll } : undefined,
    approve: can.approve && approvable.length > 0 ? { label: `Approve ${n(approvable)}`, run: () => can.approve?.(approvable) } : undefined,
    snooze: asked.length > 0 ? reviewsSnoozeMenu(asked, Date.now(), (act) => void act.catch((e: unknown) => report.error(errorText(e))), !!can.keys) : undefined,
    pin: can.pin,
    copyLink: copyItem(prs.length === 1 ? 'Copy link' : 'Copy links', lines(prs.map((p) => p.url)), 'PR links', report),
    copyBranch: copyItem(prs.length === 1 ? 'Copy branch name' : 'Copy branch names', lines(prs.map((p) => p.head)), 'branch names', report),
    takes: { approve: approvable.length, snooze: asked.length },
  };
}

/** The order of the menu of several PRs picked together, as one PR's menu has them. */
const PRS_MENU = [['github'], ['approve', 'snooze'], ['pin'], ['copyLink', 'copyBranch']] as const;

/** The menu of several PRs picked together. */
export function prsMenu(prs: PrItem[], can: PrsCan): MenuSpec {
  return laidOut(PRS_MENU, prsActions(prs, can), {}, can.keys);
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
  /** Settings › Automation with the worktree's repo picked, where Settings is. */
  automation?: (repo: string) => void;
  /** Add to todo and Remove from todo, where the todos are. */
  todos?: MenuSpec;
  /** Asks to remove it, where the window has a dialog for it (the main window). */
  cleanUp?: (lanes: Lane[]) => void;
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
  /** A new terminal in it, on this Mac (⌃`). */
  terminal?: MenuAction;
  editor?: MenuAction;
  openIn?: MenuGroup;
  finder?: MenuAction;
  preview?: MenuAction;
  pr?: MenuAction;
  reveal?: MenuAction;
  newSession?: MenuAction;
  setPreview?: MenuAction;
  automation?: MenuAction;
  reuse?: MenuAction;
  cleanUp?: MenuAction;
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
  // A new terminal here, after the default open, or as it where it's the default (no sessions).
  const terminal = lane.device ? undefined : target('l|terminal');
  return {
    open: main && !(terminal && main === 'l|terminal') ? { label: lane.targets.find((t) => t.id === main)?.label ?? 'Open', run: () => can.open(lane, main) } : undefined,
    terminal,
    // Open in <yours>, then the other editors installed here.
    editor: other('l|editor'),
    openIn: can.editors && opensInEditor(lane) ? (item(openInMenu(can.editors.apps, can.editors.icons, (id) => can.open(lane, id))) as MenuGroup | undefined) : undefined,
    finder: other('l|finder'),
    preview: other('l|preview'),
    pr: prItem(lane, report),
    reveal: can.reveal && { label: 'Show in Codealong', run: can.reveal },
    newSession: newSessionItem(lane, can.newSession),
    setPreview: can.setPreview && !lane.device ? { label: lane.preview?.set ? 'Change preview URL…' : 'Set preview URL…', run: can.setPreview } : undefined,
    automation: can.automation && lane.repoSlug && !lane.device ? { label: `Automation for ${lane.repoSlug}…`, run: () => can.automation?.(lane.repoSlug ?? '') } : undefined,
    reuse: reuseItem(lane, report),
    cleanUp: cleanUpItem([lane], can.cleanUp),
    pin: can.pin,
    desktop: can.desktop ? desktopItem('worktree', lane.id, can.desktop, report) : undefined,
    copyPath: copyItem('Copy path', lane.path, 'worktree path', report),
    copyBranch: copyItem('Copy branch name', lane.branch, 'branch name', report),
    copyPr: copyItem('Copy PR link', lane.pr?.url, 'PR link', report),
    copyPreview: copyItem('Copy preview URL', lane.preview?.url, 'preview URL', report),
  };
}

/** The order of a worktree's menu: opening it, its PR, showing it and starting in it; the
 *  window's todos; whether it's reused, and cleaning it up; pins; copying its path, branch and
 *  links. */
const LANE_MENU = [
  ['open', 'terminal', 'editor', 'openIn', 'finder', 'preview', 'pr', 'reveal', 'newSession', 'setPreview', 'automation'],
  ['todos'],
  ['reuse', 'cleanUp'],
  ['pin', 'desktop'],
  ['copyPath', 'copyBranch', 'copyPr', 'copyPreview', 'canvas'],
] as const;

/** A worktree's menu, as every window has it. */
export function laneMenu(lane: Lane, can: LaneCan): MenuSpec {
  return laidOut(LANE_MENU, laneActions(lane, can), { todos: can.todos ?? [], canvas: can.canvas ?? [] }, can.keys);
}

/** ⌃` opens a new terminal in the folder of what a window shows, as it opens VS Code's terminal:
 *  the same chord in every view, from a text field too. */
const TERMINAL_CHORD = 'Ctrl+`';
/** The main window's keys for actions, shown beside them in its menus: a letter, or the terminal's
 *  chord, which is the same everywhere. */
export const KEYS: Partial<Record<keyof SessionActions | keyof LaneActions, string>> = { seen: 'E', unread: 'U', pin: 'P', archive: 'A', rename: 'R', terminal: TERMINAL_CHORD };
/** The same actions' chords, which work in every view and from Sessions' box, where letters alone
 *  do nothing: ⌘E marks seen (and goes on, where a window works through what waits on you: it's
 *  the queue's key, queue.ts), ⇧⌘E marks done, beside it, and ⌃⌘ and the letter runs the rest.
 *  Closing a session's terminal and marking it done, which closes it too, have a chord and no
 *  letter of their own: they're not ones to press by mistake. */
export const CHORDS: typeof KEYS = { seen: QUEUE_CHORDS.seen[0], done: 'Shift+Cmd+E', unread: 'Ctrl+Cmd+U', pin: 'Ctrl+Cmd+P', archive: 'Ctrl+Cmd+A', rename: 'Ctrl+Cmd+R', close: 'Ctrl+Cmd+W', terminal: TERMINAL_CHORD };
/** The keys that send the replies suggested for a session (SuggestedReplies.tsx), the first to
 *  the sixth, as the quick panel's ⌘1 to ⌘9 do its rows: from the session's box or the list of
 *  them, and from anywhere in a window whose keyboard works through what waits on you (the notch,
 *  the waiting window). Each reply shows its key while it works, and its menu too. From the main
 *  window's list, ⌘ and a digit still open the selected row's places (mainKeys.ts); a box takes
 *  no ⌘ and a digit of its own. As many as `codealong session suggest` takes (suggestions.rs
 *  `MOST`). */
export const SUGGESTION_CHORDS = ['Cmd+1', 'Cmd+2', 'Cmd+3', 'Cmd+4', 'Cmd+5', 'Cmd+6'] as const;

/** Which suggested reply a key sends (its place in the list), or none for any other key. */
export function suggestionChord(e: Key): number | null {
  const index = SUGGESTION_CHORDS.findIndex((chord) => chordPressed(chord, e));
  return index >= 0 ? index : null;
}

// Snoozing has a chord for each of its times, ⌃⌘1 to ⌃⌘4 (snooze.ts `SNOOZE_CHORDS`), which its
// menu shows beside them. The keys for getting from one session to the next are queue.ts
// `QUEUE_CHORDS`, written the same way.
const CTRL_CMD = 'Ctrl+Cmd+';

declare global {
  interface Window {
    /** The chords, in a preview (scripts/preview.sh), whose checks hold the menus to them. */
    __CODEALONG_CHORDS__?: { session: typeof CHORDS; snooze: readonly string[] };
  }
}
// A browser check reads them here rather than keeping a copy, which goes stale as a key is added.
if (inPreview) window.__CODEALONG_CHORDS__ = { session: CHORDS, snooze: SNOOZE_CHORDS };

/** Whether ⌃⌘ with this key is one of the chords: an action's letter, or a snooze's digit. */
export function chordLetter(key: string): boolean {
  return Object.values(CHORDS).includes(CTRL_CMD + key.toUpperCase()) || snoozeChordIndex(key) >= 0;
}

/** The time ⌃⌘ with a digit snoozes a session, or your review of a PR, to (snooze.ts
 *  `SNOOZE_CHORDS`), as its Snooze menu has it now; none for one that can't be snoozed, or a key
 *  that isn't one of them. */
function snoozeChorded(actions: SessionActions | SessionsActions | LaneActions | LanesActions | PrActions | PrsActions, key: string): MenuAction | null {
  const snooze = 'snooze' in actions ? actions.snooze : undefined;
  const it = snooze?.items[snoozeChordIndex(key)];
  return it && it !== 'separator' && !('items' in it) ? it : null;
}

/** A chord as the keys' own signs, for a button that says it: `Ctrl+Cmd+W` is ⌃⌘W, and
 *  `Alt+Shift+Down` ⌥⇧↓ (on Linux in its keys' names, platform.ts). */
export function chordSigns(chord: string): string {
  return platformSigns(chord);
}

/** A chord's signs one key at a time, as a list of places to open says its keys (`Target.kbd`). */
export function chordKeys(chord: string): string[] {
  return platformKeys(chord);
}

/** The action a key runs among a session's or worktree's (or several sessions'), by the key as
 *  typed, when it applies to them now: the letter alone (`KEYS`), or with `chord` the letter
 *  pressed with ⌃⌘ (`CHORDS`). */
export function keyed(actions: SessionActions | SessionsActions | LaneActions | LanesActions | PrsActions, key: string, chord = false): { id: string; action: MenuAction } | null {
  const snooze = chord ? snoozeChorded(actions, key) : null;
  if (snooze) return { id: 'snooze', action: snooze };
  const letter = key.toUpperCase();
  const id = chord ? Object.keys(CHORDS).find((name) => CHORDS[name as keyof typeof CHORDS] === CTRL_CMD + letter) : Object.keys(KEYS).find((name) => KEYS[name as keyof typeof KEYS] === letter);
  const it = id ? (actions as Record<string, Item | undefined>)[id] : undefined;
  return id && it && !('items' in it) ? { id, action: it } : null;
}

/** The action a chord runs among a session's (`CHORDS`), when it applies to the session now:
 *  for the windows that show one session with a box to answer it, where letters are typed. */
export function chorded(actions: SessionActions, e: Pick<KeyboardEvent, 'key' | 'code' | 'metaKey' | 'ctrlKey' | 'altKey' | 'shiftKey'>): MenuAction | null {
  const snooze = modsHeld(e, 'Ctrl', 'Cmd') ? snoozeChorded(actions, e.key) : null;
  if (snooze) return snooze;
  const id = Object.keys(CHORDS).find((name) => chordPressed(CHORDS[name as keyof typeof CHORDS] ?? '', e));
  const it = id ? (actions as unknown as Record<string, Item | undefined>)[id] : undefined;
  return it && !('items' in it) ? it : null;
}

type Item = MenuAction | MenuGroup;

/** What a window that shows a PR waiting on your review as a card (PrCard.tsx) can do to it, by
 *  name, each once: the strip over its header shows them as buttons that say their keys, the
 *  chords run them (`PR_CHORDS`, `prChorded`), and its menu lays them out. One is missing when the
 *  window can't do it or the PR doesn't take it (approving a PR that doesn't ask for your review). */
export interface PrActions {
  /** Approves it, with what's in the box as the review's text. */
  approve?: MenuAction;
  /** Comments on it with what's in the box, which must say something. */
  comment?: MenuAction;
  /** Asks for changes, with what's in the box, which must say something. */
  changes?: MenuAction;
  /** Review with…'s choices, the Auto one first where there's a choice (review.ts). */
  review?: MenuGroup;
  /** Snooze review's times (snooze.ts `reviewSnoozeMenu`). */
  snooze?: MenuGroup;
  github: MenuAction;
}

/** A PR card's chords, written as `CHORDS` are. ⌘⏎ approves and goes on, as it sends a session's
 *  message and goes on; ⌥⌘⏎ comments; ⌃⌘R starts a review (Rename on a session, which a PR has
 *  no name for); ⌃⌘G opens it on GitHub. Asking for changes has no chord: it's not one to press
 *  by mistake. Snoozing has ⌃⌘1 to ⌃⌘4, as a session's does. */
export const PR_CHORDS: Partial<Record<keyof PrActions, string>> = { approve: 'Cmd+Enter', comment: 'Alt+Cmd+Enter', review: 'Ctrl+Cmd+R', github: 'Ctrl+Cmd+G' };

/** The action a chord runs among a PR card's (`PR_CHORDS`, and the snooze digits), when it
 *  applies to the PR now. Review with runs its first choice. */
export function prChorded(actions: PrActions, e: Pick<KeyboardEvent, 'key' | 'code' | 'metaKey' | 'ctrlKey' | 'altKey' | 'shiftKey'>): MenuAction | null {
  if (modsHeld(e, 'Ctrl', 'Cmd')) {
    const snooze = snoozeChorded(actions, e.key);
    if (snooze) return snooze;
  }
  const name = (Object.keys(PR_CHORDS) as (keyof PrActions)[]).find((n) => chordPressed(PR_CHORDS[n] ?? '', e));
  const it = name ? actions[name] : undefined;
  if (!it) return null;
  if ('items' in it) {
    const first = it.items.find((x) => x !== 'separator' && !('items' in x));
    return first && first !== 'separator' && !('items' in first) ? first : null;
  }
  return it;
}

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
  /** Approves it, where you may: it's open and it isn't yours (prStatus.ts `mayApprove`). */
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
  /** The board's lanes, for Primary session's choices: the PR's sessions in them. */
  lanes?: readonly Lane[];
}

/** Nudge its session on updates, checked while it's on: for a PR the board lists, which has a
 *  key, while it's open. Each PR's own, off until turned on unless `[pull_requests]
 *  nudge_by_default` turns it on for yours; only its primary session is told (nudges.rs). */
function nudgeItem(pr: { key?: string; state?: Pr['state']; nudge?: boolean }, report: Report): MenuAction | undefined {
  const { key, state } = pr;
  if (!key || !state || state === 'merged' || state === 'closed') return undefined;
  const on = !!pr.nudge;
  return { label: 'Nudge its session on updates', checked: on, run: () => void setPrNudge(key, !on).catch((e: unknown) => report.error(errorText(e))) };
}

/** Primary session: an open PR's sessions on this Mac, its primary one checked, when it has more
 *  than one (board.rs `pr_primary`). The one picked is the one a nudge tells, as `codealong pr
 *  nudge --session` makes it, and stays so until another is. */
function primaryItem(pr: { key?: string; state?: Pr['state']; primary?: string; sessions?: PrItem['sessions'] }, lanes: readonly Lane[] | undefined, report: Report): MenuGroup | undefined {
  const { key, state } = pr;
  if (!key || !lanes || !state || state === 'merged' || state === 'closed') return undefined;
  const sessions = (pr.sessions ?? []).flatMap((x) => lanes.find((l) => l.id === x.lane && !l.device)?.sessions.find((s) => s.key === x.key) ?? []);
  if (sessions.length < 2) return undefined;
  const pick = (s: Session) => void setPrPrimary(key, s.key).catch((e: unknown) => report.error(errorText(e)));
  return { label: 'Primary session', items: sessions.map((s) => ({ label: `${s.title} · ${s.account}`, checked: s.key === pr.primary, run: () => pick(s) })) };
}

/** A PR's menu: opening it, its session, reviewing it, showing it; approving it, snoozing your
 *  review, having its sessions told of what happens on it and which one; pinning it; its worktree
 *  and copying its link and branch. */
export function prMenu(pr: Pick<Pr, 'url' | 'number'> & { head?: string | null; key?: string; state?: Pr['state']; nudge?: boolean; primary?: string; sessions?: PrItem['sessions'] }, can: PrCan): MenuSpec {
  const { report } = can;
  const s = can.session;
  return sections(
    [
      { label: 'Open on GitHub', run: can.open ?? (() => void openUrl(pr.url).catch((e: unknown) => report.error(errorText(e)))) },
      ...(s ? [{ label: s.session.live ? 'Open its session' : 'Open its latest session', run: s.open }] : []),
      ...(can.review ? [{ label: 'Review with', items: can.review }] : []),
      ...(can.reveal ? [{ label: 'Show in Codealong', run: can.reveal }] : []),
    ],
    [...(can.approve ? [{ label: `Approve #${pr.number}`, run: can.approve }] : []), ...some(can.snooze), ...some(nudgeItem(pr, report)), ...some(primaryItem(pr, can.lanes, report))],
    [...some(can.pin), ...(can.desktop && pr.key ? [desktopItem('pr', pr.key, can.desktop, report)] : [])],
    [
      ...(can.showLane ? [{ label: `Show worktree ${can.showLane.lane.callsign}`, run: can.showLane.run }] : []),
      ...some(copyItem('Copy link', pr.url, 'PR link', report)),
      ...some(copyItem('Copy branch name', pr.head, 'branch name', report)),
      ...(can.canvas ?? []),
    ],
  );
}

/** One way to open a PR's changed file in an editor: the editor's name and icon, and the run. */
export interface FileOpen {
  editor: string;
  icon?: MenuImage;
  run: () => void;
}

/** What a window can do with a PR's changed file. */
export interface PrFileCan {
  report: Report;
  /** As a diff, where an editor of yours opens one (the board's `apps.diffEditor`). */
  diff?: FileOpen;
  /** At its first change, in your editor, where a worktree on this Mac holds the PR. */
  file?: FileOpen & { line: number | null };
  github: () => void;
}

/** A PR's changed file's menu (PrFiles.tsx): opening it as a diff or at its first change in your
 *  editor, on GitHub, and copying its path. */
export function prFileMenu(path: string, can: PrFileCan): MenuSpec {
  const { diff, file } = can;
  return sections(
    [
      ...(diff ? [{ label: `Open diff in ${diff.editor}`, icon: diff.icon, run: diff.run }] : []),
      ...(file ? [{ label: file.line ? `Open in ${file.editor} at line ${file.line}` : `Open in ${file.editor}`, icon: file.icon, run: file.run }] : []),
      { label: 'Open on GitHub', run: can.github },
    ],
    some(copyItem('Copy path', path, 'path', can.report)),
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

/** Ready for review, or Back to in progress (`reviewChange`). */
function reviewItems(ts: Todo[], edit: TodoEdit): MenuSpec {
  const r = reviewChange(ts);
  return r ? [{ label: r.label, run: () => r.ops.forEach((op) => edit.run(op)) }] : [];
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
 *  waiting, review, due date, labels and project; pinning it; copying it; deleting it. */
export function todoMenu(t: Todo, can: TodoCan): MenuSpec {
  const { report, edit } = can;
  const s = can.session;
  return sections(
    [
      { label: 'Start session…', run: can.start },
      ...(s ? [{ label: `Open ${s.session.account}: ${s.session.title.slice(0, 48)}`, run: s.open }] : []),
      ...(can.reveal ? [{ label: 'Show in Codealong', run: can.reveal }] : []),
    ],
    edit ? [...statusItems([t], edit), ...waitingItems([t], edit, report), ...reviewItems([t], edit), 'separator', dueItem([t], edit), ...labelsItems(t, edit), ...projectItems(t, edit)] : [],
    can.desktop ? [desktopItem('todo', t.id, can.desktop, report)] : [],
    [copyPromptItem([t], report), ...some(copyItem('Copy todo ID', t.id, 'todo ID', report)), ...(can.canvas ?? [])],
    can.remove ? [{ label: 'Delete todo', run: can.remove }] : [],
  );
}

/** The menu of several todos picked together: one session for them all; their status, waiting,
 *  review and due date; copying them; deleting them. */
export function todosMenu(ts: Todo[], can: Pick<TodoCan, 'report' | 'start' | 'edit' | 'remove'>): MenuSpec {
  const { report, edit } = can;
  return sections(
    [{ label: `Start one session for ${ts.length} todos…`, run: can.start }],
    edit ? [...statusItems(ts, edit), ...waitingItems(ts, edit, report), ...reviewItems(ts, edit), dueItem(ts, edit)] : [],
    [copyPromptItem(ts, report), ...some(copyItem('Copy todo IDs', ts.map((t) => t.id).join(' '), 'todo IDs', report))],
    can.remove ? [{ label: `Delete ${ts.length} todos`, run: can.remove }] : [],
  );
}

/** The todo panel's ⋯ menu: what the panel has no control for. */
export function todoPanelMenu(t: Todo, can: Pick<TodoCan, 'report' | 'edit' | 'remove'>): MenuSpec {
  const { report, edit } = can;
  return sections([...(edit ? projectItems(t, edit) : []), copyPromptItem([t], report), ...some(copyItem('Copy todo ID', t.id, 'todo ID', report))], can.remove ? [{ label: 'Delete todo', run: can.remove }] : []);
}
