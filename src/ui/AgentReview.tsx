// Codealong's own review of a PR (autoreview.rs, `[reviews]`), as the PR's details show it in its
// panel and its cards (PrInspector.tsx `PrDetails`): where its latest round stands, then the
// review its agent wrote, read whole from reviews.json when the PR shows (`pr_agent_reviews`), so
// the board carries none of it. The review's text, then each comment on a line of the code with
// the lines around it from the PR's diff, its file and line opening that line on GitHub at the
// commit it reviewed. The session that wrote it and where it went on GitHub (your draft, or the
// comment) are a click away, and the reviews of earlier commits too.

import { useEffect, useState } from 'react';
import { excerptAt, lineLink, type ExcerptLine } from '../agentReview';
import { openUrl, prAgentReviews } from '../api';
import { absolute, ago } from '../format';
import { splitPath } from '../prFiles';
import { autoReviewWords } from '../review';
import type { AutoReview, Lane, LineComment, PrDetail, PrItem, Session, WrittenReview } from '../types';
import { Markdown } from './Markdown';
import { cx, Icon, Seg } from './primitives';

/** A commit as people read it. */
const short = (sha: string) => sha.slice(0, 7);

/** The way to a review's session and to where it went on GitHub. */
function Links({ link, session, sessions, onShowSession }: { link: string | null | undefined; session: string | null | undefined; sessions: { lane: Lane; session: Session }[]; onShowSession: (lane: Lane, session: Session) => void }) {
  const held = sessions.find((s) => s.session.key === session);
  const draft = link?.endsWith('/files');
  return (
    <>
      {link && (
        <button type="button" className="ca-linkbtn" title={draft ? 'Open its files on GitHub, where your draft review is' : 'Open the review on GitHub'} onClick={() => void openUrl(link)}>
          {draft ? 'Draft' : 'Comment'}
        </button>
      )}
      {held && (
        <button type="button" className="ca-linkbtn" title="Show the session that reviewed it" onClick={() => onShowSession(held.lane, held.session)}>
          Session
        </button>
      )}
    </>
  );
}

/** A few lines of the diff around a comment's line, the line marked. */
function Excerpt({ lines }: { lines: ExcerptLine[] }) {
  return (
    <pre className="ca-patch ca-agentrev-code">
      {lines.map((l, i) => (
        <span key={i} className={cx(l.kind === 'add' && 'is-add', l.kind === 'del' && 'is-del', l.at && 'is-at')}>
          <span className="ca-agentrev-n">{l.n ?? ''}</span>
          {l.kind === 'add' ? '+' : l.kind === 'del' ? '-' : ' '}
          {l.text}
          {'\n'}
        </span>
      ))}
    </pre>
  );
}

/** A comment on a line: where it is, which opens it on GitHub, the lines around it, and what it says. */
function LineNote({ comment, pr, head, detail }: { comment: LineComment; pr: PrItem; head: string; detail?: PrDetail }) {
  const { dir, name } = splitPath(comment.path);
  const patch = detail?.files.find((f) => f.path === comment.path)?.patch;
  const lines = patch ? excerptAt(patch, comment.line) : [];
  return (
    <li className="ca-agentrev-comment">
      <button type="button" className="ca-agentrev-where" title={`Open ${comment.path} at line ${comment.line} on GitHub, as it was at ${short(head)}`} onClick={() => void openUrl(lineLink(pr.repoSlug, head, comment.path, comment.line))}>
        <span className="ca-agentrev-file">{name}</span>
        {dir && <span className="ca-agentrev-dir">{dir}</span>}
        <span className="ca-agentrev-line">line {comment.line}</span>
      </button>
      {lines.length > 0 && <Excerpt lines={lines} />}
      <Markdown className="ca-agentrev-text" text={comment.body} />
    </li>
  );
}

/** Where the PR's latest round stands, with its draft or comment and its session a click away. */
function Standing({ review, now, sessions, onShowSession }: { review: AutoReview; now: number; sessions: { lane: Lane; session: Session }[]; onShowSession: (lane: Lane, session: Session) => void }) {
  const { icon, says, note } = autoReviewWords(review);
  const rounds = `${review.rounds} ${review.rounds === 1 ? 'round' : 'rounds'} so far`;
  return (
    <div className={cx('ca-prinsp-auto', `is-${review.stage}`)}>
      <div className="ca-prinsp-auto-head">
        <Icon name={icon} size={12} />
        <span className="ca-prinsp-auto-says" title={`Codealong has an agent review the pull requests that wait on your review (Settings › Automation): ${rounds}.`}>
          {says}
        </span>
        <span className="ca-meta" title={absolute(review.at)}>
          {ago(review.at, now)}
        </span>
        <Links link={review.comment} session={review.parts?.find((p) => p.session)?.session} sessions={sessions} onShowSession={onShowSession} />
      </div>
      {note && <p className="ca-prinsp-auto-note">{note}</p>}
    </div>
  );
}

export interface AgentReviewProps {
  pr: PrItem & { autoReview: AutoReview };
  /** The PR's detail, once read: its files' diffs, for the lines around each comment. */
  detail?: PrDetail;
  now: number;
  /** The PR's sessions, each with its worktree, for showing the one that reviewed it. */
  sessions: { lane: Lane; session: Session }[];
  onShowSession: (lane: Lane, session: Session) => void;
}

export function AgentReview({ pr, detail, now, sessions, onShowSession }: AgentReviewProps) {
  const review = pr.autoReview;
  // Read again as a round ends or begins; what was read stays up meanwhile.
  const asked = `${pr.key}@${review.stage}:${review.at}:${review.rounds}`;
  const [written, setWritten] = useState<{ key: string; reviews: WrittenReview[] }>({ key: '', reviews: [] });
  useEffect(() => {
    let live = true;
    prAgentReviews(pr.key)
      // An engine that doesn't know the request answers nothing: no reviews to show.
      .then((reviews) => live && setWritten({ key: pr.key, reviews: reviews ?? [] }))
      // Without them the PR says where its review stands, which is what it had before.
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [asked, pr.key]);
  const reviews = written.key === pr.key ? written.reviews : [];
  const [picked, setPicked] = useState<string | null>(null);
  const shown = reviews.find((w) => w.head === picked) ?? reviews[0];
  const latest = shown && shown === reviews[0];
  const count = shown?.comments.length ?? 0;
  return (
    <section className="ca-insp-sec ca-agentrev">
      <h4 className="ca-label">Agent review{count > 0 ? ` · ${count} ${count === 1 ? 'comment' : 'comments'} on lines` : ''}</h4>
      <Standing review={review} now={now} sessions={sessions} onShowSession={onShowSession} />
      {reviews.length > 1 && <Seg className="ca-agentrev-rounds" label="The review of which commit" look="text" options={reviews.map((w, i) => ({ id: w.head, label: i === 0 ? `${short(w.head)}, the latest` : short(w.head) }))} value={shown?.head ?? null} onPick={setPicked} />}
      {shown && (
        <div className="ca-agentrev-body">
          {!latest && (
            <div className="ca-agentrev-byline">
              <span>
                Of {short(shown.head)}, {ago(shown.at, now)} ago{shown.agent ? `, by ${shown.agent}` : ''}
              </span>
              <Links link={shown.link} session={shown.session} sessions={sessions} onShowSession={onShowSession} />
            </div>
          )}
          {shown.summary && <Markdown className="ca-agentrev-text" text={shown.summary} />}
          {shown.comments.length > 0 && (
            <ol className="ca-agentrev-comments">
              {shown.comments.map((c, i) => (
                <LineNote key={`${c.path}:${c.line}:${i}`} comment={c} pr={pr} head={shown.head} detail={detail} />
              ))}
            </ol>
          )}
        </div>
      )}
    </section>
  );
}
