// The floating bar's peek (peek.rs has its window): the session whose pill is under the pointer,
// with the header its panel has in the main window, its conversation, and the box that answers it
// (SessionPanel.tsx, as a card). A click in it gives it the
// keyboard; ⏎ sends and the peek closes at once, so the app you were in has the keyboard back (a
// message that doesn't go says so on the bar, and is back in the box). Once an answer went
// through it closes too, but stays while the box holds something for you to send next. Escape
// closes it too, and what you'd typed stays with the session (drafts.ts), here and in its panels,
// until you send it. Its header's ✓ marks a finished turn seen, its snooze button snoozes the
// session, and its archive button archives it, any of which takes it off the bar; its ⋯ button,
// or a right-click, has the session's menu (menus.ts). Rename in that menu, or in a pill's, shows
// the session's name ready to change; ⏎ renames it and the peek closes. While it's up, the
// session follows the board, and the peek stays on it as its pill moves along the bar.

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { barNote, getBoard, inPreview, onBoard, onPeekHidden, onPeekOpen, onPeekShown, openUrl, peekFit, peekFocus, peekHide, peekOpen, peekReady, showMain } from '../api';
import { boardView, lanesAhead, observe, renameAhead, useAhead } from '../ahead';
import { showContextMenu } from '../contextMenu';
import { readDraft } from '../drafts';
import { homeOf } from '../lanes';
import { useEditorIcons } from '../editors';
import * as menus from '../menus';
import { attentionRows, ALL_PROJECTS } from '../quick';
import { sessionContext } from '../sessionContext';
import type { Board, Lane, PeekOpen, Pr, PrItem, Session, Theme, Todo } from '../types';
import { prRefOf } from '../prLinks';
import { reuse } from '../reuse';
import { useKeyPerf } from '../perf';
import { usePrLinks } from './PrLink';
import { SessionPanel } from './SessionPanel';
import { useTheme } from './ThemeSwitch';
import { useNow } from './useNow';
import { errorText, type Report } from '../report';
import { openWhere, where } from '../open';

/** Room around the card for its shadow (`--shadow-peek`, which falls away from the bar): the
 *  window (peek.rs) is the card, this on its sides, `FAR` on the side away from the bar, and
 *  `NEAR` on the side toward the bar, where it mustn't cover the bar's pills. */
const MARGIN = 20;
const FAR = 32;
const NEAR = 8;
/** How long an open waits for the conversation before showing what it has. */
const LOAD_WAIT_MS = 250;
/** Where a preview (scripts/preview.sh) says which session to peek at. */
const PREVIEW_KEY = 'codealong.preview.peek';

/** A preview has no engine to open the peek: it opens on the session named for it, else the first
 *  one that needs you. */
async function previewOpen(): Promise<PeekOpen | null> {
  const board = await getBoard();
  let key: string | null = null;
  try {
    key = window.localStorage.getItem(PREVIEW_KEY);
  } catch {
    // No storage: the first session that needs you.
  }
  key = key || attentionRows(board, ALL_PROJECTS).rows[0]?.session.key || null;
  const home = homeOf(board.lanes, key);
  if (!home) return null;
  const { lane, session } = home;
  return { id: 1, lane, session, terminal: board.apps.terminal, apps: board.apps, theme: board.theme, room: 580 + FAR + NEAR, below: true, focus: false, rename: false };
}

export function Peek() {
  const [open, setOpen] = useState<PeekOpen | null>(null);
  // The session as the board has it now, while the peek is up.
  const [live, setLive] = useState<{ lane: Lane; session: Session; theme: Theme } | null>(null);
  const [shown, setShown] = useState(false);
  const [engaged, setEngaged] = useState(false);
  // The session whose name the peek's own menu asked to change.
  const [renameAsked, setRenameAsked] = useState<string | null>(null);
  const now = useNow(30_000);
  const card = useRef<HTMLDivElement>(null);
  // The open answered with its height, and the height last reported after it.
  const answered = useRef<number | null>(null);
  const reported = useRef(0);
  // The session whose conversation is laid out.
  const loadedView = useRef<string | null>(null);

  useEffect(() => {
    const offOpen = onPeekOpen((o) => {
      setOpen(o);
      setRenameAsked(null);
      setLive(null);
      setShown(true);
      setEngaged(o.focus);
    });
    const offHidden = onPeekHidden(() => {
      setShown(false);
      setEngaged(false);
      loadedView.current = null;
    });
    const offShown = onPeekShown((s) => setEngaged(s.engaged));
    if (inPreview) {
      void previewOpen().then((o) => {
        if (!o) return;
        setOpen(o);
        setShown(true);
      });
    }
    return () => {
      offOpen();
      offHidden();
      offShown();
    };
  }, []);

  // The board's PRs, worktrees and todos, for what the header says around the session (its PRs,
  // its todos, the sessions it came from and that came of it) and for PR links in the conversation
  // (PrLink.tsx): asked for as the peek shows, then from the pushes that come while it's up. The
  // last ones stay for the next time it shows.
  const [prBoard, setPrBoard] = useState<Pick<Board, 'prs' | 'lanes' | 'todos'> | null>(null);
  const takeBoard = useCallback((b: Board) => setPrBoard((prev) => reuse(prev, { prs: b.prs, lanes: b.lanes, todos: b.todos })), []);

  const key = open?.session.key ?? null;
  useEffect(() => {
    if (!shown || !key) return;
    void getBoard().then(takeBoard, () => {});
    return onBoard((b) => {
      observe(boardView(b.generatedAt, [b.lanes]));
      const home = homeOf(b.lanes, key);
      if (home) setLive({ ...home, theme: b.theme });
      takeBoard(b);
    });
  }, [shown, key, takeBoard]);

  // With changes made in any window that the engine hasn't shown yet (ahead.ts).
  const ahead = useAhead();
  const engineLane = live?.lane ?? open?.lane ?? null;
  const lane = useMemo(() => (engineLane ? lanesAhead([engineLane], ahead)[0] : null), [engineLane, ahead]);
  const sessionKey = live?.session.key ?? open?.session.key ?? null;
  const session = lane?.sessions.find((s) => s.key === sessionKey) ?? live?.session ?? open?.session ?? null;
  const theme = live?.theme ?? open?.theme ?? 'system';
  useTheme(theme);
  // For the session's menu: what's on the desktop, and the editors' icons.
  const onDesktop = menus.useDesktopPins();
  const icons = useEditorIcons(open?.apps);
  // What its header says around it, from the board.
  const context = useMemo(() => (lane && session ? sessionContext(prBoard, lane, session) : null), [prBoard, lane, session]);

  const measure = () => Math.ceil((card.current?.getBoundingClientRect().height ?? 0) + FAR + NEAR);

  /** Answers the open with the page's height, once: when the conversation is laid out, or after a
   *  moment without it. */
  const ready = useCallback(() => {
    if (!open || answered.current === open.id) return;
    answered.current = open.id;
    reported.current = measure();
    void peekReady(open.id, reported.current);
  }, [open]);
  useEffect(() => {
    if (!open) return;
    const id = window.setTimeout(ready, LOAD_WAIT_MS);
    return () => window.clearTimeout(id);
  }, [open, ready]);

  // An open for the session already shown (Reply here on the pill it shows) answers at once; a
  // hidden peek shows nothing, so each showing reads the conversation afresh.
  const view = open?.session.key ?? '';
  const loaded = () => {
    loadedView.current = view;
    ready();
  };
  useLayoutEffect(() => {
    if (loadedView.current === view) ready();
  }, [open, view, ready]);

  // Once shown, the window follows the card: new steps, a longer draft, an approval coming up.
  useLayoutEffect(() => {
    const el = card.current;
    if (!el) return;
    const observer = new ResizeObserver(() => {
      if (!open || answered.current !== open.id) return;
      const h = measure();
      if (h === reported.current) return;
      reported.current = h;
      void peekFit(h);
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, [open, shown]);

  // With the perf log on, how long typed keys take to show.
  const keyed = useKeyPerf(open?.perfLog ?? false);
  useEffect(() => {
    window.addEventListener('keydown', keyed, true);
    return () => window.removeEventListener('keydown', keyed, true);
  }, [keyed]);

  // Escape closes it, and ⌘Q, ⌘H and ⌘W close it rather than going to Codealong: the menu bar is
  // the other app's. Typing with nothing focused goes to the box.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.isComposing) return;
      if (e.key === 'Escape') {
        e.preventDefault();
        void peekHide();
        return;
      }
      if (e.metaKey && !e.ctrlKey && !e.altKey && ['q', 'h', 'w'].includes(e.key.toLowerCase())) {
        e.preventDefault();
        void peekHide();
        return;
      }
      const el = document.activeElement;
      if (e.key.length === 1 && !e.metaKey && !e.ctrlKey && (!el || el === document.body)) {
        document.querySelector<HTMLTextAreaElement>('.ca-peek .ca-composer textarea')?.focus();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  // A PR link's card: its sessions show in the main window, and what comes of an open or a menu is
  // said on the bar by the peek's own session.
  const cardReport: Report = {
    error: (message) => void (key && barNote({ key, message, tone: 'error' })),
    info: (message) => void (key && barNote({ key, message, tone: 'info' })),
  };
  const showSessionMain = (l: Lane, s: Session) => {
    void peekHide();
    void showMain({ kind: 'session', laneId: l.id, key: s.key });
  };
  const openFromCard = (l: Lane, target: string) => openWhere(where(l, target), { report: cardReport, from: 'panel', before: () => void peekHide() });
  const showPrMain = (pr: PrItem) => {
    void peekHide();
    void showMain({ kind: 'pr', key: pr.key });
  };
  usePrLinks(prBoard?.prs, prBoard?.lanes, {
    showPr: showPrMain,
    showSession: showSessionMain,
    openSession: (l, s) => openFromCard(l, s.openTarget),
    sessionMenu: (l, s) => {
      const can: menus.SessionCan = {
        report: cardReport,
        open: openFromCard,
        terminal: open?.terminal ?? 'Terminal',
        reveal: () => showSessionMain(l, s),
        editors: open?.apps ? { apps: open.apps, icons } : undefined,
        desktop: onDesktop,
        left: () => void peekHide(),
      };
      void showContextMenu(menus.sessionMenu(l, s, can)).catch((e: unknown) => cardReport.error(errorText(e)));
    },
    prMenu: (url, pr) => {
      const spec = menus.prMenu(pr ?? { url, number: prRefOf(url)?.number ?? 0 }, { report: cardReport, reveal: pr ? () => showPrMain(pr) : undefined });
      void showContextMenu(spec).catch((e: unknown) => cardReport.error(errorText(e)));
    },
    // Without it, a card asks GitHub.
    needBoard: () => void getBoard().then(takeBoard, () => setPrBoard((prev) => prev ?? { prs: [], lanes: [], todos: [] })),
  });

  if (!open || !lane || !session || !shown) return null;

  /** What comes of an open or the menu's actions is said on the bar, by the session's pill,
   *  since the peek may be gone by then. */
  const report: Report = {
    error: (message) => void barNote({ key: session.key, message, tone: 'error' }),
    info: (message) => void barNote({ key: session.key, message, tone: 'info' }),
  };
  /** Opens one of the worktree's targets where it lives (open.ts), putting the peek away. */
  const openOn = (l: Lane, target: string) => openWhere(where(l, target), { report, from: 'panel', before: () => void peekHide() });

  /** Takes the session off the bar, and puts the peek away. */
  const dismiss = (act: Promise<void>) => {
    void peekHide();
    act.catch((e: unknown) => report.error(errorText(e)));
  };

  /** Renames the session, or leaves its name, and puts the peek away: that's what it opened for. */
  const renamed = (name: string | null) => {
    const here = renameAsked === session.key && !open.rename;
    setRenameAsked(null);
    if (name) dismiss(renameAhead(session.key, name));
    // Asked from the peek's own menu, leaving the name goes back to the conversation.
    else if (!here) void peekHide();
  };

  const reveal = () => {
    void peekHide();
    void showMain({ kind: 'session', laneId: lane.id, key: session.key });
  };

  /** What the peek can do to its session (menus.ts): its menu, and the header's buttons among
   *  its actions. Whatever takes the session off the bar puts the peek away. */
  const can: menus.SessionCan = {
    report,
    open: openOn,
    terminal: open.terminal,
    reveal,
    editors: open.apps ? { apps: open.apps, icons } : undefined,
    desktop: onDesktop,
    rename: () => setRenameAsked(session.key),
    left: () => void peekHide(),
  };
  const menu = (at?: { x: number; y: number }) => void showContextMenu(menus.sessionMenu(lane, session, can), at).catch((e: unknown) => report.error(errorText(e)));
  /** A session it came from, or that came of it, takes the peek's place; its menu is its own. */
  const showKin = (_l: Lane, s: Session) => void peekOpen(s.key, null, engaged).catch((e: unknown) => report.error(errorText(e)));
  const kinMenu = (l: Lane, s: Session) =>
    void showContextMenu(menus.sessionMenu(l, s, { ...can, reveal: () => showSessionMain(l, s), rename: undefined })).catch((e: unknown) => report.error(errorText(e)));
  const showLane = (l: Lane) => {
    void peekHide();
    void showMain({ kind: 'worktree', laneId: l.id });
  };
  const showTodo = (t: Todo) => {
    void peekHide();
    void showMain({ kind: 'todo', id: t.id });
  };
  const prMenu = (pr: Pr) => {
    const item = prBoard?.prs.find((p) => p.url === pr.url);
    void showContextMenu(menus.prMenu(item ?? pr, { report, reveal: item ? () => showPrMain(item) : undefined })).catch((e: unknown) => report.error(errorText(e)));
  };

  return (
    <div
      className={open.below ? 'ca-peek-root' : 'ca-peek-root is-over'}
      style={{ padding: open.below ? `${NEAR}px ${MARGIN}px ${FAR}px` : `${FAR}px ${MARGIN}px ${NEAR}px` }}
      onMouseDown={(e) => {
        // The shadow around the card is part of the window; a click there is a click outside.
        if (e.target === e.currentTarget) void peekHide();
      }}
    >
      <div
        ref={card}
        className="ca-peek"
        style={{ maxHeight: open.room - FAR - NEAR }}
        onPointerDown={() => {
          // Hovering never gives the peek the keyboard; a click does.
          if (!engaged) void peekFocus();
        }}
      >
        <SessionPanel
          key={view}
          variant="card"
          lane={lane}
          session={session}
          apps={open.apps ?? { terminal: open.terminal, terminalId: '', editor: 'Editor', editorId: '' }}
          now={now}
          autoFocus={open.focus}
          onLoaded={loaded}
          onOpen={(target) => openOn(lane, target)}
          actions={menus.sessionActions(lane, session, can)}
          renaming={open.rename || renameAsked === session.key}
          onRename={renamed}
          onMenu={menu}
          onReveal={reveal}
          onShowLane={showLane}
          prs={context?.prs}
          onOpenPr={(pr) => openUrl(pr.url).catch((e: unknown) => report.error(errorText(e)))}
          onPrMenu={prMenu}
          todos={context?.todos}
          onShowTodo={showTodo}
          lineage={context?.lineage}
          onShowSession={showKin}
          onSessionMenu={kinMenu}
          onSending={() => void peekHide()}
          onSent={(sent) => {
            // What you'd want to know goes on the bar: the prompt is on the clipboard, say, or it
            // went as a message from Codealong because its terminal couldn't be typed into.
            if (sent.opened || sent.fellBack) void barNote({ key: session.key, message: sent.message, tone: 'info' });
          }}
          // It's back in the box for when the peek shows it again.
          onFailed={(why) => void barNote({ key: session.key, message: why, tone: 'error' })}
          onAnswered={() => {
            // What you wrote while it waited is likely what you'd send next.
            if (!readDraft(session.key)) void peekHide();
          }}
        />
      </div>
    </div>
  );
}
