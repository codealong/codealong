// The waiting window's page (waiting.rs has its window): what waits on you, one at a time, in a
// window of its own that you size and keep where you like. It shows the first session that needs
// you, with its conversation and the box that answers it (SessionCard.tsx), or a PR waiting on
// your review with the box that reviews it (PrCard.tsx), and goes on to the next as each is dealt
// with: every session in every project that needs you, then the PRs waiting on your review, then
// the sessions whose turn ended, each the longest wait first (quick.ts `queueRows`, queue.ts
// `byPlace`), snoozed ones left out. With nothing waiting it says so, and once you're through
// with the last one, how that went (QueueEnd.tsx, as the notch says it); the first that comes to
// wait on you then shows by itself.
//
// It works through them as a session's panel in Sessions and the notch do, with the same keys,
// from the same code (queue.ts, useWaitingQueue.ts): ⏎ sends and stays to watch the reply, ⌘⏎
// sends (or allows the call the session waits on) and goes on to the next, ⌘E marks a finished
// turn seen and goes on, ⌘] or ⌥⇧↓ skips to the next and ⌘[ or ⌥⇧↑ goes back, ⌥↑ ⌥↓ step
// along what waits, and ⌃- ⌃⇧- go back and forward through the ones looked at here
// (history.ts). The session's own shortcuts (⌃⌘A, ⌃⌘R, ⌃⌘P) are its card's.
//
// What's around a session (its worktree and todos) shows in the main window, which comes forward
// beside this one; a session it came from, or that came of it, shows here in its place, and so
// does a PR from its chip or from a link to it in the conversation: its card leads back to the
// session, and once the PR is merged, reviewed or snoozed the window goes back there rather than
// on (PrCard.tsx `from`), as the main window's panel does.
//
// The window has no title bar (waiting.rs): the card's strip is one, and with nothing shown the
// whole window drags.

import { useEffect, useMemo, useRef, useState } from 'react';
import type { PrTab } from '../agentReview';
import { onWaitingHidden, onWaitingOpen, showMain, waitingAsked, type WaitingOpen } from '../api';
import { boardAhead, renameAhead, sentWorthSaying, useAhead } from '../ahead';
import { prCardItem } from '../boardLists';
import { showContextMenu, type MenuSpec } from '../contextMenu';
import { deviceOf, floatingLanes } from '../devices';
import { useEditorIcons } from '../editors';
import { followBoard } from '../followBoard';
import { useSessionHistory } from '../history';
import { liveOrLatest } from '../lanes';
import { useLatest } from '../latest';
import { useViewKeys } from '../mainKeys';
import * as menus from '../menus';
import { clearNotice, failedNotice, reportNotice } from '../notices';
import { openWhere, where } from '../open';
import { prRefOf } from '../prLinks';
import { ALL_PROJECTS, attentionRows, QUEUE_READS, queueRows, reviewSince, type QueueRow } from '../quick';
import { errorText, type Report } from '../report';
import { reuse } from '../reuse';
import { repoReviews } from '../repoSettings';
import type { Board, Lane, PrItem, Session } from '../types';
import { useDeviceLanes, useDevices, useObserve } from '../useDevices';
import { shownSession, useWaitingQueue } from '../useWaitingQueue';
import { dialogUp } from './focusTrap';
import { PrCard, type CameFrom, type PrWindowCan } from './PrCard';
import { showPrTab } from './PrInspector';
import { usePrLinks } from './PrLink';
import { ProjectShown } from './ProjectSaid';
import { QueueEnd } from './QueueEnd';
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
  // The session the PR `pr` was shown in place of, from its chip or a link in its conversation.
  const [from, setFrom] = useState<(CameFrom & { pr: string }) | null>(null);
  // Each PR shown that way, with its session: gone back and forward to (⌃- ⌃⇧-), it leads back
  // to the session as it did, unless it was shown some other way since (a review the queue went
  // on to).
  const fromChips = useRef(new Map<string, CameFrom>());
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
  // Your other devices' sessions wait here too, the connected ones' (devices.ts `floatingLanes`),
  // with what any window changed of them (useDevices.ts).
  const devices = useDevices(up);
  const deviceLanes = useDeviceLanes(devices, ahead);
  const there = useMemo(() => floatingLanes(engineBoard?.devices, deviceLanes), [engineBoard?.devices, deviceLanes]);
  const everywhere = useMemo(() => (board && there.length > 0 ? { ...board, lanes: board.lanes.concat(there) } : board), [board, there]);
  // What it knows of the changes ahead of the engine follows the board while it's up.
  useObserve(engineBoard, devices, up && !!engineBoard);

  // The session shown, while it's on the board or another device's.
  const replying = useMemo(() => {
    if (!everywhere || !key) return null;
    for (const l of everywhere.lanes) {
      const session = l.sessions.find((x) => x.key === key);
      if (session) return { lane: l, session };
    }
    return null;
  }, [everywhere, key]);
  // A PR waiting on your review, or one shown from a session's chip, while the board lists it.
  const reviewing = useMemo(() => (board && key && !replying ? (board.prs.find((p) => p.key === key) ?? null) : null), [board, key, replying]);
  const cameFrom = from && from.pr === key ? from : null;
  // What it works through. Snoozed ones wait their time (quick.ts `groupOf`).
  const queue = useMemo(() => (everywhere ? queueRows(everywhere, ALL_PROJECTS) : []), [everywhere]);
  // How many sessions are at work, for the end of what waits to say, as the quick panel counts them.
  const working = useMemo(() => (everywhere ? attentionRows(everywhere, ALL_PROJECTS).counts.working : 0), [everywhere]);
  const answerable = useMemo(() => new Set(everywhere ? [...everywhere.lanes.flatMap((l) => l.sessions.filter((s) => !s.archived).map((s) => s.key)), ...everywhere.prs.map((p) => p.key)] : []), [everywhere]);
  const shownKey = replying?.session.key ?? reviewing?.key ?? null;
  const looked = useSessionHistory(up ? shownKey : null, (k) => answerable.has(k), LOOKED_KEY);

  /** Shows a session or a PR here, in place of the one shown: a PR in place of the session `at`,
   *  when it was shown from one. */
  const show = (k: string, rename = false, at: CameFrom | null = null) => {
    setKey(k);
    setRenameKey(rename ? k : null);
    setFrom(at && { ...at, pr: k });
    if (at) fromChips.current.set(k, at);
    else fromChips.current.delete(k);
  };

  const way = useWaitingQueue<QueueRow>({
    waiting: queue,
    along: queue,
    reads: QUEUE_READS,
    shown: replying ? shownSession(replying.session) : reviewing ? { key: reviewing.key, title: `#${reviewing.number} ${reviewing.title}`, since: reviewSince(reviewing), seenable: false, skipsOnSeen: true } : null,
    show: (row) => show(row.key),
    travel: (dir) => {
      const to = dir === 'back' ? looked.goBack() : looked.goForward();
      if (to) show(to, false, fromChips.current.get(to) ?? null);
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
  const cameFromNow = useLatest(cameFrom);
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
        setFrom(null);
        return;
      }
      if (wasUp && shownNow.current) return;
      fresh();
      setKey(null);
      setRenameKey(null);
      setFrom(null);
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
  // once the board's been read since it was brought up; a PR shown from a session's chip goes
  // back to the session.
  const first = way.first;
  const read = readFor === opened;
  const backTo = cameFrom?.key ?? null;
  useEffect(() => {
    if (!up || !read || !board || board.loading || (key !== null && (replying || reviewing))) return;
    setRenameKey(null);
    setKey(backTo ?? (first ? QUEUE_READS.keyOf(first) : null));
  }, [up, read, board, key, replying, reviewing, first, backTo]);

  /** Done with the one shown and nothing else waits: the window says so, and how it went. */
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
  const revealPr = (pr: PrItem, tab?: PrTab | null) => void showMain({ kind: 'pr', key: pr.key, tab: tab ?? undefined });
  const revealLane = (lane: Lane) => void showMain({ kind: 'worktree', laneId: lane.id });
  // A view's keys: Codealong's window comes forward over this one, as Show in Codealong has it.
  useViewKeys();

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
  /** A click on the chip of the session shown's PR, or on a link to it in the conversation: the
   *  PR here in the session's place, on the tab the link goes to. */
  const followPr = (pr: PrItem, tab: PrTab | null = null) => {
    if (!replying) return;
    if (tab) showPrTab(pr.key, tab);
    show(pr.key, false, { key: replying.session.key, title: replying.session.title });
  };
  /** Back from the PR `pr` shown that way to its session, saying there how the PR went; nothing
   *  once another shows, as a review started from it can say so after you've moved on. */
  const returnFrom = (pr: string, said?: string) => {
    const at = cameFromNow.current;
    if (!at || at.pr !== pr || shownNow.current !== pr) return;
    show(at.key);
    if (said) way.say({ text: said, error: false }, at.key);
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

  // PR links in a conversation shown here (PrLink.tsx): a click on one shows the PR here as its
  // chip does, a session on a PR's card shows here too, and Show in Codealong the PR in the main
  // window.
  usePrLinks(board?.prs, board?.lanes, {
    showPr: revealPr,
    followPr: replying ? followPr : undefined,
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
      if (e.defaultPrevented || e.isComposing || dialogUp()) return;
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

  const page = (
    <div className="ca-waiting" data-tauri-drag-region={replying || reviewing ? undefined : 'deep'}>
      {replying ? (
        <SessionCard
          lane={replying.lane}
          session={replying.session}
          apps={apps}
          now={now}
          zoom={board?.zoom}
          board={everywhere}
          device={deviceOf(devices, replying.session.key)}
          can={can(replying.lane, replying.session)}
          canFor={can}
          // A session it came from, or that came of it, shows here in its place.
          onShowSession={(_l, s) => show(s.key)}
          onShowLane={revealLane}
          onShowTodo={(t) => void showMain({ kind: 'todo', id: t.id })}
          onShowPr={revealPr}
          onFollowPr={followPr}
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
          // strip says why; at the end of what waits, the window is back on it, as for a review,
          // since it may not wait on you and nothing else would bring it back.
          onFailed={(why) => {
            const s = replying.session;
            const said = { text: `Didn’t send to ${s.title}: ${why}`, error: true };
            way.again(s.key);
            if (shownNow.current === null) {
              setKey(s.key);
              way.say(said, s.key);
            } else way.say(said);
          }}
        />
      ) : reviewing ? (
        <PrCard
          key={reviewing.key}
          // Where it lives on this Mac, as the PR panel shows it.
          item={prCardItem(reviewing, board?.lanes ?? [])}
          now={now}
          zoom={board?.zoom}
          apps={apps}
          lanes={board?.lanes ?? []}
          usage={board?.usage ?? []}
          reviews={repoReviews(board, reviewing.repoSlug)}
          teamFiles={board?.teamFiles}
          can={reviewCan(reviewing)}
          from={cameFrom ? { ...cameFrom, onBack: (said) => returnFrom(reviewing.key, said) } : undefined}
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
        <QueueEnd said={way.ended} working={working} className="ca-waiting-empty" />
      )}
    </div>
  );
  // What waits comes from every project, so a session says its project before its worktree.
  return <ProjectShown.Provider value={ALL_PROJECTS}>{page}</ProjectShown.Provider>;
}
