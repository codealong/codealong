// A link to a GitHub PR in a conversation (Markdown.tsx) or in what you wrote
// (DescriptionEditor.tsx). A bare one reads `name#123`, with the PR's state as its icon once
// Codealong knows the PR. Hovering it brings up a card: where the PR stands on GitHub, the
// worktrees that hold it and the sessions that worked on it, as the PRs view has them. A click
// opens the link in the browser, and a right-click has the PR's menu (menus.ts). A PR's chip in a
// session's panel (`PrChipCard`) has the same card. For an open PR the board lists, the card has
// the switch that has its primary session, listed first, told of what happens on it (nudges.rs).
//
// It costs nothing until you hover: the board already lists the PRs in your projects' repos with
// their sessions, and each link watches only its own PR there, so a push re-renders a link only
// when its PR changed. A PR the board doesn't list (an older one, or another repo's) is asked of
// GitHub once its card shows (pr_card, which matches it to sessions as the board does), and the
// answer is kept a while for every link to it.

import { useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { openUrl, prCard, setPrNudge } from '../api';
import { absolute, agoPhrase, count } from '../format';
import { prIndex, prRefOf, type PrRef } from '../prLinks';
import { nudgeWords } from '../prStatus';
import { errorText } from '../report';
import { nudgeAboutFor } from '../repoSettings';
import type { Board, Lane, Pr, PrItem, Session } from '../types';
import { PanelSession, PR_PRIMARY } from './PanelSession';
import { PrPills } from './PrPills';
import { Button, cx, Icon, PR_ICON, PR_WORD, PrChip } from './primitives';
import { useDismiss } from './useDismiss';

/** What a window does with a PR link's card: showing and opening what's on it, and menus. */
export interface PrLinkActions {
  /** Shows the PR in the main window's PRs view. Only for a PR the board lists. */
  showPr?: (pr: PrItem) => void;
  /** A click on a session: shows it in this window, or the main one. */
  showSession: (lane: Lane, session: Session) => void;
  /** Double-click: opens it where it runs. */
  openSession: (lane: Lane, session: Session) => void;
  sessionMenu: (lane: Lane, session: Session) => void;
  /** The PR's right-click menu, with the board's PR when it lists it. */
  prMenu: (url: string, pr: PrItem | null) => void;
  /** Opens a web link; the quick panel steps aside first. The browser, without it. */
  openLink?: (url: string) => void;
  /** Reads the board, for a window that has none yet (the peek, until the first push): a card
   *  waits for it rather than ask GitHub about a PR the board lists. */
  needBoard?: () => void;
}

/** This window's PRs and lanes and what it does with them (`usePrLinks`). Each window is a page
 *  of its own, so one is enough. */
const host = {
  prs: undefined as readonly PrItem[] | undefined,
  index: new Map<string, PrItem>(),
  lanes: [] as readonly Lane[],
  /** What a PR's nudge tells of, which its card says: `[pull_requests] nudge_about`, and what its
   *  repo's team or your table for it says (repoSettings.ts `nudgeAboutFor`). */
  nudges: {} as NudgeSettings,
  actions: null as PrLinkActions | null,
  listeners: new Set<() => void>(),
};

/** PRs GitHub was asked about, by link key: kept a while, then asked again on the next hover. */
interface Fetched {
  at: number;
  item?: PrItem;
  error?: string;
  loading: boolean;
}
const fetched = new Map<string, Fetched>();
const FRESH_MS = 5 * 60_000;
const FAILED_MS = 30_000;

/** Links hear about a new board and about answers from GitHub. */
function subscribe(cb: () => void): () => void {
  host.listeners.add(cb);
  return () => void host.listeners.delete(cb);
}

function notify() {
  for (const cb of host.listeners) cb();
}

const lanesNow = () => host.lanes;

/** Asks GitHub about a PR, unless it was asked lately or is being asked now. */
function fetchCard(ref: PrRef) {
  const hit = fetched.get(ref.key);
  if (hit && (hit.loading || performance.now() - hit.at < (hit.error ? FAILED_MS : FRESH_MS))) return;
  const done = (f: { item?: PrItem; error?: string }) => {
    fetched.set(ref.key, { at: performance.now(), loading: false, ...f });
    notify();
  };
  fetched.set(ref.key, { at: performance.now(), item: hit?.item, loading: true });
  notify();
  prCard(ref.slug, ref.number)
    .then((item) => done({ item }))
    .catch((e: unknown) => done({ item: hit?.item, error: errorText(e) }));
}

/** What says what a PR's nudge tells of: the board's, each part as it was pushed. */
type NudgeSettings = Partial<Pick<Board, 'nudgeAbout' | 'repos' | 'teamFiles'>>;

/** Gives this window's PR links the board's PRs and lanes, and what the window does with them,
 *  with what a PR's nudge tells of (`nudges`, from the board). Links rendering with it read the
 *  new board at once; links that skip rendering hear about it after, and re-render only when their
 *  own PR changed. */
export function usePrLinks(prs: readonly PrItem[] | undefined, lanes: readonly Lane[] | undefined, actions: PrLinkActions, nudges: NudgeSettings | null | undefined) {
  host.actions = actions;
  if (host.prs !== prs) {
    host.prs = prs;
    host.index = prIndex(prs ?? []);
  }
  host.lanes = lanes ?? [];
  host.nudges = { nudgeAbout: nudges?.nudgeAbout, repos: nudges?.repos, teamFiles: nudges?.teamFiles };
  useLayoutEffect(notify, [prs, lanes, nudges?.nudgeAbout, nudges?.repos, nudges?.teamFiles]);
}

const SHOW_MS = 350;
const HIDE_MS = 200;
/** After a card closes, another link's shows at once for this long. */
const WARM_MS = 500;
/** The one card that's up, so a new one closes it. */
let shown: (() => void) | null = null;
let warmUntil = 0;

/** A PR's card for what shows the PR, a link or a chip: the handlers its anchor takes, and the
 *  card while it's up. Without a window that says what to do with it (a page that never called
 *  usePrLinks), there's no card. */
function usePrCard(url: string, ref: PrRef) {
  const hosted = host.actions !== null;
  const anchor = useRef<HTMLElement | null>(null);
  const [open, setOpen] = useState(false);
  const timer = useRef(0);
  const closeRef = useRef(() => setOpen(false));

  useEffect(
    () => () => {
      window.clearTimeout(timer.current);
      if (shown === closeRef.current) shown = null;
    },
    [],
  );

  const show = () => {
    window.clearTimeout(timer.current);
    const go = () => {
      if (shown && shown !== closeRef.current) shown();
      shown = closeRef.current;
      setOpen(true);
    };
    if (performance.now() < warmUntil) go();
    else timer.current = window.setTimeout(go, SHOW_MS);
  };
  const close = () => {
    window.clearTimeout(timer.current);
    if (shown === closeRef.current) {
      shown = null;
      warmUntil = performance.now() + WARM_MS;
    }
    setOpen(false);
  };
  const hide = () => {
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(close, HIDE_MS);
  };

  const follow = () => {
    close();
    const openLink = host.actions?.openLink;
    if (openLink) openLink(url);
    else void openUrl(url);
  };

  const card = open && anchor.current ? <PrCard anchor={anchor.current} prRef={ref} url={url} onEnter={show} onLeave={hide} onClose={close} onFollow={follow} /> : null;
  return { hosted, anchor, show, hide, close, follow, card };
}

export function PrLink({ url, pr: ref, children }: { url: string; pr: PrRef; children: ReactNode }) {
  const item = useSyncExternalStore(subscribe, () => host.index.get(ref.key) ?? fetched.get(ref.key)?.item);
  const c = usePrCard(url, ref);

  return (
    <>
      <a
        ref={(el) => void (c.anchor.current = el)}
        className="ca-md-a ca-prlink"
        href={url}
        onClick={(e) => {
          e.preventDefault();
          e.stopPropagation();
          c.follow();
        }}
        onMouseEnter={c.hosted ? c.show : undefined}
        onMouseLeave={c.hosted ? c.hide : undefined}
        onContextMenu={
          c.hosted
            ? (e) => {
                e.preventDefault();
                e.stopPropagation();
                c.close();
                host.actions?.prMenu(url, host.index.get(ref.key) ?? null);
              }
            : undefined
        }
      >
        {item && (
          <span className={`ca-lane-pricon ca-prlink-icon ca-pr--${item.state}`}>
            <Icon name={PR_ICON[item.state]} size={12} stroke={2} title={PR_WORD[item.state]} />
          </span>
        )}
        {children}
      </a>
      {c.card}
    </>
  );
}

/** A PR's chip whose card shows on hover, as a link to it does: its title and the rest are
 *  there, so the chip needs no line of its own. A click opens it on GitHub, and a right-click
 *  has its menu. */
export function PrChipCard({ pr, onOpen, onMenu }: { pr: Pr; onOpen?: () => void; onMenu?: () => void }) {
  const ref = prRefOf(pr.url);
  if (!ref) return <PrChip pr={pr} onOpen={onOpen} onMenu={onMenu} />;
  return <HoveredChip pr={pr} prRef={ref} onOpen={onOpen} onMenu={onMenu} />;
}

function HoveredChip({ pr, prRef, onOpen, onMenu }: { pr: Pr; prRef: PrRef; onOpen?: () => void; onMenu?: () => void }) {
  const c = usePrCard(pr.url, prRef);
  return (
    <span ref={(el) => void (c.anchor.current = el)} className="ca-prchipcard" onMouseEnter={c.hosted ? c.show : undefined} onMouseLeave={c.hosted ? c.hide : undefined}>
      <PrChip
        pr={pr}
        titled={!c.hosted}
        onOpen={
          onOpen &&
          (() => {
            c.close();
            onOpen();
          })
        }
        onMenu={
          onMenu &&
          (() => {
            c.close();
            onMenu();
          })
        }
      />
      {c.card}
    </span>
  );
}

/** Sessions listed before "and N more". */
const SESSIONS_SHOWN = 5;
/** Room kept between the card and the window's edges, and between it and the link. */
const EDGE = 8;
const GAP = 4;

interface CardProps {
  anchor: HTMLElement;
  prRef: PrRef;
  url: string;
  onEnter: () => void;
  onLeave: () => void;
  onClose: () => void;
  onFollow: () => void;
}

function PrCard({ anchor, prRef: ref, url, onEnter, onLeave, onClose, onFollow }: CardProps) {
  const card = useRef<HTMLDivElement>(null);
  const listed = useSyncExternalStore(subscribe, () => host.index.get(ref.key));
  const lanes = useSyncExternalStore(subscribe, lanesNow);
  const asked = useSyncExternalStore(subscribe, () => fetched.get(ref.key));
  const hasBoard = useSyncExternalStore(subscribe, () => host.prs !== undefined);
  const pr = listed ?? asked?.item;
  const now = Date.now();

  useEffect(() => {
    if (listed) return;
    if (hasBoard || !host.actions?.needBoard) fetchCard(ref);
    else host.actions.needBoard();
  }, [listed, hasBoard, ref]);

  // Under the link, or over it when there's more room there; inside the window either way, and
  // scrolling within itself when the window is short of room.
  const place = useRef(() => {});
  place.current = () => {
    const el = card.current;
    if (!el) return;
    const rects = anchor.isConnected ? anchor.getClientRects() : null;
    if (!rects || rects.length === 0) return onClose();
    const first = rects[0];
    const last = rects[rects.length - 1];
    el.style.maxHeight = '';
    const natural = el.offsetHeight;
    const below = window.innerHeight - last.bottom - GAP - EDGE;
    const above = first.top - GAP - EDGE;
    const under = natural <= below || below >= above;
    const height = Math.max(0, Math.min(natural, under ? below : above));
    const at = under ? last : first;
    el.style.maxHeight = `${height}px`;
    el.style.top = `${under ? last.bottom + GAP : first.top - GAP - height}px`;
    el.style.left = `${Math.max(EDGE, Math.min(at.left, window.innerWidth - el.offsetWidth - EDGE))}px`;
  };
  useLayoutEffect(() => place.current());

  // It goes on a key (which still does what it does), a press elsewhere, or when the window goes
  // to the background; and it follows the link as the conversation scrolls.
  useDismiss(card, onClose, { inside: (t) => anchor.contains(t), keys: 'any', blur: true });
  useEffect(() => {
    let frame = 0;
    const onMove = () => {
      window.cancelAnimationFrame(frame);
      frame = window.requestAnimationFrame(() => place.current());
    };
    window.addEventListener('scroll', onMove, true);
    window.addEventListener('resize', onMove);
    return () => {
      window.cancelAnimationFrame(frame);
      window.removeEventListener('scroll', onMove, true);
      window.removeEventListener('resize', onMove);
    };
  }, []);

  const laneById = new Map(lanes.map((l) => [l.id, l]));
  // Its primary session first, the one a nudge tells.
  const isPrimary = (s: Session) => s.key === pr?.primary;
  const sessions = (pr?.sessions ?? [])
    .flatMap((x) => {
      const lane = laneById.get(x.lane);
      const session = lane?.sessions.find((s) => s.key === x.key);
      return lane && session ? [{ lane, session }] : [];
    })
    .sort((a, b) => Number(isPrimary(b.session)) - Number(isPrimary(a.session)) || Number(b.session.live) - Number(a.session.live) || b.session.lastActivity - a.session.lastActivity);
  const primary = sessions.find((x) => isPrimary(x.session));
  const holding = (pr?.lanes ?? []).flatMap((id) => laneById.get(id) ?? []);
  const ended = pr?.mergedAt ?? pr?.closedAt ?? null;
  // The nudge, for an open PR the board lists: shown as just set until the board says so too.
  const [wanted, setWanted] = useState<boolean | null>(null);
  const [nudgeError, setNudgeError] = useState<string | null>(null);
  const canNudge = !!listed && listed.state !== 'merged' && listed.state !== 'closed';
  const nudges = wanted ?? !!listed?.nudge;
  useEffect(() => {
    if (wanted !== null && !!listed?.nudge === wanted) setWanted(null);
  }, [listed?.nudge, wanted]);
  const setNudge = (key: string, on: boolean) => {
    setWanted(on);
    setNudgeError(null);
    setPrNudge(key, on).catch((e: unknown) => {
      setWanted(null);
      setNudgeError(errorText(e));
    });
  };
  // Doing something from the card closes it first.
  const act = (f: (a: PrLinkActions) => void) => {
    onClose();
    if (host.actions) f(host.actions);
  };

  return createPortal(
    <div ref={card} className="ca-prhover" role="dialog" aria-label={`${ref.slug}#${ref.number}`} onMouseEnter={onEnter} onMouseLeave={onLeave}>
      <div className="ca-prhover-head">
        <span className={cx('ca-lane-pricon', pr && `ca-pr--${pr.state}`)}>
          <Icon name={pr ? PR_ICON[pr.state] : 'pr'} size={14} stroke={1.75} title={pr ? PR_WORD[pr.state] : undefined} />
        </span>
        <span className="ca-prhover-ref">
          {ref.slug}#{ref.number}
        </span>
        {pr && <PrPills pr={pr} />}
      </div>
      {pr ? (
        <>
          <p className="ca-prhover-title">{pr.title}</p>
          <p className="ca-prhover-meta">
            <code className="ca-ic">{pr.head}</code>
            {pr.base && (
              <>
                {' → '}
                <code className="ca-ic">{pr.base}</code>
              </>
            )}
          </p>
          <p className="ca-prhover-meta">
            by {pr.mine ? 'you' : (pr.author ?? 'unknown')} ·{' '}
            {ended != null ? (
              <span title={absolute(ended)}>
                {pr.state === 'merged' ? 'merged' : 'closed'} {agoPhrase(ended, now)}
              </span>
            ) : (
              <span title={absolute(pr.updatedAt)}>updated {agoPhrase(pr.updatedAt, now)}</span>
            )}{' '}
            · +{pr.additions.toLocaleString('en-US')} −{pr.deletions.toLocaleString('en-US')} in {count(pr.changedFiles, 'file')}
          </p>
          {holding.length > 0 && (
            <p className="ca-prhover-meta">
              Checked out in <span className="ca-prhover-lanes">{holding.map((l) => l.callsign).join(', ')}</span>
            </p>
          )}
        </>
      ) : asked?.error ? (
        <p className="ca-prhover-error">{asked.error}</p>
      ) : (
        <p className="ca-prhover-meta">Asking GitHub…</p>
      )}
      {sessions.length > 0 && (
        <div className="ca-prhover-sec">
          <h4 className="ca-label">{sessions.length === 1 ? 'Its session' : `Sessions · ${sessions.length}`}</h4>
          <div className="ca-tree" role="tree" aria-label="Sessions">
            {sessions.slice(0, SESSIONS_SHOWN).map(({ lane, session }) => (
              <PanelSession
                key={session.key}
                lane={lane}
                session={session}
                now={now}
                onShow={(l, s) => act((a) => a.showSession(l, s))}
                onOpen={(l, s) => act((a) => a.openSession(l, s))}
                onMenu={(l, s) => host.actions?.sessionMenu(l, s)}
                primary={isPrimary(session) && sessions.length > 1 ? PR_PRIMARY : undefined}
              />
            ))}
          </div>
          {sessions.length > SESSIONS_SHOWN && <p className="ca-prhover-meta">and {sessions.length - SESSIONS_SHOWN} more</p>}
        </div>
      )}
      {canNudge && listed && (
        <div className="ca-prhover-sec">
          <button type="button" className="ca-prhover-nudge" role="switch" aria-checked={nudges} onClick={() => setNudge(listed.key, !nudges)}>
            <span>Nudge {sessions.length > 1 ? 'its primary session' : 'its session'} on updates</span>
            <span className={cx('ca-switch', nudges && 'is-on')} aria-hidden="true" />
          </button>
          <p className="ca-prhover-meta">
            {nudgeWords(listed, nudges, primary ? { title: primary.session.title, runs: primary.session.live && !primary.lane.device } : null, nudgeAboutFor(host.nudges, listed))}
          </p>
          {nudgeError && <p className="ca-prhover-error">{nudgeError}</p>}
        </div>
      )}
      <div className="ca-prhover-foot">
        {listed && host.actions?.showPr && (
          <Button size="sm" icon="layout-sidebar" onClick={() => act((a) => a.showPr?.(listed))}>
            Show in Codealong
          </Button>
        )}
        <Button size="sm" icon="external" title={url} onClick={onFollow}>
          Open on GitHub
        </Button>
      </div>
    </div>,
    document.body,
  );
}
