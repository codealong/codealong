// The top of a PR's Reviews tab (PrInspector.tsx `PrDetails`), in its panel and its cards: your
// review of it, over everyone's (PrReviewers.tsx), under a heading of its own when theirs follow;
// a PR of yours shows here only the agent's review of it, and nothing when it has none. First where Codealong's own review of it stands (autoreview.rs, `[reviews]`), with the session
// that wrote it a click away, and Try again for one that didn't get done (review.ts `againFor`). Then your draft review on GitHub, when you have one (prDraft.ts),
// the agent's or one you began there, as it will go: its text and each comment on a line of the
// code, with the lines around it from the PR's diff, its file and line opening that line on
// GitHub. The text and each comment can be changed and a comment taken out, on GitHub at once,
// where only you see them. A verdict submits it, comments and all, as GitHub's Finish your review
// does (github.rs `review`), from the box at the foot of the PR's panel or card (PrBox.tsx). Without a draft, the review the agent wrote is read whole from reviews.json
// (`pr_agent_reviews`), so the board carries none of it: its text, then each comment on a line,
// and the reviews of earlier commits a click away. Once you've submitted a draft the agent wrote,
// it says so. A review of a PR of yours (`[reviews.own]`) never goes to GitHub: it says which
// session it went back to, or why it hasn't, and Send hands it to the session that opened the PR
// while that can take it (`send_agent_review`).

import { useEffect, useRef, useState } from 'react';
import { excerptAt, lineLink, type ExcerptLine } from '../agentReview';
import { dropDraftComment, editDraft, editDraftComment, openUrl, prAgentReviews, sendAgentReview, takeTeamSettings } from '../api';
import { absolute, ago, agoPhrase, middleTruncate } from '../format';
import { changeDraft, commentsWord, draftShown, readAgain, submittedWord, type Held } from '../prDraft';
import { splitPath } from '../prFiles';
import { errorText } from '../report';
import { offerFor, offerKey, offerWords, teamInstructions } from '../repoSettings';
import { againFor, againTitle, autoReviewWords, suggestWords, tryAgain } from '../review';
import { strings, useStored } from '../stored';
import type { AutoReview, Draft, DraftComment, Lane, LineComment, PrDetail, PrItem, Session, TeamFile, WrittenReview } from '../types';
import { Markdown } from './Markdown';
import { BoxText, Button, cx, Icon, Seg } from './primitives';
import { prIsOpen } from './PrsView';
import { cmd, keysIn } from '../platform';

/** A commit as people read it. */
const short = (sha: string) => sha.slice(0, 7);

/** How long a Remove waits for the click that does it. */
const ARMED_MS = 4000;

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
export function Excerpt({ lines }: { lines: ExcerptLine[] }) {
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

/** Where a comment is, which opens that line on GitHub as it was at `commit`. */
function Where({ pr, commit, path, line }: { pr: PrItem; commit: string; path: string; line: number | null }) {
  const { dir, name } = splitPath(path);
  const url = line != null ? lineLink(pr.repoSlug, commit, path, line) : `https://github.com/${pr.repoSlug}/blob/${commit}/${path.split('/').map(encodeURIComponent).join('/')}`;
  return (
    <button type="button" className="ca-agentrev-where" title={line != null ? `Open ${path} at line ${line} on GitHub, as it was at ${short(commit)}` : `Open ${path} on GitHub, as it was at ${short(commit)}`} onClick={() => void openUrl(url)}>
      <span className="ca-agentrev-file">{name}</span>
      {dir && <span className="ca-agentrev-dir">{dir}</span>}
      <span className="ca-agentrev-line">{line != null ? `line ${line}` : 'the file'}</span>
    </button>
  );
}

/** The lines around a comment's line, from the PR's diff as it is now; `before` for a line of
 *  the file as it was. */
export function around(detail: PrDetail | undefined, path: string, line: number | null, before = false): ExcerptLine[] {
  const patch = detail?.files.find((f) => f.path === path)?.patch;
  return patch && line != null ? excerptAt(patch, line, 2, before) : [];
}

/** A comment on a line the agent wrote: where it is, the lines around it, and what it says. */
function LineNote({ comment, pr, head, detail }: { comment: LineComment; pr: PrItem; head: string; detail?: PrDetail }) {
  const lines = around(detail, comment.path, comment.line);
  return (
    <li className="ca-agentrev-comment">
      <Where pr={pr} commit={head} path={comment.path} line={comment.line} />
      {lines.length > 0 && <Excerpt lines={lines} />}
      <Markdown className="ca-agentrev-text" text={comment.body} />
    </li>
  );
}

/** Where the PR's latest round stands, with its draft or comment and its session a click away;
 *  for a PR of yours, where it went back to, and Send while it hasn't. */
function Standing({ pr, review, now, onGitHub, sessions, onShowSession, teamFiles }: { pr: PrItem; review: AutoReview; now: number; onGitHub?: { draft: boolean; submitted: string | null }; sessions: { lane: Lane; session: Session }[]; onShowSession: (lane: Lane, session: Session) => void; teamFiles?: TeamFile[] }) {
  const titleOf = (key: string | null | undefined) => sessions.find((s) => s.session.key === key)?.session.title ?? null;
  const { icon, says, note } = autoReviewWords(review, onGitHub, titleOf(review.sent));
  const rounds = `${review.rounds} ${review.rounds === 1 ? 'round' : 'rounds'} so far`;
  // Said on hover only: the team's words are in Settings for whoever wants them.
  const team = teamInstructions(teamFiles, pr) ? ` Its reviewer is told what ${pr.repoSlug}’s team asks of every review there too (.codealong/config.toml).` : '';
  const [sending, setSending] = useState(false);
  const [trying, setTrying] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const to = review.own && review.stage === 'kept' && !review.sent ? review.sendable : null;
  const send = () => {
    setSending(true);
    setError(null);
    sendAgentReview(pr.key)
      .catch((e: unknown) => setError(errorText(e)))
      .finally(() => setSending(false));
  };
  // What it does shows here as the round goes, so only a failure is said.
  const again = againFor(pr);
  const retry = () => {
    setError(null);
    const busy = <T,>(_label: string, work: Promise<T>) => {
      setTrying(true);
      return work.finally(() => setTrying(false));
    };
    tryAgain(pr, { error: setError, info: () => {}, busy });
  };
  return (
    <div className={cx('ca-prinsp-auto', `is-${review.stage}`)}>
      <div className="ca-prinsp-auto-head">
        <Icon name={icon} size={12} />
        <span className="ca-prinsp-auto-says" title={`Codealong has an agent review the pull requests that wait on your review (Settings › Automation): ${rounds}.${team}`}>
          {says}
        </span>
        <span className="ca-meta" title={absolute(review.at)}>
          {ago(review.at, now)}
        </span>
        {/* A draft read from GitHub has its own way there, or is gone. */}
        {review.suggests && !review.submitted && <span className={cx('ca-prbox-suggests', `is-${review.suggests}`)}>{suggestWords(review.suggests)}</span>}
        <Links link={review.stage === 'drafted' && onGitHub ? null : review.comment} session={review.parts?.find((p) => p.session)?.session} sessions={sessions} onShowSession={onShowSession} />
        {again && (
          <button type="button" className="ca-linkbtn" disabled={trying} title={againTitle(again)} onClick={retry}>
            {trying ? 'Trying again…' : 'Try again'}
          </button>
        )}
        {to && (
          <button type="button" className="ca-linkbtn" disabled={sending} title="Hand the review to the session that opened the pull request, as a file it reads, to fix what holds up" onClick={send}>
            {sending ? 'Sending…' : `Send to “${middleTruncate(titleOf(to) ?? 'its session', 32)}”`}
          </button>
        )}
      </div>
      {note && <p className="ca-prinsp-auto-note">{note}</p>}
      {error && <p className="ca-form-error">{error}</p>}
    </div>
  );
}

/** Text being changed: a box with Save and Cancel. ⌘⏎ saves, Escape cancels. */
function Editing({ text, label, saving, onText, onSave, onCancel }: { text: string; label: string; saving: boolean; onText: (t: string) => void; onSave: () => void; onCancel: () => void }) {
  const box = useRef<HTMLTextAreaElement>(null);
  useEffect(() => box.current?.focus(), []);
  return (
    <div className="ca-composer ca-agentrev-edit">
      <div className="ca-composer-field">
        <BoxText
          ref={box}
          value={text}
          lines={10}
          hint=""
          aria-label={label}
          spellCheck={false}
          disabled={saving}
          onChange={(e) => onText(e.target.value)}
          onKeyDown={(e) => {
            if (e.nativeEvent.isComposing) return;
            if (e.key === 'Enter' && cmd(e)) {
              e.preventDefault();
              onSave();
            } else if (e.key === 'Escape') {
              e.preventDefault();
              e.stopPropagation();
              onCancel();
            }
          }}
        />
      </div>
      <div className="ca-agentrev-actions">
        <Button size="sm" variant="primary" disabled={saving} title={keysIn('Save it on GitHub (⌘⏎)')} onClick={onSave}>
          {saving ? 'Saving…' : 'Save'}
        </Button>
        <Button size="sm" variant="ghost" disabled={saving} title="Leave it as it was (Escape)" onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </div>
  );
}

/** What's being changed in a draft: its text, or one of its comments, by id. */
type Change = { what: 'text' | string; text: string };

/** Your draft review as it will go: its text, then its comments, each of which can be changed or
 *  taken out, on GitHub at once. */
function DraftReview({ pr, draft, detail, verdicts }: { pr: PrItem; draft: Draft; detail?: PrDetail; verdicts: string }) {
  const [change, setChange] = useState<Change | null>(null);
  const [saving, setSaving] = useState(false);
  const [armed, setArmed] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const commit = draft.commit ?? pr.headOid ?? pr.head;
  useEffect(() => {
    if (!armed) return;
    const t = setTimeout(() => setArmed(null), ARMED_MS);
    return () => clearTimeout(t);
  }, [armed]);

  /** Does `act` on GitHub, shows `shown` at once once it's done, and reads the draft again. */
  const run = (act: Promise<void>, shown: (d: Draft) => Draft) => {
    setSaving(true);
    setError(null);
    act
      .then(() => {
        changeDraft(pr.key, shown);
        setChange(null);
        readAgain(pr);
      })
      .catch((e: unknown) => setError(errorText(e)))
      .finally(() => setSaving(false));
  };
  const save = () => {
    if (!change) return;
    const text = change.text.trim();
    if (change.what === 'text') run(editDraft(pr.repoSlug, pr.number, text), (d) => ({ ...d, body: text }));
    else if (!text) setError('A comment needs text: remove it instead.');
    else run(editDraftComment(change.what, text), (d) => ({ ...d, comments: d.comments.map((c) => (c.id === change.what ? { ...c, body: text } : c)) }));
  };
  const remove = (c: DraftComment) => {
    if (armed !== c.id) {
      setArmed(c.id);
      return;
    }
    setArmed(null);
    run(dropDraftComment(c.id), (d) => ({ ...d, comments: d.comments.filter((x) => x.id !== c.id), count: Math.max(0, d.count - 1) }));
  };
  const editing = (what: string) => change?.what === what;
  const edit = (what: string, text: string) => {
    setError(null);
    setChange({ what, text });
  };

  return (
    <div className="ca-agentrev-draft">
      <div className="ca-agentrev-drafthead">
        <span className="ca-agentrev-drafttitle">Your draft review{draft.count > 0 ? ` · ${commentsWord(draft.count)}` : ''}</span>
        <button type="button" className="ca-linkbtn" title="Open its files on GitHub, where your draft review is" onClick={() => void openUrl(`${pr.url}/files`)}>
          On GitHub
        </button>
      </div>
      <p className="ca-agentrev-draftnote">Only you see it. {verdicts}</p>
      {error && <p className="ca-form-error">{error}</p>}
      {editing('text') && change ? (
        <Editing text={change.text} label="Your draft review’s text" saving={saving} onText={(t) => setChange({ what: 'text', text: t })} onSave={save} onCancel={() => setChange(null)} />
      ) : (
        <div className="ca-agentrev-drafttext">
          {draft.body.trim() ? <Markdown className="ca-agentrev-text" text={draftShown(draft.body)} /> : <p className="ca-empty">No text: only its comments.</p>}
          <div className="ca-agentrev-actions">
            <button type="button" className="ca-linkbtn" disabled={saving} title="Change its text, on GitHub" onClick={() => edit('text', draft.body)}>
              Edit text
            </button>
          </div>
        </div>
      )}
      {draft.comments.length > 0 && (
        <ol className="ca-agentrev-comments">
          {draft.comments.map((c) => {
            const lines = c.outdated ? [] : around(detail, c.path, c.line);
            return (
              <li key={c.id} className="ca-agentrev-comment">
                <Where pr={pr} commit={commit} path={c.path} line={c.line} />
                {c.outdated && <span className="ca-tag" title="The PR’s changes since moved or changed its line">Outdated</span>}
                {lines.length > 0 && <Excerpt lines={lines} />}
                {editing(c.id) && change ? (
                  <Editing text={change.text} label={`Your comment on ${c.path}`} saving={saving} onText={(t) => setChange({ what: c.id, text: t })} onSave={save} onCancel={() => setChange(null)} />
                ) : (
                  <>
                    <Markdown className="ca-agentrev-text" text={c.body} />
                    <div className="ca-agentrev-actions">
                      <button type="button" className="ca-linkbtn" disabled={saving} title="Change what it says, on GitHub" onClick={() => edit(c.id, c.body)}>
                        Edit
                      </button>
                      <button type="button" className={cx('ca-linkbtn', armed === c.id && 'is-armed')} disabled={saving} title="Take it out of your draft review, on GitHub" onClick={() => remove(c)}>
                        {armed === c.id ? 'Click again to remove' : 'Remove'}
                      </button>
                    </div>
                  </>
                )}
              </li>
            );
          })}
        </ol>
      )}
      {draft.count > draft.comments.length && <p className="ca-meta">And {commentsWord(draft.count - draft.comments.length)} more, which go with it: GitHub lists them.</p>}
    </div>
  );
}

/** How many commits' reviews are a click away before the rest are asked for. */
const ROUNDS_SHOWN = 3;

/** The review the agent wrote, from reviews.json: its text and comments on lines, each earlier
 *  commit's a click away. */
function Written({ pr, review, detail, now, sessions, onShowSession }: { pr: PrItem; review: AutoReview; detail?: PrDetail; now: number; sessions: { lane: Lane; session: Session }[]; onShowSession: (lane: Lane, session: Session) => void }) {
  // Read again as a round ends or begins; what was read stays up meanwhile.
  const asked = `${pr.key}@${review.stage}:${review.at}:${review.rounds}`;
  const [read, setRead] = useState<{ key: string; reviews: WrittenReview[] }>({ key: '', reviews: [] });
  useEffect(() => {
    let live = true;
    prAgentReviews(pr.key)
      // An engine that doesn't know the request answers nothing: no reviews to show.
      .then((reviews) => live && setRead({ key: pr.key, reviews: reviews ?? [] }))
      // Without them the PR says where its review stands, which is what it had before.
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [asked, pr.key]);
  const reviews = read.key === pr.key ? read.reviews : [];
  const [picked, setPicked] = useState<string | null>(null);
  // The PR whose earlier rounds are all shown, after a click on the `+`.
  const [allOf, setAllOf] = useState<string | null>(null);
  const shown = reviews.find((w) => w.head === picked) ?? reviews[0];
  if (!shown) return null;
  const latest = shown === reviews[0];
  // The latest few, and the rest behind a `+`: a PR reviewed at every push has a commit for each.
  // The one shown stays among them when new rounds push it down.
  const rounds = allOf === pr.key || reviews.indexOf(shown) >= ROUNDS_SHOWN ? reviews : reviews.slice(0, ROUNDS_SHOWN);
  const hidden = reviews.length - rounds.length;
  return (
    <>
      {reviews.length > 1 && (
        <div className="ca-agentrev-rounds">
          <Seg label="The review of which commit" look="text" options={rounds.map((w, i) => ({ id: w.head, label: short(w.head), note: ago(w.at, now), title: `${i === 0 ? 'The latest, reviewed' : 'Reviewed'} ${agoPhrase(w.at, now)}` }))} value={shown.head} onPick={setPicked} />
          {hidden > 0 && (
            <button type="button" className="ca-agentrev-more" title={hidden === 1 ? 'The review of an earlier commit' : `The reviews of ${hidden} earlier commits`} onClick={() => setAllOf(pr.key)}>
              +{hidden}
            </button>
          )}
        </div>
      )}
      <div className="ca-agentrev-body">
        <div className="ca-agentrev-byline">
          <span>
            {latest ? `The agent’s review${shown.agent ? ` (${shown.agent})` : ''}` : `Of ${short(shown.head)}, ${agoPhrase(shown.at, now)}${shown.agent ? `, by ${shown.agent}` : ''}`}
            {shown.comments.length > 0 ? ` · ${commentsWord(shown.comments.length)} on lines` : ''}
          </span>
          {!latest && <Links link={shown.link} session={shown.session} sessions={sessions} onShowSession={onShowSession} />}
        </div>
        {shown.summary && <Markdown className="ca-agentrev-text" text={shown.summary} />}
        {shown.comments.length > 0 && (
          <ol className="ca-agentrev-comments">
            {shown.comments.map((c, i) => (
              <LineNote key={`${c.path}:${c.line}:${i}`} comment={c} pr={pr} head={shown.head} detail={detail} />
            ))}
          </ol>
        )}
      </div>
    </>
  );
}

export interface PrReviewProps {
  pr: PrItem;
  /** The PR's detail, once read: its files' diffs, for the lines around each comment. */
  detail?: PrDetail;
  now: number;
  /** Your review of it on GitHub, as read (prDraft.ts `useYourReview`). */
  yours?: Held;
  /** The PR's sessions, each with its worktree, for showing the one that reviewed it. */
  sessions: { lane: Lane; session: Session }[];
  onShowSession: (lane: Lane, session: Session) => void;
  /** The repos' team files: the Reviews tab says, once, what this PR's repo's team suggests. */
  teamFiles?: TeamFile[];
  /** Whether everyone's reviews follow it in the tab (PrReviewers.tsx); null while they're read. */
  others: boolean | null;
}

/** The team offers dismissed (repoSettings.ts `offerKey`), in every window: one comes back only
 *  when its team comes to suggest something else. */
const DISMISSED_KEY = 'codealong.teamOffers.dismissed';
const NONE_DISMISSED: string[] = [];
/** How many dismissals are kept: the latest. */
const DISMISSED_MOST = 50;

/** What a PR's repo's team suggests that you don't have, in place of what an empty Reviews tab
 *  says, with Use them, until dismissed. */
function TeamOffer({ file, onDismiss }: { file: TeamFile; onDismiss: () => void }) {
  const [taking, setTaking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const take = () => {
    setTaking(true);
    setError(null);
    takeTeamSettings(file.repo, file.branch)
      .catch((e: unknown) => setError(errorText(e)))
      .finally(() => setTaking(false));
  };
  return (
    <div className="ca-agentrev-offer" role="note">
      <Icon name="users" size={12} />
      <div className="ca-agentrev-offer-body">
        <p>
          {file.repo}’s team suggests: {offerWords(file.offers).join('; ')}.
        </p>
        <p className="ca-agentrev-offer-more">Use them to have that for this repo; Settings › Automation shows them with {file.repo} picked.</p>
        <Button size="sm" variant="primary" disabled={taking} onClick={take}>
          Use them
        </Button>
        {error && <p className="ca-form-error">{error}</p>}
      </div>
      <button type="button" className="ca-agentrev-offer-x" aria-label="Dismiss" title="Don’t say this again, unless the team comes to suggest something else" onClick={onDismiss}>
        <Icon name="x" size={12} />
      </button>
    </div>
  );
}

export function PrReview({ pr, detail, now, yours, sessions, onShowSession, teamFiles, others }: PrReviewProps) {
  const review = pr.autoReview;
  const read = yours?.review;
  const draft = read?.draft ?? null;
  const onGitHub = read ? { draft: !!draft, submitted: read.submitted?.state ?? null } : undefined;
  const open = prIsOpen(pr);
  // What a verdict does with the draft, as the box that gives one is here or below.
  const verdicts = open ? 'Approve, Comment or Request changes in the box below submits it, with its comments.' : 'The PR is closed, so it can’t be submitted.';
  const [dismissed, setDismissed] = useStored(DISMISSED_KEY, strings, NONE_DISMISSED);
  const offer = !draft && !review && !yours?.loading ? offerFor(teamFiles, pr, dismissed) : undefined;
  const dismiss = (f: TeamFile) => setDismissed((d) => [...d.filter((k) => k !== offerKey(f)), offerKey(f)].slice(-DISMISSED_MOST));
  const nothing = !offer && !draft && !review && !yours?.loading;
  // A PR of yours the agent hasn't reviewed has no review of yours to show, nor one being read:
  // the tab is everyone else's, and says nobody's there only once they're read.
  if (pr.mine && !offer && !draft && !review && others !== false && !yours?.error) return null;
  return (
    <section className="ca-insp-sec ca-agentrev" aria-label="Your review">
      {others && <h4 className="ca-label">{pr.mine ? 'The agent’s review' : 'Your review'}</h4>}
      {review && <Standing pr={pr} review={review} now={now} onGitHub={onGitHub} sessions={sessions} onShowSession={onShowSession} teamFiles={teamFiles} />}
      {read?.submitted && !draft && (
        <p className="ca-agentrev-submitted">
          <Icon name="check" size={12} />
          <span>
            You submitted the agent’s review, {submittedWord(read.submitted.state)}
            {read.submitted.at ? `, ${agoPhrase(read.submitted.at, now)}` : ''}.
          </span>
          {read.submitted.url && (
            <button type="button" className="ca-linkbtn" title="Open your review on GitHub" onClick={() => void openUrl(read.submitted?.url ?? pr.url)}>
              On GitHub
            </button>
          )}
        </p>
      )}
      {yours?.error && <p className="ca-form-error">Couldn’t read your review on GitHub: {yours.error}</p>}
      {draft ? <DraftReview pr={pr} draft={draft} detail={detail} verdicts={verdicts} /> : review && <Written pr={pr} review={review} detail={detail} now={now} sessions={sessions} onShowSession={onShowSession} />}
      {offer && <TeamOffer file={offer} onDismiss={() => dismiss(offer)} />}
      {nothing && <p className="ca-empty">{pr.mine ? 'Nobody has reviewed it or written on it yet. Settings › Automation can have an agent on another model review the PRs your sessions open, and hand its review back to the session.' : 'No draft review of yours. Settings › Automation has an agent draft one for each PR that waits on your review.'}</p>}
      {!draft && !review && yours?.loading && <p className="ca-empty">Reading your review on GitHub…</p>}
    </section>
  );
}
