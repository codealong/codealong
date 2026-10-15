// A session as a card of its own, wherever a window shows one session with a box to answer it:
// the floating bar's peek and a session kept on the desktop (Peek.tsx), and the waiting window
// (Waiting.tsx). It's the session's panel (SessionPanel.tsx) with what's the same in all
// of them done once: what the header says around the session (its PRs, its todos, the sessions it
// came from and that came of it), the session's menu on ⋯ and on a right-click, the menus of the
// PRs and sessions the header names, and the Sessions list's shortcuts for the session shown:
// ⌘E marks it seen, ⌃⌘A archives it, ⌃⌘R renames it, ⌃⌘P pins it in Sessions and ⌃` opens a
// terminal in its folder, each shown beside its item in the menu. It draws at the zoom, and ⌘=
// ⌘- ⌘0 zoom it as they do the board's windows (zoom.ts, cardHost.ts `zoomKey`).
//
// A window says only what differs: how it reports, how it opens things and shows them in
// Codealong, what it does once the session has left (the peek goes away, the waiting window goes
// on to the next), and what it adds (a queue, a pin that keeps it on the desktop).

import { useEffect, useMemo, useRef } from 'react';
import { openUrl, setPinned } from '../api';
import { showContextMenu, type MenuAction, type MenuSpec } from '../contextMenu';
import { useEditorIcons } from '../editors';
import * as menus from '../menus';
import { errorText, type Report } from '../report';
import { useLatest } from '../latest';
import { sessionContext } from '../sessionContext';
import { useStartAccounts } from '../startAccounts';
import type { Board, Lane, Pr, PrItem, Session, Todo } from '../types';
import { zoomKey } from './cardHost';
import { SessionPanel, type SessionPanelProps } from './SessionPanel';

/** What the card reads from the board: what's around the session, and what's pinned in Sessions. */
export type CardBoard = Pick<Board, 'prs' | 'lanes' | 'todos' | 'sides' | 'nudgeAbout' | 'reviews' | 'repos' | 'teamFiles'> & { pinned?: string[] };

/** What a window does for a session's actions (menus.ts `SessionCan`), less what the card knows
 *  for every window: the apps, what's on the desktop, the pin and the shortcuts. A window that has
 *  the board gives its worktrees, for Move to worktree. */
export type WindowCan = Pick<menus.SessionCan, 'report' | 'open' | 'reveal' | 'reply' | 'rename' | 'left' | 'worktrees'>;

interface SessionCardProps extends Pick<SessionPanelProps, 'zoom' | 'queue' |'notice' | 'hotkeys' | 'autoFocus' | 'keys' | 'renaming' | 'onRename' | 'onKeep' | 'keepTitle' | 'onClose' | 'closeTitle' | 'onSending' | 'onSent' | 'onFailed' | 'onAnswered' | 'onDone' | 'onLoaded'> {
  lane: Lane;
  session: Session;
  /** The terminal's name and the editors installed, for the ways to open it. */
  apps: Board['apps'];
  now: number;
  /** The board as the window has it; without one yet, the header says only what the session's
   *  own worktree does. */
  board: CardBoard | null | undefined;
  /** What this window does for the session shown. */
  can: WindowCan;
  /** And for another session the header names: one it came from, or that came of it. */
  canFor: (lane: Lane, session: Session) => WindowCan;
  /** The session shown is on its way out: the window asked for another, which hasn't come yet
   *  (the notch keeps the old card up meanwhile, never an empty one). Its shortcuts do nothing
   *  until then, so a quick ⌃⌘W or ⌘E doesn't act on the session you just left. */
  outgoing?: boolean;
  /** A click on one of those. */
  onShowSession: (lane: Lane, session: Session) => void;
  onShowLane: (lane: Lane) => void;
  onShowTodo: (todo: Todo) => void;
  /** Shows a PR the board lists in the main window: Show in Codealong, in its menu. */
  onShowPr?: (pr: PrItem) => void;
  /** More of the menu of a PR the board lists, where the window has more (its session). */
  prCan?: (pr: PrItem) => Partial<menus.PrCan>;
}

/** Pin session or Unpin session, as Sessions has it: pinned sessions lead that list. */
function pinItem(board: CardBoard | null | undefined, s: Session, report: Report): MenuAction | undefined {
  const pinned = board?.pinned;
  if (!pinned) return undefined;
  const on = pinned.includes(s.key);
  const save = () =>
    setPinned(on ? pinned.filter((k) => k !== s.key) : [s.key, ...pinned]).then(
      () => report.info(on ? 'Unpinned in Sessions.' : 'Pinned at the top of Sessions.'),
      (e: unknown) => report.error(errorText(e)),
    );
  return { label: on ? 'Unpin session' : 'Pin session', run: () => void save() };
}

export function SessionCard({ lane, session, apps, now, board, can, canFor, outgoing = false, onShowSession, onShowLane, onShowTodo, onShowPr, prCan, renaming, ...panel }: SessionCardProps) {
  const { report } = can;
  const icons = useEditorIcons(apps);
  const onDesktop = menus.useDesktopPins();
  /** A window's part for a session, with the card's: the same menu in every window. */
  const whole = (s: Session, base: WindowCan): menus.SessionCan => ({ ...base, terminal: apps.terminal, editors: { apps, icons }, desktop: onDesktop, pin: pinItem(board, s, report), keys: 'chords' });
  const mine = whole(session, can);
  // Where one that isn't running goes on (Resume in, Carry on) is its account's places, which a
  // window reads once: the card shows them as they come in.
  useStartAccounts(!session.live);
  const actions = menus.sessionActions(lane, session, mine);
  const context = useMemo(() => sessionContext(board, lane, session), [board, lane, session]);
  const show = (spec: MenuSpec, at?: { x: number; y: number }) => void showContextMenu(spec, at).catch((e: unknown) => report.error(errorText(e)));

  // The Sessions list's shortcuts, for the session shown, and ⌘= ⌘- ⌘0, which zoom it as they do
  // the board's windows. Not while its name is being typed.
  const latest = useRef(actions);
  latest.current = actions;
  const leaving = useRef(outgoing);
  leaving.current = outgoing;
  const reportNow = useLatest(report);
  useEffect(() => {
    if (renaming) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.isComposing) return;
      if (zoomKey(e, reportNow.current)) return;
      const action = menus.chorded(latest.current, e);
      if (!action) return;
      e.preventDefault();
      // Taken, so nothing else acts on it, but not run: it would be for the session going.
      if (leaving.current) return;
      action.run();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [renaming, reportNow]);

  const open = (url: string) => void openUrl(url).catch((e: unknown) => report.error(errorText(e)));
  /** A PR's menu, as every window has it: for one the board lists, with showing it in Codealong
   *  and pinning it to the desktop. */
  const prMenu = (pr: Pr) => {
    const item = board?.prs.find((p) => p.url === pr.url);
    show(menus.prMenu(item ?? pr, { report, reveal: item && onShowPr ? () => onShowPr(item) : undefined, desktop: onDesktop, ...(item ? prCan?.(item) : undefined) }));
  };
  /** A click on a PR's chip: the PR in the main window, or on GitHub when the board doesn't list it. */
  const showPr = (pr: Pr) => {
    const item = board?.prs.find((p) => p.url === pr.url);
    if (item && onShowPr) onShowPr(item);
    else open(pr.url);
  };

  return (
    <SessionPanel
      key={session.key}
      variant="card"
      lane={lane}
      session={session}
      apps={apps}
      now={now}
      {...panel}
      renaming={renaming}
      onOpen={(target) => can.open(lane, target)}
      actions={actions}
      onMenu={(at) => show(menus.sessionMenu(lane, session, mine), at)}
      onThreadMenu={(items) => show([...items, 'separator', ...menus.sessionMenu(lane, session, mine)])}
      onShowLane={onShowLane}
      prs={context.prs}
      onShowPr={onShowPr && showPr}
      onOpenPr={(pr) => open(pr.url)}
      onPrMenu={prMenu}
      todos={context.todos}
      onShowTodo={onShowTodo}
      lineage={context.lineage}
      onShowSession={onShowSession}
      onSessionMenu={(l, s) => show(menus.sessionMenu(l, s, whole(s, canFor(l, s))))}
    />
  );
}
