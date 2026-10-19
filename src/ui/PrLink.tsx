// A link to a GitHub PR in a conversation (Markdown.tsx) or in what you wrote
// (DescriptionEditor.tsx). A bare one reads `name#123`, with the PR's state as its icon once
// Codealong knows the PR; one in a tool's output keeps its words as written, without the icon.
// Hovering it brings up a card: where the PR stands on GitHub, the worktrees that hold it and the
// sessions that worked on it, as the PRs view has them. A click shows the PR in Codealong when
// the board lists it, as a click on its chip does, on the tab that has what the link goes to
// (`followPrLink`); one the board doesn't list has no panel here and opens in the browser, as
// ⌘-click always does. A right-click has the PR's menu (menus.ts). A PR's chip in a session's
// panel (`PrChipCard`) has the same card, less Show in Codealong, which a click on the chip does.
// For an open PR the board lists, the card has the switch that has its primary session, listed
// first, told of what happens on it (nudges.rs). An open PR's card ends in GitHub's merge box
// (MergeBox.tsx), as its panel has it, so a session's PR merges from the session: it stays once
// the PR has merged, saying so and offering to delete the branch.
//
// It costs nothing until you hover: the board already lists the PRs in your projects' repos with
// their sessions, and each link watches only its own PR there, so a push re-renders a link only
// when its PR changed. A PR the board doesn't list (an older one, or another repo's) is asked of
// GitHub once its card shows (pr_card, which matches it to sessions as the board does), and the
// answer is kept a while for every link to it. An open PR's merge box is read as its card shows,
// and kept for it until it's updated (MergeBox.tsx).

import { useEffect, useLayoutEffect, useState, useSyncExternalStore, type ReactNode } from 'react';
import type { PrTab } from '../agentReview';
import { openUrl, prCard, setPrNudge } from '../api';
import { absolute, agoPhrase, count } from '../format';
import { cmd } from '../platform';
import { prIndex, prRefOf, prTabOf, type PrRef } from '../prLinks';
import { nudgeWords, primaryWords, reviewersWords } from '../prStatus';
import { errorText } from '../report';
import { nudgeAboutFor } from '../repoSettings';
import type { Board, Lane, Pr, PrItem, Session } from '../types';
import { HoverCard, useHoverCard, type HoverCardState } from './HoverCard';
import { MergeBox } from './MergeBox';
import { PanelSession } from './PanelSession';
import { PrLabels } from './PrLabels';
import { PrPills } from './PrPills';
import { Button, cx, Icon, PR_ICON, PR_WORD, PrChip } from './primitives';

/** What a window does with a PR link's card: showing and opening what's on it, and menus. */
export interface PrLinkActions {
  /** Shows the PR in the main window's PRs view, on `tab` when the link goes to one of its parts.
   *  Only for a PR the board lists. */
  showPr?: (pr: PrItem, tab: PrTab | null) => void;
  /** A click on a link to the PR: shows it as a click on its chip does, in place of the panel the
   *  link is in (App.tsx `showPrOf`). `showPr` where there's no such place. */
  followPr?: (pr: PrItem, tab: PrTab | null) => void;
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
   *  repo's team or your table for it says (repoSettings.ts `nudgeAboutFor`); and what becomes of
   *  its news while its primary session isn't running (nudges.rs `stopped_primaries`). */
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
type NudgeSettings = Partial<Pick<Board, 'nudgeAbout' | 'prStopped' | 'repos' | 'teamFiles'>>;

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
  host.nudges = { nudgeAbout: nudges?.nudgeAbout, prStopped: nudges?.prStopped, repos: nudges?.repos, teamFiles: nudges?.teamFiles };
  useLayoutEffect(notify, [prs, lanes, nudges?.nudgeAbout, nudges?.prStopped, nudges?.repos, nudges?.teamFiles]);
}

/** Opens a link in the browser, as this window opens one. */
function browse(url: string) {
  const openLink = host.actions?.openLink;
  if (openLink) openLink(url);
  else void openUrl(url);
}

/** A click on a link to a PR, here or in a PR's description (PrInspector.tsx): the PR in Codealong
 *  when the board lists it, on the tab with what the link goes to (`prTabOf`), else, as with
 *  ⌘ held (`browser`), the link in the browser. */
export function followPrLink(url: string, ref: PrRef, browser: boolean) {
  const listed = host.index.get(ref.key);
  const show = host.actions?.followPr ?? host.actions?.showPr;
  if (listed && show && !browser) show(listed, prTabOf(ref));
  else browse(url);
}

/** A PR's card for what shows the PR, a link or a chip: the handlers its anchor takes, and the
 *  card while it's up. Without a window that says what to do with it (a page that never called
 *  usePrLinks), there's no card. `clickShows`: a click on what it's beside shows the PR in
 *  Codealong, so the card has no button for it. */
function usePrCard(url: string, ref: PrRef, clickShows = false) {
  const hosted = host.actions !== null;
  const c = useHoverCard();

  const follow = (browser: boolean) => {
    c.close();
    followPrLink(url, ref, browser);
  };

  const card = c.open && c.anchor.current ? <PrCard card={c} prRef={ref} url={url} onBrowse={() => follow(true)} clickShows={clickShows} /> : null;
  return { hosted, anchor: c.anchor, show: c.show, hide: c.hide, close: c.close, follow, card };
}

/** `plain` keeps the link's words as written, without the PR's state before them: one in a
 *  tool's output, which shows every character as it was. */
export function PrLink({ url, pr: ref, plain, children }: { url: string; pr: PrRef; plain?: boolean; children: ReactNode }) {
  const item = useSyncExternalStore(subscribe, () => (plain ? undefined : (host.index.get(ref.key) ?? fetched.get(ref.key)?.item)));
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
          c.follow(cmd(e));
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
 *  there, so the chip needs no line of its own. A click shows it in Codealong and a double-click
 *  opens it on GitHub (PrChip), and a right-click has its menu; its card merges it. */
export function PrChipCard({ pr, onShow, onOpen, onMenu }: { pr: Pr; onShow?: () => void; onOpen?: () => void; onMenu?: () => void }) {
  const ref = prRefOf(pr.url);
  if (!ref) return <PrChip pr={pr} onShow={onShow} onOpen={onOpen} onMenu={onMenu} />;
  return <HoveredChip pr={pr} prRef={ref} onShow={onShow} onOpen={onOpen} onMenu={onMenu} />;
}

function HoveredChip({ pr, prRef, onShow, onOpen, onMenu }: { pr: Pr; prRef: PrRef; onShow?: () => void; onOpen?: () => void; onMenu?: () => void }) {
  const c = usePrCard(pr.url, prRef, !!onShow);
  return (
    <span ref={(el) => void (c.anchor.current = el)} className="ca-prchipcard" onMouseEnter={c.hosted ? c.show : undefined} onMouseLeave={c.hosted ? c.hide : undefined}>
      <PrChip
        pr={pr}
        titled={!c.hosted}
        onShow={
          onShow &&
          (() => {
            c.close();
            onShow();
          })
        }
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

interface CardProps {
  card: HoverCardState;
  prRef: PrRef;
  url: string;
  /** Open on GitHub: the link in the browser. */
  onBrowse: () => void;
  clickShows: boolean;
}

function PrCard({ card, prRef: ref, url, onBrowse, clickShows }: CardProps) {
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
  const reviewers = pr ? reviewersWords(pr) : null;
  const open = !!pr && pr.state !== 'merged' && pr.state !== 'closed';
  // The merge box, from when the PR shows open: kept as it merges, so what was done stays said.
  const [merges, setMerges] = useState(false);
  if (open && !merges) setMerges(true);
  // The nudge, for an open PR the board lists: shown as just set until the board says so too.
  const [wanted, setWanted] = useState<boolean | null>(null);
  const [nudgeError, setNudgeError] = useState<string | null>(null);
  const canNudge = !!listed && open;
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
    card.close();
    if (host.actions) f(host.actions);
  };
  // Its buttons: at its foot, or after the merge box's in their row, sized as those are.
  const buttons = (size: 'sm' | 'md') => (
    <>
      {listed && host.actions?.showPr && !clickShows && (
        <Button size={size} icon="layout-sidebar" onClick={() => act((a) => a.showPr?.(listed, prTabOf(ref)))}>
          Show in Codealong
        </Button>
      )}
      <Button size={size} icon="external" title={url} onClick={onBrowse}>
        Open on GitHub
      </Button>
    </>
  );

  return (
    <HoverCard card={card} label={`${ref.slug}#${ref.number}`} className="ca-prhover">
      <div className="ca-hovercard-head">
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
          <PrLabels labels={pr.labels} />
          <p className="ca-hovercard-meta">
            <code className="ca-ic">{pr.head}</code>
            {pr.base && (
              <>
                {' → '}
                <code className="ca-ic">{pr.base}</code>
              </>
            )}
          </p>
          <p className="ca-hovercard-meta">
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
          {reviewers && <p className="ca-hovercard-meta ca-prhover-reviewers">{reviewers}</p>}
          {holding.length > 0 && (
            <p className="ca-hovercard-meta">
              Checked out in <span className="ca-prhover-lanes">{holding.map((l) => l.callsign).join(', ')}</span>
            </p>
          )}
        </>
      ) : asked?.error ? (
        <p className="ca-hovercard-error">{asked.error}</p>
      ) : (
        <p className="ca-hovercard-meta">Asking GitHub…</p>
      )}
      {sessions.length > 0 && (
        <div className="ca-hovercard-sec">
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
                primary={pr && isPrimary(session) && sessions.length > 1 ? primaryWords(pr) : undefined}
              />
            ))}
          </div>
          {sessions.length > SESSIONS_SHOWN && <p className="ca-hovercard-meta">and {sessions.length - SESSIONS_SHOWN} more</p>}
        </div>
      )}
      {canNudge && listed && (
        <div className="ca-hovercard-sec">
          <button type="button" className="ca-prhover-nudge" role="switch" aria-checked={nudges} onClick={() => setNudge(listed.key, !nudges)}>
            <span>Nudge {sessions.length > 1 ? 'its primary session' : 'its session'} on updates</span>
            <span className={cx('ca-switch', nudges && 'is-on')} aria-hidden="true" />
          </button>
          <p className="ca-hovercard-meta">
            {nudgeWords(listed, nudges, primary ? { title: primary.session.title, runs: primary.session.live && !primary.lane.device, stopped: host.nudges.prStopped?.[listed.key] } : null, nudgeAboutFor(host.nudges, listed))}
          </p>
          {nudgeError && <p className="ca-hovercard-error">{nudgeError}</p>}
        </div>
      )}
      {merges && pr ? (
        <div className="ca-hovercard-sec ca-prhover-merge">
          <MergeBox key={pr.key} pr={pr} trail={buttons('md')} />
        </div>
      ) : (
        <div className="ca-hovercard-foot">{buttons('sm')}</div>
      )}
    </HoverCard>
  );
}
