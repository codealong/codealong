// A PR's box, at the foot of its card (PrCard.tsx) and of its panel (PrInspector.tsx), fixed there
// while its details scroll: the one component both draw, with what it holds kept by `usePrBox` so
// a window's own keys can give its verdicts. Over it, always, a line saying where the PR's review
// stands (an agent's round, what the agent suggests, your draft review on GitHub), which a click
// takes to the PR's Reviews tab.
//
// The box has two sides, Review and Ask, a switch at its foot, each with a draft of its own kept
// per PR while the window goes from one PR to the next. Review's verdicts are Request changes,
// Comment and Approve, the one the agent suggests (`AutoReview.suggests`) marked so; a comment or
// a request for changes needs something to say, which your draft review on GitHub can hold, and a
// verdict on a PR you have a draft of submits it, comments and all, what's written going before
// its text (prDraft.ts). When the agent suggests asking for changes or commenting, and its review
// of the commit the PR's at waits to be submitted, that verdict is the filled button and submits
// the agent's review in one press, with nothing written needed (agentReview.ts `agentsVerdict`,
// autoreview.rs `submit_now`): your draft review, else the review itself or a link to it as
// posted, what you wrote going first. Approve steps back beside it, and ⌘⏎ still approves. Ask puts questions about the PR to an agent (side.rs `ask_pr`), which
// reads its description, discussion and whole diff, and the code around them in a checkout of the
// repo here, and changes nothing: ⏎ asks, and the answers show above the box, in the strip a
// session's side conversations show in (SideStrip.tsx), kept to read again after it's closed.
// Neither GitHub nor the PR hears of it. `/side` and a question (or `/btw`) asks from Review too,
// as in a session's box. Who answers is the agent menu beside Ask (AgentMenu.tsx): Auto's account,
// or one picked there with its model and effort (review.ts `askOn`). A verdict goes with Review's
// text from either side, so asking never loses a review half written.
//
// A window says what a verdict does to it (`PrBoxHost`): a card goes on to what waits next at
// once and brings the PR back with what was written if GitHub refuses it; the panel stays, the
// box sending, then empty, or saying why GitHub refused it with what was written still there.

import { useEffect, useRef, useState, type RefObject } from 'react';
import { reviewAhead } from '../ahead';
import { agentsVerdict, agentsWords } from '../agentReview';
import { prSideAsk } from '../api';
import { commentsWord, readAgain, reviewedWords, saysSomething, useYourReview, verdictWords } from '../prDraft';
import { mayApprove } from '../prStatus';
import { errorText, type Report } from '../report';
import { againFor, againTitle, askAccounts, askOn, reviewLineWords, suggestWords, tryAgain } from '../review';
import { notAsked, showSides, startingSide, useKept, useSide, useSideView, type SideView } from '../side';
import { useStartAccounts, useStartGroups } from '../startAccounts';
import { AUTO, isAuto, modelName, ownModels } from '../start';
import { asText, readStored, writeStored } from '../stored';
import type { Draft, PrItem, RoutingGroup, Side, SideKept, StartAccount, Usage, Verdict } from '../types';
import { AgentMenu, type AgentPick } from './AgentMenu';
import { prIsOpen } from './PrsView';
import { BoxText, Button, cx, Icon, Seg } from './primitives';
import { prSides, SideStrip } from './SideStrip';
import { bare, chordPressed, keysIn } from '../platform';

/** What's written for each PR, kept while the window goes from one to the next and back: the
 *  review, and the question being written on Ask. */
const drafts = new Map<string, string>();
const askDrafts = new Map<string, string>();

/** Which side each PR's box is on, kept the same way. */
type BoxSide = 'review' | 'ask';
const boxSides = new Map<string, BoxSide>();

/** The account picked to answer questions about PRs, in every window, none for Auto's; and the
 *  model and effort picked with it, none for the account's own. */
const ASK_ACCOUNT = 'codealong.prask.account';
const ASK_MODEL = 'codealong.prask.model';
const ASK_EFFORT = 'codealong.prask.effort';

/** Who was picked to answer, as kept; Auto's for none. */
function readAsker(): AgentPick {
  const account = readStored(ASK_ACCOUNT, asText, '');
  return account ? { account, model: readStored(ASK_MODEL, asText, '') || null, effort: readStored(ASK_EFFORT, asText, '') || null } : { account: AUTO, model: null, effort: null };
}

/** `/side` or `/btw` at the start of what's written, and the question after it. */
const SIDE_COMMAND = /^\/(?:side|btw)(?:\s+([\s\S]*))?$/;

/** What a window does as a verdict goes. */
export interface PrBoxHost {
  report: Report;
  usage: Usage[];
  now: number;
  /** The verdict is on its way: a card's window goes on at once, and the box sends nothing more. */
  onSent?: (verdict: Verdict) => void;
  /** GitHub refused a verdict a card went on from: the window says why, with the PR back. */
  onFailed?: (why: string) => void;
  /** Puts the cursor in the box. */
  autoFocus?: boolean;
}

/** What a box holds, and what it does, for the box and a window's keys. */
export interface PrBox {
  side: BoxSide;
  asking: boolean;
  reviewText: string;
  setReviewText: (text: string) => void;
  askText: string;
  setAskText: (text: string | ((was: string) => string)) => void;
  note: { text: string; error?: boolean } | null;
  setNote: (note: { text: string; error?: boolean } | null) => void;
  box: RefObject<HTMLTextAreaElement>;
  /** Your draft review on GitHub, which a verdict submits. */
  draft: Draft | null;
  /** Any open PR but your own takes a review; anything open takes a comment. */
  reviewable: boolean;
  stillOpen: boolean;
  conversation: Side | null;
  shown: SideView;
  kept: SideKept[];
  /** The account that answers: the open conversation's, else the one picked or Auto's. */
  answerer: string | null;
  /** Who was picked beside Ask, Auto's or an account's. */
  asker: AgentPick;
  starting: boolean;
  /** The verdict being sent from a box that stays (the panel's). */
  sending: Verdict | null;
  /** The verdict that submits the agent's review in one press (`agentsVerdict`). */
  agents: Verdict | null;
  accounts: StartAccount[];
  groups: RoutingGroup[];
  /** The accounts' usage, for the agent menu beside Ask. */
  usage: Usage[];
  now: number;
  switchTo: (to: BoxSide) => void;
  /** Gives a verdict with what's written on Review. */
  review: (verdict: Verdict) => void;
  ask: (question: string) => void;
  pickAnswerer: (pick: AgentPick) => void;
  /** Tries the agent's review that didn't get done again (review.ts `tryAgain`). */
  tryAgain: () => void;
}

export function usePrBox(pr: PrItem, { report, usage, now, onSent, onFailed, autoFocus }: PrBoxHost): PrBox {
  const [side, setSide] = useState<BoxSide>(() => boxSides.get(pr.key) ?? 'review');
  const asking = side === 'ask';
  const [reviewText, setReviewText] = useState(() => drafts.get(pr.key) ?? '');
  const [askText, setAskText] = useState(() => askDrafts.get(pr.key) ?? '');
  // A verdict went from a card: it's on its way out, and a second ⌘⏎ sends nothing.
  const sent = useRef(false);
  const [sending, setSending] = useState<Verdict | null>(null);
  const [note, setNote] = useState<{ text: string; error?: boolean } | null>(null);
  const box = useRef<HTMLTextAreaElement>(null);
  const accounts = useStartAccounts();
  const groups = useStartGroups();
  const reviewable = mayApprove(pr);
  const stillOpen = prIsOpen(pr);
  const draft = useYourReview(pr, stillOpen)?.review?.draft ?? null;

  useEffect(() => {
    drafts.set(pr.key, reviewText);
  }, [pr.key, reviewText]);
  useEffect(() => {
    askDrafts.set(pr.key, askText);
  }, [pr.key, askText]);
  useEffect(() => {
    boxSides.set(pr.key, side);
  }, [pr.key, side]);
  useEffect(() => {
    if (autoFocus) box.current?.focus();
  }, [autoFocus, pr.key]);

  // Ask's conversation, and the account that answers: the open conversation's, else the one
  // picked, else Auto's.
  const conversation = useSide(pr.key);
  const shown = useSideView(pr.key);
  const kept = useKept(pr.key);
  const [asker, setAsker] = useState<AgentPick>(readAsker);
  const asks = askOn(accounts, usage, now, isAuto(asker.account) ? null : asker);
  const answerer = conversation && !conversation.ended ? (conversation.account ?? null) : (asks?.account ?? null);
  const [starting, setStarting] = useState(false);

  const switchTo = (to: BoxSide) => {
    setSide(to);
    setNote(null);
    box.current?.focus();
  };

  // The verdict that submits the agent's review in one press, what it suggests asking or saying:
  // it needs nothing written, the review saying it (agentReview.ts `agentsVerdict`).
  const agents = agentsVerdict(pr, !!draft);
  const review = (verdict: Verdict) => {
    if (sent.current || sending) return;
    const body = reviewText.trim();
    const withAgents = verdict === agents;
    if (!withAgents && !saysSomething(verdict, body, draft)) {
      setSide('review');
      setNote({ text: verdict === 'comment' ? 'Write the comment first.' : 'Say what to change first.', error: true });
      box.current?.focus();
      return;
    }
    setNote(null);
    if (onSent) {
      sent.current = true;
      drafts.delete(pr.key);
      onSent(verdict);
    } else {
      setSending(verdict);
    }
    reviewAhead(pr, verdict, body, withAgents)
      .then(
        (done) => {
          report.info(reviewedWords(verdict, pr.number, done, withAgents));
          if (!onSent) {
            setReviewText('');
            drafts.delete(pr.key);
            readAgain(pr);
          }
        },
        (e: unknown) => {
          const why = `Didn’t ${verdict === 'approve' ? 'approve' : verdict === 'comment' ? 'comment on' : 'request changes on'} #${pr.number}: ${errorText(e)}`;
          if (!onSent) {
            setNote({ text: why, error: true });
            return;
          }
          drafts.set(pr.key, reviewText);
          if (onFailed) onFailed(why);
          else report.error(why);
        },
      )
      .finally(() => setSending(null));
  };

  /** Asks the PR's conversation `question` (on Ask, or `/side` from Review), making one on the
   *  account that answers when there's none. The box clears at once and the question shows above
   *  it as the engine has it; one that can't be asked is back in Ask's box, with why. No question
   *  shows the ones kept from before. */
  const ask = (question: string) => {
    if (starting) return;
    setSide('ask');
    setNote(null);
    if (!question) {
      showSides(pr.key, { kind: 'list' });
      return;
    }
    if (!answerer) {
      setAskText(question);
      setNote({ text: 'No account here can answer questions about a PR: a Claude Code or Codex one can.', error: true });
      return;
    }
    setAskText('');
    startingSide(pr.key);
    setStarting(true);
    // An open conversation goes on on what it was made on.
    prSideAsk(pr.key, asks && !(conversation && !conversation.ended) ? asks : { account: answerer, model: null, effort: null }, question)
      .catch((e: unknown) => {
        setAskText((now_) => [question, now_].filter((t) => t.trim()).join('\n\n'));
        notAsked(pr.key);
        setNote({ text: errorText(e), error: true });
      })
      .finally(() => setStarting(false));
  };

  /** Picks who answers, from beside Ask: Auto, or one of the accounts that can with its model and
   *  effort, kept for every window. */
  const pickAnswerer = (pick: AgentPick) => {
    setAsker(pick);
    const auto = isAuto(pick.account);
    writeStored(ASK_ACCOUNT, asText, auto ? '' : pick.account);
    writeStored(ASK_MODEL, asText, auto ? '' : (pick.model ?? ''));
    writeStored(ASK_EFFORT, asText, auto ? '' : (pick.effort ?? ''));
  };

  return { side, asking, reviewText, setReviewText, askText, setAskText, note, setNote, box, draft, reviewable, stillOpen, conversation, shown, kept, answerer, asker, starting, sending, agents, accounts, groups, usage, now, switchTo, review, ask, pickAnswerer, tryAgain: () => tryAgain(pr, report) };
}

/** The model and effort an open conversation about a PR answers on, as its chip says them; null
 *  for the account's own. */
function conversationOn(c: Side, accounts: StartAccount[]): string | null {
  const a = accounts.find((x) => x.name === c.account);
  const model = c.model ? modelName(c.model, ownModels(a, accounts.flatMap((x) => x.models ?? []))) : null;
  return [model, c.effort ? `${c.effort} effort` : null].filter(Boolean).join(' · ') || null;
}

/** Where the PR's review stands, over its box: a click shows its Reviews tab. An agent review
 *  that didn't get done says why on hover, and has Try again beside it (`againFor`). */
function ReviewLine({ pr, draft, onShowReview, onTryAgain }: { pr: PrItem; draft: Draft | null; onShowReview: () => void; onTryAgain: () => void }) {
  const { icon, says, busy, approved } = reviewLineWords(pr, draft);
  const suggests = pr.autoReview?.suggests;
  const again = againFor(pr);
  const why = again ? pr.autoReview?.note : null;
  return (
    <div className="ca-prbox-standing">
      <button type="button" className={cx('ca-prbox-line', busy && 'is-busy')} title={why ? `${why}\n\nShow its Reviews tab` : 'Show its Reviews tab'} onClick={onShowReview}>
        <Icon name={icon} size={12} />
        <span className="ca-prbox-line-says">{says}</span>
        {suggests && !pr.autoReview?.submitted && <span className={cx('ca-prbox-suggests', `is-${suggests}`)}>{suggestWords(suggests)}</span>}
        {approved && !suggests && (
          <span className="ca-prbox-suggests is-approve" title={`It approved ${approved} before, so the PR waits on your sign-off meanwhile`}>
            {suggestWords('approve')} {approved}
          </span>
        )}
        <span className="ca-prbox-line-go">
          Review
          <Icon name="chevron-right" size={11} />
        </span>
      </button>
      {again && (
        <button type="button" className="ca-linkbtn ca-prbox-again" title={againTitle(again)} onClick={onTryAgain}>
          Try again
        </button>
      )}
    </div>
  );
}

export interface PrBoxProps {
  pr: PrItem;
  box: PrBox;
  /** The keys a verdict goes by, where they're said: a card's chords, or the box's own. */
  keys: { approve: string; comment: string };
  /** ⌘⏎ approves (else comments) and ⌥⌘⏎ comments from the box itself, where the window has no
   *  chords of its own for them (the panel). */
  ownKeys?: boolean;
  /** A verdict goes on to what waits next (a card's window). */
  goesOn?: boolean;
  /** Shows the PR's Reviews tab. */
  onShowReview: () => void;
  className?: string;
}

/** What Auto means beside Ask, on its line. */
const ASK_AUTO = 'The account the quick panel’s Auto would start, among those that can answer: one with room, the one whose week resets soonest. On its own model and effort.';

export function PrBoxView({ pr, box: b, keys, ownKeys = false, goesOn = false, onShowReview, className }: PrBoxProps) {
  const { asking, draft, reviewable, stillOpen, conversation, shown, kept, answerer, starting, note, sending, agents } = b;
  const text = asking ? b.askText : b.reviewText;
  const setText = asking ? b.setAskText : b.setReviewText;
  const suggests = pr.autoReview && !pr.autoReview.submitted ? pr.autoReview.suggests : null;
  // The button the agent's suggestion is, marked as such.
  const suggested = (v: Verdict) => (suggests === v ? { icon: 'sparkles', why: `The agent suggests this. ` } : null);
  const notReviewable = pr.mine ? 'GitHub doesn’t let you approve your own PR' : 'It’s closed';
  const onward = goesOn ? ' and goes on' : '';
  const withDraft = draft ? ` with your draft review${draft.count > 0 ? ` and its ${commentsWord(draft.count)}` : ''}` : '';
  // Where the agent's review goes in one press, the line says that press first, and that
  // approving is still there.
  const verdicts =
    agents === 'request-changes'
      ? `Request changes sends the agent’s review${onward} · ${keys.approve} approves anyway · ${keys.comment} comments`
      : agents === 'comment'
        ? `${keys.comment} comments with the agent’s review${onward} · ${keys.approve} approves anyway`
        : reviewable
          ? `${keys.approve} approves${withDraft}${onward} · ${keys.comment} comments`
          : stillOpen
            ? `${notReviewable}; ${keys.comment} comments${withDraft}`
            : 'It’s closed; a comment still goes to the PR';
  const answering = conversation?.running && !asking ? `${conversation.account ?? 'Ask'} is answering on Ask · ` : '';
  const said = note?.text ?? (sending ? 'Sending to GitHub…' : starting ? 'Reading the PR…' : asking ? [keysIn(`⏎ asks${answerer ? ` ${answerer}` : ''}`), reviewable && `${keys.approve} approves${onward}`].filter(Boolean).join(' · ') : answering + verdicts);
  // Ask's earlier questions, while nothing about them shows above the box.
  const earlier = kept.filter((k) => k.id !== conversation?.id);
  const sideShown = !!conversation || (!!shown && shown.kind !== 'starting') || starting;
  const button = (v: Verdict, label: string, base: string) => {
    const s = suggested(v);
    const label2 = sending === v ? 'Sending…' : label;
    // The agent's verdict submits its review, what you wrote going first.
    if (v === agents) {
      const yours = b.reviewText.trim() ? ', what you wrote before it' : '';
      return { icon: 'sparkles', title: `${label} ${agentsWords(pr.autoReview?.stage ?? '', draft ? draft.count : null)}${yours}. The agent suggests this.`, label: label2 };
    }
    return { icon: s?.icon, title: `${s?.why ?? ''}${verdictWords(v, draft) ?? base}`, label: label2 };
  };
  const changes = button('request-changes', 'Request changes', 'Ask for changes, with what’s written');
  const comment = button('comment', 'Comment', 'Comment with what’s written');
  const approve = button('approve', 'Approve', 'Approve');
  return (
    <>
      {asking && <SideStrip key={`side:${pr.key}`} sideKey={pr.key} words={prSides(pr, answerer)} />}
      <div className={cx('ca-composer', 'ca-prreview-box', asking && 'is-aside', className)}>
        <ReviewLine pr={pr} draft={draft} onShowReview={onShowReview} onTryAgain={b.tryAgain} />
        <div
          className="ca-composer-field"
          onMouseDown={(e) => {
            if ((e.target as Element).closest('button, textarea')) return;
            e.preventDefault();
            b.box.current?.focus();
          }}
        >
          <BoxText
            ref={b.box}
            value={text}
            lines={6}
            hint={asking ? `Ask about #${pr.number}: ${answerer ?? 'an agent'} reads it and its code to answer` : draft ? 'Your words, before your draft review’s text (optional)' : reviewable ? 'Your review, if it says anything' : `Comment on #${pr.number}`}
            onChange={(e) => {
              setText(e.target.value);
              if (note?.error) b.setNote(null);
            }}
            onKeyDown={(e) => {
              if (e.key !== 'Enter' || e.nativeEvent.isComposing) return;
              // ⌘⏎ and ⌥⌘⏎ are the window's chords, or the box's own where it has none.
              const approve = chordPressed('Cmd+Enter', e);
              if (approve || chordPressed('Alt+Cmd+Enter', e)) {
                if (!ownKeys) return;
                e.preventDefault();
                b.review(reviewable && approve ? 'approve' : 'comment');
                return;
              }
              if (!bare(e)) return;
              // ⏎ asks on Ask, and runs `/side` on Review; ⇧⏎ is a new line.
              const typed = e.currentTarget.value.trim();
              const command = asking ? null : SIDE_COMMAND.exec(typed);
              if (!asking && !command) return;
              e.preventDefault();
              if (command) {
                b.setReviewText('');
                b.ask((command[1] ?? '').trim());
              } else if (typed) b.ask(typed);
            }}
            aria-label={asking ? `Question about #${pr.number}` : `Review of #${pr.number}`}
          />
          <div className="ca-composer-tools ca-prreview-sides">
            <Seg
              label="What the box is for"
              options={[
                { id: 'review', label: 'Review', title: 'Write the review that goes to GitHub' },
                { id: 'ask', label: 'Ask', title: 'Ask an agent about the PR. It reads the PR and its code, changes nothing, and GitHub never sees it.' },
              ]}
              value={b.side}
              onPick={b.switchTo}
            />
            {asking && !sideShown && earlier.length > 0 && (
              <button type="button" className="ca-linkbtn" title="The questions asked about this PR before, kept by Codealong" onClick={() => showSides(pr.key, { kind: 'list' })}>
                {earlier.length === 1 ? '1 earlier' : `${earlier.length} earlier`}
              </button>
            )}
          </div>
          {asking ? (
            <div className="ca-composer-buttons ca-prreview-buttons">
              {conversation && !conversation.ended ? (
                <span className="ca-prreview-who" title="The open conversation’s agent answers. Close it above to ask another account, model or effort.">
                  {conversation.account}
                  {conversationOn(conversation, b.accounts) && <span className="ca-modelmenu-detail">{conversationOn(conversation, b.accounts)}</span>}
                </span>
              ) : (
                <AgentMenu
                  accounts={askAccounts(b.accounts)}
                  usage={b.usage}
                  now={b.now}
                  value={b.asker}
                  auto={{ title: ASK_AUTO }}
                  title="Who answers: Auto’s account, or one you pick with its model and effort"
                  disabled={askAccounts(b.accounts).length === 0}
                  onPick={b.pickAnswerer}
                  onDone={() => b.box.current?.focus()}
                />
              )}
              <button type="button" className="ca-composer-send" aria-label="Ask" title={keysIn('Ask (⏎)')} disabled={!b.askText.trim() || starting} onClick={() => b.ask(b.askText.trim())}>
                <Icon name="help" size={16} stroke={2} />
              </button>
            </div>
          ) : (
            <div className="ca-composer-buttons ca-prreview-buttons">
              {/* The agent's verdict, when its review goes in one press, is the filled one, and
                  needs nothing written; Approve steps back beside it. */}
              {reviewable && (
                <Button variant={agents === 'request-changes' ? 'primary' : 'secondary'} size="sm" icon={changes.icon} disabled={!!sending || (agents !== 'request-changes' && !saysSomething('request-changes', b.reviewText, draft))} title={changes.title} onClick={() => b.review('request-changes')}>
                  {changes.label}
                </Button>
              )}
              <Button variant={agents === 'comment' ? 'primary' : 'secondary'} size="sm" icon={comment.icon} disabled={!!sending || (agents !== 'comment' && !saysSomething('comment', b.reviewText, draft))} title={`${comment.title} (${keys.comment})`} onClick={() => b.review('comment')}>
                {comment.label}
              </Button>
              {reviewable && (
                <Button variant={agents ? 'secondary' : 'primary'} size="sm" icon={approve.icon ?? 'check'} disabled={!!sending} title={`${approve.title} (${keys.approve})`} onClick={() => b.review('approve')}>
                  {approve.label}
                </Button>
              )}
            </div>
          )}
        </div>
        <span className={cx('ca-composer-note', note?.error && 'is-error')} role="status">
          {said}
        </span>
      </div>
    </>
  );
}
