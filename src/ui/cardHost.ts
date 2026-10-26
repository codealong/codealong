// What's the same around a session's card in the windows that aren't the board's: the floating
// bar's peek and a session kept on the desktop (Peek.tsx), and the session the bar docked in the
// notch shows under its pills (NotchSession.tsx). Each shows one session as a card
// (SessionCard.tsx) and is told only which, so both go by these: the session as the board has it
// now, while the card is up (`useLiveSession`), or as your other devices say for one of theirs (the
// bar lists those too, barfeed.rs), and what a click on something around it does,
// which is show it in the main window and put the card away (`useCardHost`). A PR waiting on
// your review shown as a card (PrCard.tsx, in the notch) is kept up the same way (`useLivePr`).
// A card in a window that floats draws at the zoom and takes its keys (`zoomKey`, zoom.ts).

import { useCallback, useMemo, useRef, useState, useEffect } from 'react';
import type { PrTab } from '../agentReview';
import { barNote, getBoard, getDevices, onDevices, prAt, showMain, zoomWindows } from '../api';
import { boardView, lanesAhead, observe, useAhead } from '../ahead';
import { showContextMenu } from '../contextMenu';
import { deviceOf, shortOf } from '../devices';
import { useEditorIcons } from '../editors';
import { followBoard, type Following } from '../followBoard';
import { homeOf } from '../lanes';
import * as menus from '../menus';
import { openWhere, where } from '../open';
import { prRefOf } from '../prLinks';
import { errorText, type Report } from '../report';
import { reuse } from '../reuse';
import type { Board, DeviceView, DevicesView, Lane, PrItem, Session, Theme, Todo, Usage } from '../types';
import { zoomStep } from '../zoom';
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
  /** The zoom, in percent, once a board has said. */
  zoom: number | null;
  /** The board's PRs, worktrees and todos, for what the header says around the session and for
   *  PR links in the conversation. The last ones stay for the next session shown. */
  board: CardBoard | null;
  /** The device it's on, when that's another of yours, as your devices last said: whether it's
   *  connected and takes prompts from here (SessionPanel.tsx). */
  device: DeviceView | null;
  /** Asks for the board now: a PR link's card goes by it, and would ask GitHub without. */
  needBoard: () => void;
}

/** The session `told` of, kept up with the board while it's `shown`: the windows that show one
 *  don't get the board otherwise, so it's asked for as the card shows and then taken from the
 *  pushes that come while it's up. One on another device is kept up the same way from what your
 *  devices say (`devices` events); the board still says the theme, the zoom and what's around it
 *  here. What a push says counts only for the telling it came under: told of a session again, the
 *  window shows it as told, which is newer. */
export function useLiveSession(told: Told | null, shown: boolean): LiveSession {
  const [live, setLive] = useState<{ under: Told; lane: Lane; session: Session; theme: Theme; zoom: number | null } | null>(null);
  const [there, setThere] = useState<{ under: Told; lane: Lane; session: Session } | null>(null);
  // Your devices as they last said, for the device of whichever session is shown: going from one
  // of theirs to another reads them again only as they change.
  const [devices, setDevices] = useState<DevicesView | null>(null);
  // The theme and zoom as the latest push says, for one the board doesn't list (another device's).
  const [look, setLook] = useState<{ theme: Theme; zoom: number | null } | null>(null);
  const [board, setBoard] = useState<CardBoard | null>(null);
  const takeBoard = useCallback((b: Board) => setBoard((prev) => reuse(prev, { prs: b.prs, lanes: b.lanes, todos: b.todos, sides: b.sides, pinned: b.pinned, nudgeAbout: b.nudgeAbout, reviews: b.reviews, repos: b.repos, teamFiles: b.teamFiles })), []);
  const toldNow = useRef(told);
  toldNow.current = told;

  // One listener while a session is shown, whichever it is: going from one to the next doesn't
  // ask for the board again. The board is read as the card shows, and a read older than a push
  // that came first is dropped (followBoard.ts).
  const key = told?.session.key ?? null;
  const up = shown && key !== null;
  const elsewhere = key !== null && shortOf(key) !== null;
  const following = useRef<Following | null>(null);
  useEffect(() => {
    if (!up) return;
    // The board's pushes come while the card is up, so one that didn't read comes with the next.
    const f = followBoard((b, how) => {
      if (how === 'push') {
        observe(boardView(b.generatedAt, [b.lanes]));
        const under = toldNow.current;
        const home = under && homeOf(b.lanes, under.session.key);
        if (under && home) setLive({ under, ...home, theme: b.theme, zoom: b.zoom ?? null });
      }
      setLook((prev) => (prev?.theme === b.theme && prev.zoom === (b.zoom ?? null) ? prev : { theme: b.theme, zoom: b.zoom ?? null }));
      takeBoard(b);
    });
    following.current = f;
    return () => {
      f.off();
      following.current = null;
    };
  }, [up, takeBoard]);

  // Another device's session, from what your devices say: read as the card shows, then each push.
  useEffect(() => {
    if (!up || !elsewhere) return;
    let over = false;
    const take = (d: DevicesView) => {
      if (over) return;
      // Its time isn't this board's: a change shown ahead goes once this view shows it.
      observe(boardView(null, [d.lanes]));
      setDevices(d);
      const under = toldNow.current;
      const home = under && homeOf(d.lanes, under.session.key);
      if (under && home) setThere({ under, ...home });
    };
    void getDevices().then((d) => d && take(d), () => {});
    const off = onDevices(take);
    return () => {
      over = true;
      off();
    };
  }, [up, elsewhere]);

  const ahead = useAhead();
  const now = live && live.under === told ? live : null;
  const nowThere = there && there.under === told ? there : null;
  const engineLane = nowThere?.lane ?? now?.lane ?? told?.lane ?? null;
  const lane = useMemo(() => (engineLane ? lanesAhead([engineLane], ahead)[0] : null), [engineLane, ahead]);
  const session = lane?.sessions.find((s) => s.key === key) ?? nowThere?.session ?? now?.session ?? told?.session ?? null;
  const needBoard = useCallback(() => {
    const failed = () => setBoard((prev) => prev ?? { prs: [], lanes: [], todos: [] });
    // While it follows the board, by the same rule as its first read.
    if (following.current) following.current.read(failed);
    else void getBoard().then(takeBoard, failed);
  }, [takeBoard]);
  return { lane, session, theme: now?.theme ?? (elsewhere ? look?.theme : null) ?? null, zoom: now?.zoom ?? (elsewhere ? look?.zoom : null) ?? null, board, device: elsewhere && key ? deviceOf(devices, key) : null, needBoard };
}

/** ⌘= ⌘- ⌘0 on a card in a window that floats: they zoom every window, as in the board's, and
 *  the card says where it got to. True when it took the key. */
export function zoomKey(e: KeyboardEvent, report: Report): boolean {
  const step = zoomStep(e);
  if (!step) return false;
  e.preventDefault();
  zoomWindows(step).then(
    (percent) => report.info(`Zoomed to ${percent}%.`),
    (err: unknown) => report.error(errorText(err)),
  );
  return true;
}

export interface LivePr {
  /** The PR as the board has it now, else as the window was told of it; null until told. */
  pr: PrItem | null;
  /** The board's worktrees, once a board came, else the ones the PR was told with: where it's
   *  checked out and where its sessions are listed, and where a review of it can start. */
  lanes: Lane[];
  usage: Usage[];
  apps: Board['apps'] | null;
  board: CardBoard | null;
  /** The board no longer lists it. */
  gone: boolean;
  needBoard: () => void;
}

/** The PR `key`, kept up with the board while it's `shown`: asked for as the card shows (`prAt`),
 *  then taken from the pushes that come while it's up. */
export function useLivePr(key: string | null, shown: boolean): LivePr {
  const [told, setTold] = useState<{ key: string; pr: PrItem | null; lanes: Lane[]; apps: Board['apps'] | null } | null>(null);
  const [now, setNow] = useState<{ board: CardBoard; lanes: Lane[]; usage: Usage[]; apps: Board['apps'] } | null>(null);
  const take = useCallback((b: Board) => {
    setNow((prev) => ({ board: reuse<CardBoard>(prev?.board ?? { prs: [], lanes: [], todos: [] }, { prs: b.prs, lanes: b.lanes, todos: b.todos, sides: b.sides, pinned: b.pinned, nudgeAbout: b.nudgeAbout, reviews: b.reviews, repos: b.repos, teamFiles: b.teamFiles }), lanes: b.lanes, usage: b.usage, apps: b.apps }));
  }, []);

  useEffect(() => {
    if (key === null) return;
    let over = false;
    prAt(key).then(
      (at) => {
        if (!over) setTold(at ? { key, pr: at.pr, lanes: at.lanes, apps: at.apps } : { key, pr: null, lanes: [], apps: null });
      },
      () => {
        if (!over) setTold({ key, pr: null, lanes: [], apps: null });
      },
    );
    return () => {
      over = true;
    };
  }, [key]);

  const up = shown && key !== null;
  const following = useRef<Following | null>(null);
  useEffect(() => {
    if (!up) return;
    // A read that didn't come back is made up for by the next push.
    const f = followBoard((b, how) => {
      if (how === 'push') observe(boardView(b.generatedAt, [b.lanes]));
      take(b);
    });
    following.current = f;
    return () => {
      f.off();
      following.current = null;
    };
  }, [up, take]);

  // One that didn't read comes with the next push while the card is up.
  const needBoard = useCallback(() => (following.current ? following.current.read() : void getBoard().then(take, () => {})), [take]);
  const known = told && told.key === key ? told : null;
  const onBoardNow = key !== null && now ? (now.board.prs.find((p) => p.key === key) ?? null) : null;
  const pr = onBoardNow ?? known?.pr ?? null;
  // Gone once the window was told there's no such PR, or a board that came since doesn't list it.
  const gone = known !== null && known.pr === null ? true : known !== null && now !== null && onBoardNow === null;
  return { pr, lanes: now?.lanes ?? known?.lanes ?? [], usage: now?.usage ?? [], apps: now?.apps ?? known?.apps ?? null, board: now?.board ?? null, gone, needBoard };
}

export interface CardHost {
  /** What comes of an open or a menu's action is said on the bar, by the pill of the session
   *  shown, since the card may be gone by then. */
  report: Report;
  /** Opens one of a worktree's targets where it lives (open.ts), putting the card away. */
  open: (lane: Lane, target: string) => void;
  /** Each shows in the main window, and the card goes away. */
  showSession: (lane: Lane, session: Session) => void;
  showPr: (pr: PrItem, tab?: PrTab | null) => void;
  showLane: (lane: Lane) => void;
  showTodo: (todo: Todo) => void;
}

/** What the card of the session `sessionKey` does for what's around it, in a window that puts
 *  itself `away` as you go to the main window or to where a session lives. It also answers for
 *  the PR links in the conversation (PrLink.tsx): a click on one shows the PR as a click on its
 *  chip does (`followPr`, in the window's own place for it), a PR's card shows its sessions in
 *  the main window, and their menus are their own. */
export function useCardHost({ sessionKey, anchor, live, apps, terminal, away, followPr }: { sessionKey: string | null; /** The pill the bar says things by, where the card isn't a session's (the reviews pill). */ anchor?: string; live: Pick<LiveSession, 'board' | 'needBoard'>; apps: Board['apps'] | undefined; terminal: string; away: () => void; followPr?: (pr: PrItem, tab: PrTab | null) => void }): CardHost {
  const onDesktop = menus.useDesktopPins();
  const icons = useEditorIcons(apps);
  const by = anchor ?? sessionKey;
  const report: Report = {
    error: (message) => void (by && barNote({ key: by, message, tone: 'error' })),
    info: (message) => void (by && barNote({ key: by, message, tone: 'info' })),
  };
  const showSession = (l: Lane, s: Session) => {
    away();
    void showMain({ kind: 'session', laneId: l.id, key: s.key });
  };
  const showPr = (pr: PrItem, tab?: PrTab | null) => {
    away();
    void showMain({ kind: 'pr', key: pr.key, tab: tab ?? undefined });
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
    followPr,
    showSession,
    openSession: (l, s) => open(l, s.openTarget),
    sessionMenu: (l, s) => {
      const can: menus.SessionCan = { report, open, terminal, reveal: () => showSession(l, s), editors: apps ? { apps, icons } : undefined, desktop: onDesktop, left: away };
      void showContextMenu(menus.sessionMenu(l, s, can)).catch((e: unknown) => report.error(errorText(e)));
    },
    prMenu: (url, pr) => {
      const spec = menus.prMenu(pr ?? { url, number: prRefOf(url)?.number ?? 0 }, { report, reveal: pr ? () => showPr(pr) : undefined, lanes: live.board?.lanes });
      void showContextMenu(spec).catch((e: unknown) => report.error(errorText(e)));
    },
    needBoard: live.needBoard,
  }, live.board);
  return { report, open, showSession, showPr, showLane, showTodo };
}
