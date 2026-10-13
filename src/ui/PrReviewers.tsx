// Everyone's reviews of a PR and what they wrote on it, in its Reviews tab under your own review
// (AgentReview.tsx `PrReview`), in its panel and its cards: each person once (prReviews.ts
// `reviewersOf`), with where their review stands, whether it's still asked of them, and what they
// wrote, what's still open first. A thread on the code says its file and line, has the lines
// around it from the PR's diff while it's open, and how many replies follow; a click on any
// comment's head opens it on GitHub, where its replies are. Text much longer than a review's
// usual (a bot's report) shows cut short until Show all.

import { useState } from 'react';
import { openUrl } from '../api';
import { absolute, ago } from '../format';
import { splitPath } from '../prFiles';
import { isOpen, LONG_TEXT, REVIEW_WORD, saidShown, type Reviewer } from '../prReviews';
import { commentPreview, commentText } from '../prStatus';
import type { PrComment, PrDetail } from '../types';
import { around, Excerpt } from './AgentReview';
import { Markdown } from './Markdown';
import { cx } from './primitives';

/** One thing someone wrote: its head (where it is, when, how it stands), then its words. The text
 *  of an earlier review of theirs says its verdict where that isn't the one they stand by now
 *  (`standing`), which their own line says already; else the head starts at when. */
function Said({ c, standing, detail, asks, now }: { c: PrComment; standing: string | null; detail: PrDetail; asks: Set<string>; now: number }) {
  const [whole, setWhole] = useState(false);
  const text = commentText(c.body);
  const lines = c.path && !c.resolved && !c.outdated && c.line != null ? around(detail, c.path, c.line, !!c.before) : [];
  const { dir, name } = splitPath(c.path ?? '');
  const replies = c.replies ?? 0;
  const verdict = !c.path && c.review && c.review !== standing ? (REVIEW_WORD[c.review] ?? null) : null;
  return (
    <li className={cx('ca-prsaid', isOpen(c, asks) && 'is-open', c.resolved && 'is-resolved')}>
      <button type="button" className="ca-prsaid-head" disabled={!c.url} title={c.url ? 'Open it on GitHub' : undefined} onClick={() => c.url && void openUrl(c.url)}>
        {c.path && (
          <>
            <span className="ca-agentrev-file">{name}</span>
            {dir && <span className="ca-agentrev-dir">{dir}</span>}
            <span className="ca-meta" title={c.before && c.line != null ? `Line ${c.line} of the file as it was before the PR` : undefined}>
              {c.line != null ? `line ${c.line}${c.before ? ' before' : ''}` : c.outdated ? 'a line since changed' : 'the file'}
            </span>
          </>
        )}
        {verdict && (
          <span className="ca-prsaid-kind" title="The text of an earlier review of theirs">
            {verdict}
          </span>
        )}
        {c.at ? (
          <span className="ca-prsaid-when" title={absolute(c.at)}>
            {ago(c.at, now)} ago
          </span>
        ) : (
          !c.path && !verdict && <span className="ca-prsaid-when">On GitHub</span>
        )}
        {replies > 0 && <span className="ca-meta">{replies === 1 ? '1 reply' : `${replies} replies`}</span>}
        {c.outdated && !c.resolved && <span className="ca-tag" title="The PR’s changes since moved or changed its line">Outdated</span>}
        {c.resolved && <span className="ca-tag">Resolved</span>}
        {asks.has(c.id) && <span className="ca-tag is-attn">Asks for a change</span>}
      </button>
      {lines.length > 0 && <Excerpt lines={lines} />}
      {c.resolved || (text.length > LONG_TEXT && !whole) ? (
        <p className="ca-prsaid-short">
          {commentPreview(c.body)}
          {!c.resolved && (
            <>
              {' '}
              <button type="button" className="ca-linkbtn" onClick={() => setWhole(true)}>
                Show all
              </button>
            </>
          )}
        </p>
      ) : (
        <Markdown className="ca-prsaid-text" text={text} />
      )}
    </li>
  );
}

/** Someone on the PR: who, where their review stands, then what they wrote. */
function Person({ r, detail, asks, now }: { r: Reviewer; detail: PrDetail; asks: Set<string>; now: number }) {
  const [all, setAll] = useState(false);
  const shown = saidShown(r, all);
  const verdict = r.state ? (REVIEW_WORD[r.state] ?? r.state) : null;
  return (
    <li className={cx('ca-prrev', r.state && `is-${r.state.toLowerCase()}`)}>
      <div className="ca-prrev-head">
        <span className="ca-prrev-who">{r.mine ? 'you' : r.who}</span>
        {r.author && <span className="ca-meta">author</span>}
        {verdict && <span className="ca-prrev-verdict">{verdict}</span>}
        {r.requested && <span className="ca-prrev-asked">{r.state ? 'Asked again' : 'Review requested'}</span>}
        {r.at && (
          <span className="ca-meta" title={absolute(r.at)}>
            {ago(r.at, now)} ago
          </span>
        )}
        {r.open > 0 && (
          <span className="ca-tag is-attn" title="Threads on the code not resolved, and comments that ask for a change">
            {r.open} open
          </span>
        )}
      </div>
      {shown.length > 0 && (
        <ul className="ca-prsaid-list">
          {shown.map((c) => (
            <Said key={c.id} c={c} standing={r.state} detail={detail} asks={asks} now={now} />
          ))}
        </ul>
      )}
      {shown.length < r.said.length && (
        <button type="button" className="ca-more-btn" onClick={() => setAll(true)}>
          Show {r.said.length - shown.length} more
        </button>
      )}
    </li>
  );
}

export interface PrReviewersProps {
  /** Everyone on the PR (`reviewersOf`). */
  reviewers: Reviewer[];
  /** The PR's detail: its files' diffs, for the lines around each thread. */
  detail: PrDetail;
  /** The comments Jev says ask for a change. */
  asks: Set<string>;
  /** What's still open from others than you (`openFromOthers`). */
  open: number;
  now: number;
}

export function PrReviewers({ reviewers, detail, asks, open, now }: PrReviewersProps) {
  if (reviewers.length === 0) return null;
  return (
    <section className="ca-insp-sec ca-prrevs" aria-label="Everyone’s reviews">
      <h4 className="ca-label">
        Reviews and comments
        {open > 0 && <span className="ca-prrevs-open"> · {open} open</span>}
      </h4>
      <ul className="ca-prrevs-list">
        {reviewers.map((r) => (
          <Person key={r.who} r={r} detail={detail} asks={asks} now={now} />
        ))}
      </ul>
    </section>
  );
}
