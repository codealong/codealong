// The waiting window's page (waiting.rs has its window): what waits on you, one at a time, in a
// window of its own that you size and keep where you like. It shows the first session that needs
// you, with its conversation and the box that answers it (SessionCard.tsx), or a PR waiting on
// your review with the box that reviews it (PrCard.tsx), and goes on to the next as each is dealt
// with: every session in every project that needs you, then the PRs waiting on your review, then
// the sessions whose turn ended, each the longest wait first (quick.ts `queueRows`, queue.ts
// `byPlace`), snoozed ones left out. With nothing waiting it says so, and the first that comes
// to wait on you shows by itself.
//
// It works through them as a session's panel in Sessions and the notch do, with the same keys,
// from the same code (queue.ts, useWaitingQueue.ts): ⏎ sends and stays to watch the reply, ⌘⏎
// sends (or allows the call the session waits on) and goes on to the next, ⌘E marks a finished
// turn seen and goes on, ⌘] or ⌥⇧↓ skips to the next and ⌘[ or ⌥⇧↑ goes back, ⌥↑ ⌥↓ step
// along what waits, and ⌃- ⌃⇧- go back and forward through the ones looked at here
// (history.ts). The session's own shortcuts (⌃⌘A, ⌃⌘R, ⌃⌘P) are its card's.
//
// What's around a session (its worktree, todos and PRs) shows in the main window, which comes
// forward beside this one; a session it came from, or that came of it, shows here in its place.
//
// The window has no title bar (waiting.rs): the card's strip is one, and with nothing shown the
// whole window drags.

import { useEffect, useMemo, useRef, useState } from 'react';
import { onWaitingHidden, onWaitingOpen, showMain, waitingAsked, type WaitingOpen } from '../api';
import { boardAhead, boardView, observe, renameAhead, sentWorthSaying, useAhead } from '../ahead';
import { showContextMenu, type MenuSpec } from '../contextMenu';
import { useEditorIcons } from '../editors';
import { followBoard } from '../followBoard';
import { useSessionHistory } from '../history';
import { liveOrLatest } from '../lanes';
import { useLatest } from '../latest';
import * as menus from '../menus';
import { clearNotice, failedNotice, reportNotice } from '../notices';
import { openWhere, where } from '../open';
import { prRefOf } from '../prLinks';
import { ALL_PROJECTS, QUEUE_READS, queueRows, reviewSince, type QueueRow } from '../quick';
import { errorText, type Report } from '../report';
import { reuse } from '../reuse';
import { repoReviews } from '../repoSettings';
import type { Board, Lane, PrItem, Session } from '../types';
import { shownSession, useWaitingQueue } from '../useWaitingQueue';
import { PrCard, type PrWindowCan } from './PrCard';
import { usePrLinks } from './PrLink';
import { prPlaces, type PrRowItem } from './PrsView';
import { SessionCard, type WindowCan } from './SessionCard';
import { useTheme } from './ThemeSwitch';
import { Reading } from './Tip';
import { useNow } from './useNow';
import { chordMod } from '../platform';

/** Where the sessions looked at here are kept, for ⌃- and ⌃⇧- (history.ts), apart from the other
 *  windows'. */
const LOOKED_KEY = 'codealong.sessionHistory:waiting';
const BOARD_NOTICE = 'waiting-board';

export function Waiting() {
  const [engineBoard, setBoard] = useState<Board | null>(null);
  // Changes made in any window that the engine hasn't shown yet (ahead.ts), on top of the board.
  const ahead = useAhead();
  const board = useMemo(() => (engineBoard ? boardAhead(engineBoard, ahead) : null), [engineBoard, ahead]);
  // Up, or put away: the page follows the board only while it's up.
  const [up, setUp] = useState(true);
  // Counts the times it was brought up after being put away, and the one the board was last read
  // for: what it shows first goes by a board read since, not the one it had when put away.
  const [opened, setOpened] = useState(0);
  const [readFor, setReadFor] = useState(-1);
  // The session or PR shown, by its key; none shows the first that waits, or says nothing does.
  const [key, setKey] = useState<string | null>(null);
  // The session shown with its name ready to change.
  const [renameKey, setRenameKey] = useState<string | null>(null);
  const now = useNow(30_000);
  const icons = useEditorIcons(board?.apps);
  const onDesktop = menus.useDesktopPins();
  useTheme(board?.theme ?? 'system');

  // Followed afresh each time it's brought up (followBoard.ts): any board that comes then, read or
  // pushed, was made since it came up.
  useEffect(() => {
    if (!up) return;
    const following = followBoard(
      (b) => {
        setBoard((prev) => (prev ? reuse(prev, b) : b));
        setReadFor(opened);
        clearNotice(BOARD_NOTICE);
      },
      (e) => reportNotice(failedNotice(BOARD_NOTICE, 'Can’t read sessions', 'The waiting window couldn’t read your sessions', e)),
    );
    return following.off;
  }, [up, opened]);
  // What it knows of the changes ahead of the engine follows the board while it's up.
  useEffect(() => {
    if (up && engineBoard) observe(boardView(engineBoard.generatedAt, [engineBoard.lanes]));
  }, [up, engineBoard]);

  // The session shown, while it's on the board.
  const replying = useMemo(() => {
    if (!board || !key) return null;
    for (const l of board.lanes) {
      const session = l.sessions.find((x) => x.key === key);
      if (session) return { lane: l, session };
    }
    return null;
  }, [board, key]);
  // A PR waiting on your review, shown while the board lists it.
  const reviewing = useMemo(() => (board && key && !replying ? (board.prs.find((p) => p.key === key) ?? null) : null), [board, key, replying]);
  // What it works through. Snoozed ones wait their time (quick.ts `groupOf`).
  const queue = useMemo(() => (board ? queueRows(board, ALL_PROJECTS) : []), [board]);
  const answerable = useMemo(() => new Set(board ? [...board.lanes.flatMap((l) => l.sessions.filter((s) => !s.archived).map((s) => s.key)), ...board.prs.map((p) => p.key)] : []), [board]);
  const shownKey = replying?.session.key ?? reviewing?.key ?? null;
  const looked = useSessionHistory(up ? shownKey : null, (k) => answerable.has(k), LOOKED_KEY);

  /** Shows a session or a review here, in place of the one shown. */
  const show = (k: string, rename = false) => {
    setKey(k);
    setRenameKey(rename ? k : null);
  };

  const way = useWaitingQueue<QueueRow>({
    waiting: queue,
    along: queue,
    reads: QUEUE_READS,
    shown: replying ? shownSession(replying.session) : reviewing ? { key: reviewing.key, title: `#${reviewing.number} ${reviewing.title}`, since: reviewSince(reviewing), seenable: false, skipsOnSeen: true } : null,
    show: (row) => show(row.key),
    travel: (dir) => {
      const to = dir === 'back' ? looked.goBack() : looked.goForward();
      if (to) show(to);
      return to !== null;
    },
  });

  // Opening: on what was asked for; with nothing asked, on the one shown while it's up, else the
  // first that waits. What the latest open asked is read as the page comes up, and each open
  // after that comes as an event.
  // Whether it's up as the events say, at once: put away and brought up again in one go, it starts
  // afresh, before a render would say it was put away.
  const upNow = useRef(true);
  const shownNow = useLatest(shownKey);
  const fresh = way.fresh;
  useEffect(() => {
    const apply = (asked: WaitingOpen | null) => {
      const wasUp = upNow.current;
      upNow.current = true;
      setUp(true);
      if (!wasUp) setOpened((n) => n + 1);
      if (asked?.key) {
        fresh();
        setKey(asked.key);
        setRenameKey(asked.rename ? asked.key : null);
        return;
      }
      if (wasUp && shownNow.current) return;
      fresh();
      setKey(null);
      setRenameKey(null);
    };
    const offOpen = onWaitingOpen(apply);
    const offHidden = onWaitingHidden(() => {
      upNow.current = false;
      setUp(false);
    });
    void waitingAsked().then(apply, () => apply(null));
    return () => {
      offOpen();
      offHidden();
    };
  }, [fresh, upNow, shownNow]);

  // With nothing shown, or one that's gone from the board, the first that waits takes the window,
  // once the board's been read since it was brought up.
  const first = way.first;
  const read = readFor === opened;
  useEffect(() => {
    if (!up || !read || !board || board.loading || (key !== null && (replying || reviewing))) return;
    setRenameKey(null);
    setKey(first ? QUEUE_READS.keyOf(first) : null);
  }, [up, read, board, key, replying, reviewing, first]);

  /** Done with the one shown and nothing else waits: the window says so. */
  const went = (to: 'on' | 'last' | 'stay' | 'away') => {
    if (to === 'last') {
      setKey(null);
      setRenameKey(null);
    }
  };

  /** What comes of an open or a menu's action is said on the strip of the one shown. */
  const report: Report = {
    error: (message) => way.say({ text: message, error: true }),
    info: (message) => way.say({ text: message, error: false }),
  };
  const openHere = (lane: Lane, target: string) => openWhere(where(lane, target), { report, from: 'main' });
  const openSession = (lane: Lane, s: Session) => openHere(lane, s.openTarget);
  const revealSession = (lane: Lane, s: Session) => void showMain({ kind: 'session', laneId: lane.id, key: s.key });
  const revealPr = (pr: PrItem) => void showMain({ kind: 'pr', key: pr.key });
  const revealLane = (lane: Lane) => void showMain({ kind: 'worktree', laneId: lane.id });

  /** What the window does for a session's actions (SessionCard.tsx has the rest): the one shown
   *  goes on to the next that waits once it's left what waits on you; another is shown here. */
  const can = (lane: Lane, s: Session): WindowCan => ({
    report,
    open: openHere,
    reveal: () => revealSession(lane, s),
    reply: s.key === shownKey ? undefined : () => show(s.key),
    rename: () => show(s.key, true),
    left: s.key === shownKey ? (done) => went(way.left(s.key, done)) : undefined,
    worktrees: board?.lanes,
  });
  const sessionMenu = (lane: Lane, s: Session): MenuSpec => menus.sessionMenu(lane, s, { ...can(lane, s), terminal: board?.apps.terminal ?? 'Terminal', editors: board ? { apps: board.apps, icons } : undefined, desktop: onDesktop });
  const showMenu = (spec: MenuSpec) => void showContextMenu(spec).catch((e: unknown) => report.error(errorText(e)));

  /** The latest session that worked on a PR, where it's listed. */
  const prSession = (pr: PrItem): { lane: Lane; session: Session } | null => {
    const found = pr.sessions.flatMap((x) => {
      const lane = board?.lanes.find((l) => l.id === x.lane);
      const session = lane?.sessions.find((s) => s.key === x.key);
      return lane && session ? [{ lane, session }] : [];
    });
    return liveOrLatest(found);
  };
  /** A PR as its card shows it: with where it lives on this Mac (PrsView.tsx `prPlaces`). */
  const reviewItem = (pr: PrItem): PrRowItem => {
    const lanes = board?.lanes ?? [];
    const holding = lanes.filter((l) => pr.lanes.includes(l.id));
    const sessions = pr.sessions.flatMap((x) => {
      const lane = lanes.find((l) => l.id === x.lane);
      const session = lane?.sessions.find((s) => s.key === x.key);
      return lane && session ? [{ lane, session }] : [];
    });
    return { kind: 'pr', key: pr.key, pr, lane: holding[0] ?? null, sessions, places: prPlaces(holding, sessions) };
  };
  const reviewCan = (pr: PrItem): PrWindowCan => ({
    report,
    open: openHere,
    reveal: () => revealPr(pr),
    left: (done) => went(way.left(pr.key, done)),
    // The review didn't go, after the window went on from it: it's back in what's left, and the
    // strip says why; with nothing shown since, the window is back on it.
    failed: (why) => {
      way.again(pr.key);
      if (shownNow.current === null) {
        setKey(pr.key);
        way.say({ text: why, error: true }, pr.key);
      } else way.say({ text: why, error: true });
    },
  });

  // PR links in a conversation shown here (PrLink.tsx): a session on a PR's card shows here too,
  // and the PR in the main window.
  usePrLinks(board?.prs, board?.lanes, {
    showPr: revealPr,
    showSession: (_lane, s) => show(s.key),
    openSession,
    sessionMenu: (lane, s) => showMenu(sessionMenu(lane, s)),
    prMenu: (url, pr) => {
      const s = pr ? prSession(pr) : null;
      showMenu(
        menus.prMenu(pr ?? { url, number: prRefOf(url)?.number ?? 0 }, {
          report,
          session: s ? { ...s, open: () => openSession(s.lane, s.session) } : undefined,
          reveal: pr ? () => revealPr(pr) : undefined,
          desktop: onDesktop,
        }),
      );
    },
  }, board);

  // The queue's keys, with nothing else taking them, and typing with nothing focused goes to the
  // box.
  const onQueueKey = way.onKey;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.isComposing) return;
      if (onQueueKey(e)) {
        e.preventDefault();
        return;
      }
      const el = document.activeElement;
      if (e.key.length === 1 && !chordMod(e) && (!el || el === document.body)) {
        document.querySelector<HTMLTextAreaElement>('.ca-waiting .ca-composer textarea')?.focus();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onQueueKey]);

  const back = looked.back ? () => void way.run('historyBack') : null;
  const apps = board?.apps ?? { terminal: 'Terminal', terminalId: '', editor: 'Editor', editorId: '' };

  return (
    <div className="ca-waiting" data-tauri-drag-region={replying || reviewing ? undefined : 'deep'}>
      {replying ? (
        <SessionCard
          lane={replying.lane}
          session={replying.session}
          apps={apps}
          now={now}
          zoom={board?.zoom}
          board={board}
          can={can(replying.lane, replying.session)}
          canFor={can}
          // A session it came from, or that came of it, shows here in its place.
          onShowSession={(_l, s) => show(s.key)}
          onShowLane={revealLane}
          onShowTodo={(t) => void showMain({ kind: 'todo', id: t.id })}
          onShowPr={revealPr}
          prCan={(pr) => {
            const on = prSession(pr);
            return { session: on ? { ...on, open: () => openSession(on.lane, on.session) } : undefined };
          }}
          autoFocus
          keys
          queue={way.queue}
          notice={way.notice}
          hotkeys={{ onBack: back, titleBar: true }}
          renaming={renameKey === replying.session.key}
          onRename={(name) => {
            const s = replying.session;
            setRenameKey(null);
            if (name) renameAhead(s.key, name).catch((e: unknown) => report.error(errorText(e)));
          }}
          onDone={(done) => went(way.answered(replying.session.key, done))}
          onSent={(sent) => {
            // What you'd want to know: the prompt is on the clipboard, say, or it may not have
            // gone in.
            if (sentWorthSaying(sent)) report.info(sent.message);
          }}
          // It didn't go, after the window went on from it: it's back in what's left, and the
          // strip says why.
          onFailed={(why) => {
            const s = replying.session;
            way.again(s.key);
            way.say({ text: `Didn’t send to ${s.title}: ${why}`, error: true });
          }}
        />
      ) : reviewing ? (
        <PrCard
          key={reviewing.key}
          item={reviewItem(reviewing)}
          now={now}
          zoom={board?.zoom}
          apps={apps}
          lanes={board?.lanes ?? []}
          usage={board?.usage ?? []}
          reviews={repoReviews(board, reviewing.repoSlug)}
          teamFiles={board?.teamFiles}
          can={reviewCan(reviewing)}
          autoFocus
          queue={way.queue}
          notice={way.notice}
          hotkeys={{ onBack: back, titleBar: true }}
          onShowSession={(_l, s) => show(s.key)}
          onOpenSession={openSession}
          onShowLane={revealLane}
          onLaneMenu={(l) => showMenu(menus.laneMenu(l, { report, open: openHere, reveal: () => revealLane(l), editors: board ? { apps: board.apps, icons } : undefined, desktop: onDesktop }))}
          onSessionMenu={(l, s) => showMenu(sessionMenu(l, s))}
        />
      ) : !board || board.loading ? (
        <Reading what="Reading sessions…" place="waiting" />
      ) : (
        <div className="ca-waiting-empty" role="status">
          <h1>Nothing waits on you</h1>
          <p>Sessions that need you, PRs waiting on your review and turns that ended show here, one at a time, as they come.</p>
        </div>
      )}
    </div>
  );
}
