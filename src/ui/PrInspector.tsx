// A PR in the right sidebar: where it lives on this Mac first (worktrees and the sessions that
// worked on it, drawn like the sidebar's tree), then where it stands, its merge box (MergeBox.tsx),
// its comments, its description as GitHub renders it, and its changed files with their diffs
// (PrFiles.tsx), which the file count under Checks and reviews goes to. `PrDetails` is that body
// on its own, which the panel wraps with its buttons, and a PR's card (PrCard.tsx) with the strip
// that works through what waits on you and the box that reviews it.
//
// The description is GitHub's own rendering (`bodyHTML`), which GitHub sanitizes; the page's
// content security policy blocks scripts on top of that. Links in it open in the browser.

import { useEffect, useRef, useState, type MouseEvent as ReactMouseEvent, type ReactNode } from 'react';
import { openUrl, prCommentAsks, prDetail } from '../api';
import { absolute, ago, middleTruncate } from '../format';
import { asksFor, commentPreview, commentsToAsk } from '../prStatus';
import type { Lane, PrComment, PrDetail, Session } from '../types';
import { MergeBox } from './MergeBox';
import { PanelPlace } from './PanelSession';
import { PrFiles, type FilesFinder } from './PrFiles';
import { PrPills } from './PrPills';
import { Button, headerMenu, Icon } from './primitives';
import { ChecksSummary, PrGlyph, prIsOpen, reviewSnoozed, syncWords, type PrPlace, type PrRowItem } from './PrsView';
import { untilWords } from '../snooze';
import { errorText } from '../report';
import { clearNotice, failedNotice, reportNotice } from '../notices';

/** Details already fetched, by PR and last update, so going back to a PR is instant. */
const cache = new Map<string, PrDetail>();

/** Comments listed before "Show all". */
const COMMENTS_SHOWN = 6;

/** Jev's word on comments already asked about, by comment id, across PRs. */
const askCache = new Map<string, Record<string, number>>();

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

/** What the worktree says next to its name: how it stands against GitHub when it holds the PR,
 *  otherwise what it has moved on to, when that's known. */
function PlaceNote({ place: { lane, holds }, prNumber, head }: { place: PrPlace; prNumber: number; head: string }) {
  if (holds) {
    const words = syncWords(lane.git);
    return words ? <span className="ca-tree-note">{words}</span> : null;
  }
  if (lane.pr && lane.pr.number !== prNumber) return <span className="ca-tree-note">now on #{lane.pr.number}</span>;
  if (lane.branch && lane.branch !== head) {
    return (
      <span className="ca-tree-note">
        now on <code className="ca-ic">{lane.branch}</code>
      </span>
    );
  }
  return null;
}

/** What the details need: the PR with where it lives, and what a click on a worktree or session
 *  there does. */
export interface PrDetailsProps {
  item: PrRowItem;
  now: number;
  /** The panel's buttons, under the header. */
  actions?: ReactNode;
  /** Right-click on its header: the PR's menu, as its row has it. A PR's card takes the
   *  right-click anywhere on it (PrCard.tsx), so it passes none. */
  onMenu?: () => void;
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
}

interface PrInspectorProps extends Omit<PrDetailsProps, 'actions'> {
  approving: boolean;
  onApprove: () => void;
  /** Drops Review with… from the button. */
  onReview: (button: Element) => void;
  /** Drops Snooze review's times from the button. */
  onSnooze: (button: Element) => void;
}

export function PrInspector({ approving, onApprove, onReview, onSnooze, ...details }: PrInspectorProps) {
  const { pr } = details.item;
  const requested = pr.reviewRequested && prIsOpen(pr);
  const actions = (
    <div className="ca-insp-actions">
      {requested && (
        <Button variant="primary" icon="check" disabled={approving} onClick={onApprove}>
          {approving ? 'Approving…' : 'Approve'}
        </Button>
      )}
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
    </div>
  );
  return (
    <aside className="ca-insp ca-prinsp" aria-label={`#${pr.number} details`}>
      <PrDetails {...details} actions={actions} />
    </aside>
  );
}

/** The PR itself: its header, then where it lives on this Mac, its checks and reviews, its
 *  comments, its description and its files. */
export function PrDetails({ item, now, actions, onMenu, onShowSession, onOpenSession, onShowLane, onLaneMenu, onSessionMenu, keys }: PrDetailsProps) {
  const { pr } = item;
  const files = useRef<FilesFinder>(null);
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
      <header onContextMenu={headerMenu(onMenu)}>
        <div className="ca-insp-title">
          <PrGlyph pr={pr} />
          <span className="ca-insp-call">#{pr.number}</span>
          <span className="ca-prinsp-status">
            {reviewSnoozed(pr) ? (
              <span className="ca-tag" title="Out of Review requested and off the floating bar until then">
                <Icon name="zzz" size={12} />
                Review snoozed until {untilWords(pr.snoozedUntil, now)}
              </span>
            ) : (
              requested && <span className="ca-tag is-attn">Review requested</span>
            )}
            <PrPills pr={pr} />
          </span>
        </div>
        <h3 className="ca-prinsp-title">{pr.title}</h3>
        <div className="ca-prinsp-meta">
          <span>{pr.repoSlug}</span>
          <span>
            <code className="ca-ic">{pr.head}</code>
            {pr.base && (
              <>
                {' → '}
                <code className="ca-ic">{pr.base}</code>
              </>
            )}
          </span>
          <span>
            by {pr.mine ? 'you' : (pr.author ?? 'unknown')} · opened <span title={absolute(pr.createdAt)}>{ago(pr.createdAt, now)}</span> ago · updated{' '}
            <span title={absolute(pr.updatedAt)}>{ago(pr.updatedAt, now)}</span> ago
          </span>
        </div>
      </header>
      {actions}

      {item.places.length > 0 && (
        <section className="ca-insp-sec">
          <h4 className="ca-label">On this Mac</h4>
          <div className="ca-tree" role="tree" aria-label="On this Mac">
            {item.places.map((place) => (
              <PanelPlace
                key={place.lane.id}
                lane={place.lane}
                sessions={place.sessions}
                note={<PlaceNote place={place} prNumber={pr.number} head={pr.head} />}
                now={now}
                onShowLane={onShowLane}
                onLaneMenu={onLaneMenu}
                onShowSession={onShowSession}
                onOpenSession={onOpenSession}
                onSessionMenu={onSessionMenu}
              />
            ))}
          </div>
        </section>
      )}

      <section className="ca-insp-sec">
        <h4 className="ca-label">Checks and reviews</h4>
        <div className="ca-prinsp-checks">
          <ChecksSummary checks={pr.checks} />
          {pr.checks.pending > 0 && pr.checks.failed > 0 && <span className="ca-meta">{pr.checks.pending} running</span>}
          <span className="ca-meta">
            +{pr.additions.toLocaleString('en-US')} −{pr.deletions.toLocaleString('en-US')} in{' '}
            {detail ? (
              <button type="button" className="ca-linkbtn ca-prinsp-files" title="Go to the files" onClick={() => files.current?.find()}>
                {pr.changedFiles} {pr.changedFiles === 1 ? 'file' : 'files'}
              </button>
            ) : (
              `${pr.changedFiles} ${pr.changedFiles === 1 ? 'file' : 'files'}`
            )}
            {detail ? ` · ${detail.commits} ${detail.commits === 1 ? 'commit' : 'commits'}` : ''}
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
        {detail && (detail.reviews.length > 0 || detail.requested.length > 0) && (
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
        )}
      </section>

      <MergeBox key={pr.key} pr={pr} />

      {comments.length > 0 && (
        <section className="ca-insp-sec">
          <h4 className="ca-label">
            Comments · {comments.length}
            {asking.size > 0 && <span className="ca-prcomments-asks"> · {asking.size} {asking.size === 1 ? 'asks' : 'ask'} for a change</span>}
          </h4>
          <Comments comments={comments} asks={asking} now={now} />
        </section>
      )}

      <section className="ca-insp-sec">
        <h4 className="ca-label">Description</h4>
        {error && <p className="ca-form-error">{error}</p>}
        {!detail && !error && <p className="ca-empty">Loading…</p>}
        {detail &&
          (detail.bodyHtml.trim() ? (
            <div className="ca-prbody" onClick={onBodyClick} dangerouslySetInnerHTML={{ __html: detail.bodyHtml }} />
          ) : (
            <p className="ca-empty">No description.</p>
          ))}
      </section>

      {detail && <PrFiles key={pr.key} ref={files} prKey={pr.key} files={detail.files} total={pr.changedFiles} more={detail.moreFiles} keys={keys} />}
    </>
  );
}
