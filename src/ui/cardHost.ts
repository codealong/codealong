// What's the same around a session's card in the windows that aren't the board's: the floating
// bar's peek and a session kept on the desktop (Peek.tsx), and the session the bar docked in the
// notch shows under its pills (NotchSession.tsx). Each shows one session as a card
// (SessionCard.tsx) and is told only which, so both go by these: the session as the board has it
// now, while the card is up (`useLiveSession`), and what a click on something around it does,
// which is show it in the main window and put the card away (`useCardHost`).

import { useCallback, useMemo, useRef, useState, useEffect } from 'react';
import { barNote, getBoard, onBoard, showMain } from '../api';
import { boardView, lanesAhead, observe, useAhead } from '../ahead';
import { showContextMenu } from '../contextMenu';
import { useEditorIcons } from '../editors';
import { homeOf } from '../lanes';
import * as menus from '../menus';
import { openWhere, where } from '../open';
import { prRefOf } from '../prLinks';
import { errorText, type Report } from '../report';
import { reuse } from '../reuse';
import type { Board, Lane, PrItem, Session, Theme, Todo } from '../types';
import { usePrLinks } from './PrLink';
import type { CardBoard } from './SessionCard';

/** A session with its worktree, as the window was told of it when it was asked to show it. */
export interface Told {
  lane: Lane;
  session: Session;
}

export interface LiveSession {
  /** The session and its worktree as the board has them now, with what any window changed that
   *  the engine hasn't shown yet (ahead.ts); as told, until a board comes. */
  lane: Lane | null;
  session: Session | null;
  /** The theme, once a board has said. */
  theme: Theme | null;
  /** The board's PRs, worktrees and todos, for what the header says around the session and for
   *  PR links in the conversation. The last ones stay for the next session shown. */
  board: CardBoard | null;
  /** Asks for the board now: a PR link's card goes by it, and would ask GitHub without. */
  needBoard: () => void;
}

/** The session `told` of, kept up with the board while it's `shown`: the windows that show one
 *  don't get the board otherwise, so it's asked for as the card shows and then taken from the
 *  pushes that come while it's up. What a push says counts only for the telling it came under:
 *  told of a session again, the window shows it as told, which is newer. */
export function useLiveSession(told: Told | null, shown: boolean): LiveSession {
  const [live, setLive] = useState<{ under: Told; lane: Lane; session: Session; theme: Theme } | null>(null);
  const [board, setBoard] = useState<CardBoard | null>(null);
  const takeBoard = useCallback((b: Board) => setBoard((prev) => reuse(prev, { prs: b.prs, lanes: b.lanes, todos: b.todos, pinned: b.pinned })), []);
  const toldNow = useRef(told);
  toldNow.current = told;

  // One listener while a session is shown, whichever it is: going from one to the next doesn't
  // ask for the board again.
  const key = told?.session.key ?? null;
  const up = shown && key !== null;
  useEffect(() => {
    if (!up) return;
    void getBoard().then(takeBoard, () => {});
    return onBoard((b) => {
      observe(boardView(b.generatedAt, [b.lanes]));
      const under = toldNow.current;
      const home = under && homeOf(b.lanes, under.session.key);
      if (under && home) setLive({ under, ...home, theme: b.theme });
      takeBoard(b);
    });
  }, [up, takeBoard]);

  const ahead = useAhead();
  const now = live && live.under === told ? live : null;
  const engineLane = now?.lane ?? told?.lane ?? null;
  const lane = useMemo(() => (engineLane ? lanesAhead([engineLane], ahead)[0] : null), [engineLane, ahead]);
  const session = lane?.sessions.find((s) => s.key === key) ?? now?.session ?? told?.session ?? null;
  const needBoard = useCallback(() => void getBoard().then(takeBoard, () => setBoard((prev) => prev ?? { prs: [], lanes: [], todos: [] })), [takeBoard]);
  return { lane, session, theme: now?.theme ?? null, board, needBoard };
}

export interface CardHost {
  /** What comes of an open or a menu's action is said on the bar, by the pill of the session
   *  shown, since the card may be gone by then. */
  report: Report;
  /** Opens one of a worktree's targets where it lives (open.ts), putting the card away. */
  open: (lane: Lane, target: string) => void;
  /** Each shows in the main window, and the card goes away. */
  showSession: (lane: Lane, session: Session) => void;
  showPr: (pr: PrItem) => void;
  showLane: (lane: Lane) => void;
  showTodo: (todo: Todo) => void;
}

/** What the card of the session `sessionKey` does for what's around it, in a window that puts
 *  itself `away` as you go to the main window or to where a session lives. It also answers for
 *  the PR links in the conversation (PrLink.tsx): a PR's card shows its sessions in the main
 *  window, and their menus are their own. */
export function useCardHost({ sessionKey, live, apps, terminal, away }: { sessionKey: string | null; live: LiveSession; apps: Board['apps'] | undefined; terminal: string; away: () => void }): CardHost {
  const onDesktop = menus.useDesktopPins();
  const icons = useEditorIcons(apps);
  const report: Report = {
    error: (message) => void (sessionKey && barNote({ key: sessionKey, message, tone: 'error' })),
    info: (message) => void (sessionKey && barNote({ key: sessionKey, message, tone: 'info' })),
  };
  const showSession = (l: Lane, s: Session) => {
    away();
    void showMain({ kind: 'session', laneId: l.id, key: s.key });
  };
  const showPr = (pr: PrItem) => {
    away();
    void showMain({ kind: 'pr', key: pr.key });
  };
  const showLane = (l: Lane) => {
    away();
    void showMain({ kind: 'worktree', laneId: l.id });
  };
  const showTodo = (t: Todo) => {
    away();
    void showMain({ kind: 'todo', id: t.id });
  };
  const open = (l: Lane, target: string) => openWhere(where(l, target), { report, from: 'panel', before: away });
  usePrLinks(live.board?.prs, live.board?.lanes, {
    showPr,
    showSession,
    openSession: (l, s) => open(l, s.openTarget),
    sessionMenu: (l, s) => {
      const can: menus.SessionCan = { report, open, terminal, reveal: () => showSession(l, s), editors: apps ? { apps, icons } : undefined, desktop: onDesktop, left: away };
      void showContextMenu(menus.sessionMenu(l, s, can)).catch((e: unknown) => report.error(errorText(e)));
    },
    prMenu: (url, pr) => {
      const spec = menus.prMenu(pr ?? { url, number: prRefOf(url)?.number ?? 0 }, { report, reveal: pr ? () => showPr(pr) : undefined });
      void showContextMenu(spec).catch((e: unknown) => report.error(errorText(e)));
    },
    needBoard: live.needBoard,
  });
  return { report, open, showSession, showPr, showLane, showTodo };
}
