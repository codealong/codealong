// A PR in the right sidebar: a short header first, as a session's is (SessionHeader.tsx): its
// number and title with the panel's buttons, one line of where it comes from, its pills and size,
// and where it lives on this Mac as chips (the worktrees and the sessions that worked on it,
// PanelSession.tsx `PanelPlaces`). Its details, open for every PR once you open them, add the
// whole title, its times and commits, its reviews and the chips as a tree, under the merge box.
// Then its parts as tabs, one shown at a time: Description (its description as GitHub renders it,
// then its comments), Changes (its changed files with their diffs, PrFiles.tsx, which the file
// count in the header and ⌘F go to) and Review (your review: the agent's, your draft on GitHub,
// AgentReview.tsx). In PRs the panel can take the whole window (⇧⌘F, App.tsx). The
// tabs stay at the top as what's under them scrolls. A PR opens on the tab it showed last, else on
// its review when the agent's is there to read and the PR waits on yours (agentReview.ts
// `openingTab`), else its description; a window keeps which (`usePrTab`), so the line over its box
// can show the Review tab. Under the header is a row of buttons: the panel's (Approve, Snooze,
// GitHub, Review with) with the merge box's (MergeBox.tsx) after Approve, so merging is where
// approving is. `PrDetails` is that body on its own, which the panel and a PR's card (PrCard.tsx)
// each scroll over the box that reviews it (PrBox.tsx), fixed at their foot; the card adds the
// strip that works through what waits on you.
//
// The description is GitHub's own rendering (`bodyHTML`), which GitHub sanitizes; the page's
// content security policy blocks scripts on top of that. Links in it open in the browser.

import { useEffect, useId, useLayoutEffect, useRef, useState, type MouseEvent as ReactMouseEvent, type ReactNode } from 'react';
import { agentsVerdict, openingTab, type PrTab } from '../agentReview';
import { below, showContextMenu } from '../contextMenu';
import { openPrFile, openUrl, prCommentAsks, prDetail } from '../api';
import { useEditorIcons } from '../editors';
import { absolute, ago, middleTruncate } from '../format';
import { prFileMenu } from '../menus';
import { fileOnGitHub, firstChanged, parsePatch } from '../prFiles';
import { asksFor, commentPreview, commentsToAsk, mayApprove } from '../prStatus';
import { useYourReview, verdictWords } from '../prDraft';
import type { Report } from '../report';
import type { Board, Lane, PrComment, PrDetail, PrFile, PrItem, Session, Usage } from '../types';
import { PrReview } from './AgentReview';
import { PrBoxView, usePrBox } from './PrBox';
import { MergeBox, type MergeGoesOn } from './MergeBox';
import { DetailsButton, PanelPlaces, PR_PRIMARY, useDetailsOpen } from './PanelSession';
import { PrFiles, type FileOpener, type FilesFinder } from './PrFiles';
import { PrPills } from './PrPills';
import { BranchLabel, Button, cx, headerMenu, Icon, Tabs } from './primitives';
import { PrGlyph, prIsOpen, reviewSnoozed, syncWords, type PrPlace, type PrRowItem } from './PrsView';
import { Tip } from './Tip';
import { untilWords } from '../snooze';
import { errorText } from '../report';
import { clearNotice, failedNotice, reportNotice } from '../notices';

/** Whether a PR's details are open, for every PR, as you left them. */
const DETAILS_KEY = 'codealong.prDetailsOpen';

/** Details already fetched, by PR and last update, so going back to a PR is instant. */
const cache = new Map<string, PrDetail>();

/** Comments listed before "Show all". */
const COMMENTS_SHOWN = 6;

/** Jev's word on comments already asked about, by comment id, across PRs. */
const askCache = new Map<string, Record<string, number>>();

/** The tab each PR showed last, kept while the window goes from one PR to the next. */
const tabsShown = new Map<string, PrTab>();

/** Which of a PR's tabs shows, for its details and what's beside them: the one picked for it,
 *  else the one it showed last, else where it opens. `show` brings a tab into view from outside
 *  the tab bar (the line over the box), `shown` counting those. */
export interface PrTabs {
  tab: PrTab;
  pick: (to: PrTab) => void;
  show: (to: PrTab) => void;
  shown: number;
}

export function usePrTab(pr: PrItem): PrTabs {
  const [picked, setPicked] = useState<{ key: string; tab: PrTab } | null>(null);
  const [shown, setShown] = useState(0);
  const tab = picked?.key === pr.key ? picked.tab : (tabsShown.get(pr.key) ?? openingTab(pr));
  const pick = (to: PrTab) => {
    tabsShown.set(pr.key, to);
    setPicked({ key: pr.key, tab: to });
  };
  return { tab, pick, show: (to) => (pick(to), setShown((n) => n + 1)), shown };
}

/** A PR's comments, newest first, the ones asking for a change marked. */
function Comments({ comments, asks, now }: { comments: PrComment[]; asks: Set<string>; now: number }) {
  const [all, setAll] = useState(false);
  // What asks for a change comes first, so it's never below the fold.
  const ordered = [...comments].sort((a, b) => Number(asks.has(b.id)) - Number(asks.has(a.id)));
  const shown = all ? ordered : ordered.slice(0, COMMENTS_SHOWN);
  return (
    <>
      <ul className="ca-prcomments">
        {shown.map((c) => (
          <li key={c.id} className={asks.has(c.id) ? 'is-asks' : undefined}>
            <button type="button" className="ca-prcomment" title={c.url ? 'Open it on GitHub' : undefined} disabled={!c.url} onClick={() => c.url && void openUrl(c.url)}>
              <span className="ca-prcomment-head">
                <span className="ca-prinsp-reviewer">{c.mine ? 'you' : c.author}</span>
                {c.path && <code className="ca-ic">{middleTruncate(c.path, 36)}</code>}
                {c.at && <span className="ca-meta">{ago(c.at, now)}</span>}
                {c.resolved && <span className="ca-tag">Resolved</span>}
                {asks.has(c.id) && <span className="ca-tag is-attn">Asks for a change</span>}
              </span>
              <span className="ca-prcomment-body">{commentPreview(c.body)}</span>
            </button>
          </li>
        ))}
      </ul>
      {!all && comments.length > COMMENTS_SHOWN && (
        <button type="button" className="ca-more-btn" onClick={() => setAll(true)}>
          Show all {comments.length} comments
        </button>
      )}
    </>
  );
}

const REVIEW_WORD: Record<string, string> = { APPROVED: 'Approved', CHANGES_REQUESTED: 'Requested changes', COMMENTED: 'Commented', DISMISSED: 'Dismissed' };

/** What the worktree says of itself: how it stands against GitHub when it holds the PR, otherwise
 *  what it has moved on to, when that's known. */
function placeNote({ lane, holds }: PrPlace, prNumber: number, head: string): string | null {
  if (holds) return syncWords(lane.git);
  if (lane.pr && lane.pr.number !== prNumber) return `now on #${lane.pr.number}`;
  if (lane.branch && lane.branch !== head) return `now on ${lane.branch}`;
  return null;
}

/** What the details need: the PR with where it lives, and what a click on a worktree or session
 *  there does. */
export interface PrDetailsProps {
  item: PrRowItem;
  now: number;
  /** The panel's buttons, in the row under the header with the merge box's (MergeBox.tsx):
   *  Approve before them, the rest after. */
  lead?: ReactNode;
  trail?: ReactNode;
  /** The PR's menu, as its row has it: from the ⋯ button, below it, and at the pointer from a
   *  right-click on the header. A PR's card takes the right-click anywhere on it and has its own
   *  ⋯ (PrCard.tsx), so it passes none. */
  onMenu?: (at?: { x: number; y: number }) => void;
  /** Shows a session in the panel, in place of the PR until you go back. */
  onShowSession: (lane: Lane, session: Session) => void;
  /** Opens a session where it runs. */
  onOpenSession: (lane: Lane, session: Session) => void;
  onShowLane: (lane: Lane) => void;
  /** Right-click on a worktree. */
  onLaneMenu: (lane: Lane) => void;
  /** Right-click on a session. */
  onSessionMenu: (lane: Lane, session: Session) => void;
  /** ⌘F goes to the files' filter (PrFiles.tsx). */
  keys?: boolean;
  /** A window that goes on to the next PR as one is merged (MergeBox.tsx). */
  goesOn?: MergeGoesOn;
  /** Which tab shows (`usePrTab`). */
  tabs: PrTabs;
  /** In PRs: whether the panel takes the whole window, and the way to switch. */
  maximized?: boolean;
  onMaximize?: () => void;
  /** How a window says something it did didn't work: with it, a changed file opens on GitHub,
   *  and with the board's `apps` too, in your editors (`fileOpener`). */
  report?: Report;
  apps?: Board['apps'];
}

/** How the PR's changed files open outside the window: as a diff in the editor that opens one,
 *  at a line in yours where a worktree here holds the PR, and on GitHub. */
function fileOpener(item: PrRowItem, apps: Board['apps'] | undefined, icons: ReturnType<typeof useEditorIcons>, report: Report): FileOpener {
  const { pr } = item;
  const holds = item.places.find((p) => p.holds);
  const diff = apps?.diffEditor ?? null;
  const yours = holds && apps ? { id: apps.editorId, name: apps.editor } : null;
  const failed = (e: unknown) => report.error(errorText(e));
  const open = (f: PrFile, how: 'diff' | 'file', line?: number | null) => {
    if (how === 'diff' && diff) report.info(`Opening the diff of ${f.path.split('/').pop()} in ${diff.name}…`);
    openPrFile({ pr: pr.key, path: f.path, previous: f.previous, status: f.status, how, line }).catch(failed);
  };
  const github = (f: PrFile) => void fileOnGitHub(pr.url, f.path).then(openUrl).catch(failed);
  return {
    diff: diff && { name: diff.name, icon: icons[diff.id]?.url },
    file: yours && { name: yours.name, icon: icons[yours.id]?.url },
    after: holds ? `your copy in ${holds.lane.callsign}` : 'the PR’s',
    open,
    github,
    menu: (f) => {
      const line = f.patch ? firstChanged(parsePatch(f.patch)) : null;
      const spec = prFileMenu(f.path, {
        report,
        diff: diff ? { editor: diff.name, icon: icons[diff.id]?.menu, run: () => open(f, 'diff') } : undefined,
        file: yours ? { editor: yours.name, icon: icons[yours.id]?.menu, line, run: () => open(f, 'file', line) } : undefined,
        github: () => github(f),
      });
      void showContextMenu(spec).catch(failed);
    },
  };
}

interface PrInspectorProps extends Omit<PrDetailsProps, 'lead' | 'trail' | 'tabs'> {
  /** How the box says a verdict went. */
  report: Report;
  usage: Usage[];
  approving: boolean;
  onApprove: () => void;
  /** Drops Review with… from the button. */
  onReview: (button: Element) => void;
  /** Drops Snooze review's times from the button. */
  onSnooze: (button: Element) => void;
}

export function PrInspector({ approving, onApprove, onReview, onSnooze, report, usage, ...details }: PrInspectorProps) {
  const { pr } = details.item;
  const tabs = usePrTab(pr);
  const box = usePrBox(pr, { report, usage, now: details.now });
  const requested = pr.reviewRequested && prIsOpen(pr);
  // What's read of your draft by the details below: approving submits it.
  const draft = useYourReview(pr, false)?.review?.draft;
  // Approve is for any open PR but yours, asked for or not; it's filled when it's asked of you,
  // unless the agent suggests otherwise and its review goes from the box in one press.
  const lead = mayApprove(pr) && (
    <Button variant={requested && !agentsVerdict(pr, !!draft) ? 'primary' : 'secondary'} icon="check" disabled={approving} title={verdictWords('approve', draft) ?? (requested ? undefined : 'Approve it as you, though your review wasn’t asked for')} onClick={onApprove}>
      {approving ? 'Approving…' : 'Approve'}
    </Button>
  );
  const trail = (
    <>
      {requested && (
        <Button icon="zzz" title="Out of Review requested and off the floating bar until the time you pick" onClick={(e) => onSnooze(e.currentTarget)}>
          Snooze…
        </Button>
      )}
      <Button icon="external" onClick={() => void openUrl(pr.url)}>
        GitHub
      </Button>
      <Button icon="eye" onClick={(e) => onReview(e.currentTarget)}>
        Review with…
      </Button>
    </>
  );
  return (
    <aside className="ca-insp ca-prpanel" aria-label={`#${pr.number} details`}>
      <div className="ca-prinsp ca-prpanel-body">
        <PrDetails {...details} report={report} tabs={tabs} lead={lead} trail={trail} />
      </div>
      <PrBoxView pr={pr} box={box} keys={{ approve: '⌘⏎', comment: '⌥⌘⏎' }} ownKeys onShowReview={() => tabs.show('review')} />
    </aside>
  );
}

/** The PR itself: its header with where it lives on this Mac, the merge box, its details when
 *  they're open, then its tabs: its description and comments, its files, your review. */
export function PrDetails({ item, now, lead, trail, onMenu, onShowSession, onOpenSession, onShowLane, onLaneMenu, onSessionMenu, keys, goesOn, tabs, maximized, onMaximize, report, apps }: PrDetailsProps) {
  const { pr } = item;
  // Its details, open or not as you left them for every PR.
  const [open, toggleOpen] = useDetailsOpen(DETAILS_KEY);
  const icons = useEditorIcons(apps);
  const opener = report && fileOpener(item, apps, icons, report);
  const files = useRef<FilesFinder>(null);
  const idBase = `prtab${useId().replace(/:/g, '')}`;

  const { tab } = tabs;
  const bar = useRef<HTMLDivElement>(null);
  const panels = useRef<Record<PrTab, HTMLDivElement | null>>({ description: null, changes: null, review: null });
  // A tab picked while the tabs are stuck at the top shows from its start, under them.
  const fromTop = useRef(false);
  const pick = (to: PrTab) => {
    const at = bar.current?.getBoundingClientRect();
    const view = bar.current?.parentElement?.getBoundingClientRect();
    fromTop.current = !!at && !!view && at.top - view.top < 2 && (bar.current?.parentElement?.scrollTop ?? 0) > 0;
    tabs.pick(to);
  };
  useLayoutEffect(() => {
    if (!fromTop.current) return;
    fromTop.current = false;
    panels.current[tab]?.scrollIntoView({ block: 'start' });
  }, [tab]);
  // A tab shown from outside the bar (the line over the box) comes into view from its start.
  useLayoutEffect(() => {
    if (tabs.shown > 0) panels.current[tab]?.scrollIntoView({ block: 'start' });
    // Only as it's asked for: the tab changing on its own moves nothing.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tabs.shown]);
  // Your review on GitHub, read while its tab shows, and for a draft the agent made.
  const yours = useYourReview(pr, tab === 'review' || pr.autoReview?.stage === 'drafted');
  const draft = yours?.review?.draft;
  const cacheKey = `${pr.key}@${pr.updatedAt}`;
  const [state, setState] = useState<{ key: string; detail?: PrDetail; error?: string }>({ key: '' });

  useEffect(() => {
    const hit = cache.get(cacheKey);
    if (hit) {
      setState({ key: cacheKey, detail: hit });
      return;
    }
    let live = true;
    setState({ key: cacheKey });
    prDetail(pr.repoSlug, pr.number)
      .then((d) => {
        cache.set(cacheKey, d);
        if (live) setState({ key: cacheKey, detail: d });
      })
      .catch((e: unknown) => {
        if (live) setState({ key: cacheKey, error: errorText(e) });
      });
    return () => {
      live = false;
    };
  }, [cacheKey, pr.repoSlug, pr.number]);

  const detail = state.key === cacheKey ? state.detail : undefined;

  // Once the comments are showing, Jev says which ask for a change (when that's on); the markers
  // arrive after, so the panel never waits on it.
  const [asks, setAsks] = useState<{ key: string; asks: Record<string, number> }>({ key: '', asks: {} });
  useEffect(() => {
    if (!detail?.comments) return;
    const hit = askCache.get(cacheKey);
    if (hit) {
      setAsks({ key: cacheKey, asks: hit });
      return;
    }
    let live = true;
    const ask = commentsToAsk(detail.comments).map((c) => ({ id: c.id, author: c.author, body: c.body, path: c.path }));
    prCommentAsks(pr.title, ask)
      .then((a) => {
        askCache.set(cacheKey, a);
        clearNotice('jev-pr-comments');
        if (live) setAsks({ key: cacheKey, asks: a });
      })
      .catch((e: unknown) => reportNotice(failedNotice('jev-pr-comments', 'Jev can’t read PR comments', `Jev couldn’t say which comments on “${pr.title}” ask for something`, e)));
    return () => {
      live = false;
    };
  }, [detail, cacheKey, pr.title]);
  const comments = detail?.comments ?? [];
  const asking = asksFor(comments, asks.key === cacheKey ? asks.asks : {});
  const error = state.key === cacheKey ? state.error : undefined;
  const requested = pr.reviewRequested && prIsOpen(pr);

  // Where it lives on this Mac: chips under the header, the tree with its details.
  const places = item.places.map((p) => ({ lane: p.lane, sessions: p.sessions, note: placeNote(p, pr.number, pr.head) }));
  const placesProps = { places, label: 'On this Mac', now, onShowLane, onLaneMenu, onShowSession, onOpenSession, onSessionMenu, primary: pr.primary ? { key: pr.primary, means: PR_PRIMARY } : null };

  // Links in the description open in the browser rather than inside the app.
  const onBodyClick = (e: ReactMouseEvent<HTMLDivElement>) => {
    const a = (e.target as Element).closest('a');
    if (!a) return;
    e.preventDefault();
    const href = a.getAttribute('href') ?? '';
    if (/^https?:\/\//.test(href)) void openUrl(href);
  };

  return (
    <>
      <header className={cx('ca-prinsp-head', open && 'is-open')} onContextMenu={headerMenu(onMenu && (() => onMenu()))}>
        <div className="ca-prinsp-top">
          <PrGlyph pr={pr} />
          <span className="ca-insp-call">#{pr.number}</span>
          <h3 className="ca-prinsp-title" title={open ? undefined : pr.title}>
            {pr.title}
          </h3>
          <div className="ca-prinsp-tools">
            {onMaximize && (
              <Button
                variant="ghost"
                size="sm"
                icon={maximized ? 'minimize' : 'maximize'}
                aria-label={maximized ? 'Show the list' : 'Expand to the window'}
                aria-pressed={maximized}
                title={maximized ? 'Show the list again (⇧⌘F)' : 'Expand to the whole window (⇧⌘F)'}
                onClick={onMaximize}
              />
            )}
            <DetailsButton open={open} says="when it was opened, its commits and reviews, and what each session on it is doing" onToggle={toggleOpen} />
            {onMenu && <Button variant="ghost" size="sm" icon="dots" aria-label="More" aria-haspopup="menu" title="Review with, approve, snooze, pin, copy and more" onClick={(e) => onMenu(below(e.currentTarget))} />}
          </div>
        </div>
        <div className="ca-prinsp-meta">
          <span title={pr.repoSlug}>{pr.repo}</span>
          <span aria-hidden="true">·</span>
          <BranchLabel branch={pr.head} />
          {pr.base && (
            <>
              <span aria-hidden="true">→</span>
              <span className="ca-prinsp-base" title={`Into ${pr.base}`}>
                {pr.base}
              </span>
            </>
          )}
          <span aria-hidden="true">·</span>
          <span>by {pr.mine ? 'you' : (pr.author ?? 'unknown')}</span>
          <span aria-hidden="true">·</span>
          <span title={`Opened ${absolute(pr.createdAt)}, updated ${absolute(pr.updatedAt)}`}>updated {ago(pr.updatedAt, now)} ago</span>
        </div>
        <div className="ca-prinsp-status">
          {reviewSnoozed(pr) ? (
            <span className="ca-tag" title="Out of Review requested and off the floating bar until then">
              <Icon name="zzz" size={12} />
              Review snoozed until {untilWords(pr.snoozedUntil, now)}
            </span>
          ) : (
            requested && <span className="ca-tag is-attn">Review requested</span>
          )}
          <PrPills pr={pr} />
          {/* The pills say a failing check over the ones still running: these say they run. */}
          {pr.checks.pending > 0 && pr.checks.failed > 0 && <span className="ca-meta">{pr.checks.pending} running</span>}
          <span className="ca-meta ca-prinsp-size">
            +{pr.additions.toLocaleString('en-US')} −{pr.deletions.toLocaleString('en-US')} in{' '}
            {detail ? (
              <button type="button" className="ca-linkbtn ca-prinsp-files" title="Go to the files, under Changes" onClick={() => files.current?.find()}>
                {pr.changedFiles} {pr.changedFiles === 1 ? 'file' : 'files'}
              </button>
            ) : (
              `${pr.changedFiles} ${pr.changedFiles === 1 ? 'file' : 'files'}`
            )}
          </span>
        </div>
        {pr.checks.failing.length > 0 && (
          <div className="ca-prcard-failing">
            {pr.checks.failing.slice(0, 8).map((name) => (
              <code key={name} className="ca-ic">
                {name}
              </code>
            ))}
          </div>
        )}
        {places.length > 0 && !open && <PanelPlaces {...placesProps} open={false} />}
      </header>
      <MergeBox key={pr.key} pr={pr} lead={lead} trail={trail} goesOn={goesOn} />

      {open && (
        <dl className="ca-kv ca-prinsp-kv">
          <dt>Repo</dt>
          <dd>{pr.repoSlug}</dd>
          <dt>Branch</dt>
          <dd className="ca-code">
            {pr.head}
            {pr.base ? ` → ${pr.base}` : ''}
          </dd>
          <dt>Author</dt>
          <dd>{pr.mine ? 'you' : (pr.author ?? 'unknown')}</dd>
          <dt>Opened</dt>
          <dd title={absolute(pr.createdAt)}>{ago(pr.createdAt, now)} ago</dd>
          <dt>Updated</dt>
          <dd title={absolute(pr.updatedAt)}>{ago(pr.updatedAt, now)} ago</dd>
          {detail && (
            <>
              <dt>Commits</dt>
              <dd>{detail.commits}</dd>
            </>
          )}
        </dl>
      )}

      {open && detail && (detail.reviews.length > 0 || detail.requested.length > 0) && (
        <section className="ca-insp-sec">
          <h4 className="ca-label">Reviews</h4>
          <ul className="ca-prinsp-reviews">
            {detail.reviews.map((r) => (
              <li key={r.author} className={`is-${r.state.toLowerCase()}`}>
                <span className="ca-prinsp-reviewer">{r.author}</span>
                <span>{REVIEW_WORD[r.state] ?? r.state}</span>
                {r.at && <span className="ca-meta">{ago(r.at, now)}</span>}
              </li>
            ))}
            {detail.requested.map((who) => (
              <li key={`req-${who}`} className="is-requested">
                <span className="ca-prinsp-reviewer">{who}</span>
                <span>Review requested</span>
              </li>
            ))}
          </ul>
        </section>
      )}

      {open && places.length > 0 && (
        <section className="ca-insp-sec">
          <h4 className="ca-label">On this Mac</h4>
          <PanelPlaces {...placesProps} open />
        </section>
      )}

      <div ref={bar} className="ca-prtabs">
        <Tabs
          label={`#${pr.number}`}
          idBase={idBase}
          value={tab}
          onPick={pick}
          tabs={[
            { id: 'description', label: 'Description', title: comments.length > 0 ? `Its description, then its ${comments.length === 1 ? 'comment' : `${comments.length} comments`}` : 'Its description' },
            { id: 'changes', label: 'Changes', count: pr.changedFiles, title: 'The files it changes, with their diffs (⌘F filters them)' },
            {
              id: 'review',
              label: 'Review',
              count: draft ? draft.count : null,
              busy: pr.autoReview?.stage === 'reviewing',
              title: draft ? 'Your draft review, to submit' : pr.autoReview?.stage === 'reviewing' ? 'An agent is reviewing it' : 'Your review of it',
            },
          ]}
        />
      </div>

      <div ref={(el) => void (panels.current.description = el)} className="ca-prtab-panel" role="tabpanel" id={`${idBase}-description`} aria-labelledby={`${idBase}-description-tab`} hidden={tab !== 'description'}>
        <section className="ca-insp-sec" aria-label="Description">
          {error && <p className="ca-form-error">{error}</p>}
          {!detail && !error && <p className="ca-empty">Loading…</p>}
          {!detail && !error && <Tip place="pr" />}
          {detail &&
            (detail.bodyHtml.trim() ? (
              <div className="ca-prbody" onClick={onBodyClick} dangerouslySetInnerHTML={{ __html: detail.bodyHtml }} />
            ) : (
              <p className="ca-empty">No description.</p>
            ))}
        </section>
        {comments.length > 0 && (
          <section className="ca-insp-sec">
            <h4 className="ca-label">
              Comments · {comments.length}
              {asking.size > 0 && <span className="ca-prcomments-asks"> · {asking.size} {asking.size === 1 ? 'asks' : 'ask'} for a change</span>}
            </h4>
            <Comments comments={comments} asks={asking} now={now} />
          </section>
        )}
      </div>

      <div ref={(el) => void (panels.current.changes = el)} className="ca-prtab-panel" role="tabpanel" id={`${idBase}-changes`} aria-labelledby={`${idBase}-changes-tab`} hidden={tab !== 'changes'}>
        {detail ? (
          <PrFiles key={pr.key} ref={files} prKey={pr.key} files={detail.files} total={pr.changedFiles} more={detail.moreFiles} keys={keys} onShow={() => pick('changes')} opener={opener} />
        ) : (
          <section className="ca-insp-sec">{error ? <p className="ca-form-error">{error}</p> : <p className="ca-empty">Loading…</p>}</section>
        )}
      </div>

      <div ref={(el) => void (panels.current.review = el)} className="ca-prtab-panel" role="tabpanel" id={`${idBase}-review`} aria-labelledby={`${idBase}-review-tab`} hidden={tab !== 'review'}>
        <PrReview pr={pr} detail={detail} now={now} yours={yours} sessions={item.sessions} onShowSession={onShowSession} />
      </div>
    </>
  );
}
