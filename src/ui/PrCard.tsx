// A PR waiting on your review as a card of its own, wherever a window works through what waits
// on you with a PR among the sessions: under the pills of the bar docked in the notch
// (NotchReview.tsx) and in the waiting window (Waiting.tsx). It's the PR panel's details
// (PrInspector.tsx `PrDetails`) with what's the same in both done once: the strip over the header
// with what's commonly done to the PR as buttons that say their keys, the box at its foot that
// reviews it, the PR's menu on a right-click, and its chords (menus.ts `PR_CHORDS`): ⌘⏎
// approves it with what's in the box and goes on, as it sends a session's message and goes on;
// ⌥⌘⏎ comments; ⌃⌘1 to ⌃⌘4 snooze the review; ⌃⌘R starts a review on an account (review.ts);
// ⌃⌘G opens it on GitHub. Asking for changes is a button in the box, with no key to press by
// mistake, and needs something written, as a comment does. It draws at the zoom, and ⌘= ⌘- ⌘0
// zoom it, as a session's card (SessionCard.tsx).
//
// Any review settles GitHub's request to you, so each of those is how the PR left what waits on
// you (`can.left`), and the window goes on to the next at once, as it does from a session's
// message on its way: the review goes to GitHub behind it. One GitHub refuses comes back round
// (`can.failed`): the PR is back in what waits on you with what you wrote, and the strip says
// why. What you wrote stays with the PR until it goes, so skipping to the next and coming back
// loses nothing.
//
// The box has two sides, Review and Ask, a switch at its foot, each with a draft of its own. Ask
// puts questions about the PR to an agent (side.rs `ask_pr`), which reads its description,
// discussion and whole diff, and the code around them in a checkout of the repo here, and changes
// nothing: ⏎ asks, and the answers show above the box, in the strip a session's side
// conversations show in (SideStrip.tsx), kept to read again after it's closed. Neither GitHub nor
// the PR hears of it. `/side` and a question (or `/btw`) asks from Review too, as in a session's
// box. The account that answers is beside Ask: Auto's, or the one picked there (review.ts
// `askAccount`). A verdict's keys work from either side and go with Review's text, so asking
// never loses a review half written.
//
// A window says only what differs: how it reports, how it opens things and shows them in
// Codealong, what it does once the PR has left, and what it adds (a queue).

import { useEffect, useRef, useState, type ReactNode } from 'react';
import { openUrl, prSideAsk, reviewPr } from '../api';
import { below, showContextMenu, type MenuAction, type MenuSpec } from '../contextMenu';
import { useLatest } from '../latest';
import * as menus from '../menus';
import { errorText, type Report } from '../report';
import { askAccount, askAccounts, reviewItems } from '../review';
import { notAsked, showSides, startingSide, useKept, useSide, useSideView } from '../side';
import { reviewSnoozeMenu } from '../snooze';
import { useStartAccounts } from '../startAccounts';
import { asText, readStored, writeStored } from '../stored';
import type { Lane, Session, Usage, Verdict } from '../types';
import { cardZoom } from '../zoom';
import { zoomKey } from './cardHost';
import { PrDetails } from './PrInspector';
import { prIsOpen, type PrRowItem } from './PrsView';
import { BoxText, Button, cx, Icon, Seg } from './primitives';
import { Hotkey, QueueStrip, snoozeKeys, snoozeRange, type Hotkeys, type Notice, type Queue } from './SessionPanel';
import { prSides, SideStrip } from './SideStrip';

/** What a window does for a PR's actions, less what the card knows for every window. */
export interface PrWindowCan {
  report: Report;
  /** Opens one of a worktree's targets: a session where it lives. */
  open: (lane: Lane, target: string) => void;
  /** Shows it in the main window, from the other windows. */
  reveal?: () => void;
  /** It left what waits on you, and how: the window goes on to the next. */
  left?: (done: menus.Left) => void;
  /** The review it left on didn't go (GitHub refused it): it's back in what waits on you, and
   *  the window says why, on whatever it shows now. */
  failed?: (why: string) => void;
}

interface PrCardProps {
  item: PrRowItem;
  now: number;
  /** The board's worktrees and plan usage, for where a review starts and on which account. */
  lanes: Lane[];
  usage: Usage[];
  can: PrWindowCan;
  /** The PR shown is on its way out: the window asked for another, which hasn't come yet. Its
   *  chords do nothing until then. */
  outgoing?: boolean;
  queue?: Queue;
  notice?: Notice | null;
  /** The strip is always there, with the PR's common actions and the keys for them. */
  hotkeys?: Hotkeys;
  /** Puts the cursor in the box. */
  autoFocus?: boolean;
  onShowSession: (lane: Lane, session: Session) => void;
  onOpenSession: (lane: Lane, session: Session) => void;
  onShowLane: (lane: Lane) => void;
  onLaneMenu: (lane: Lane) => void;
  onSessionMenu: (lane: Lane, session: Session) => void;
  /** It draws at the zoom, in percent, as a session's card does (zoom.ts `cardZoom`). */
  zoom?: number;
}

/** What's written for each PR, kept while the card goes from one to the next and back: the review,
 *  and the question being written on Ask. */
const drafts = new Map<string, string>();
const askDrafts = new Map<string, string>();

/** Which side each PR's box is on, kept the same way. */
type Side = 'review' | 'ask';
const sides = new Map<string, Side>();

/** The account picked to answer questions about PRs, in every window; none for Auto's. */
const ASK_ACCOUNT = 'codealong.prask.account';

/** `/side` or `/btw` at the start of what's written, and the question after it. */
const SIDE_COMMAND = /^\/(?:side|btw)(?:\s+([\s\S]*))?$/;

/** How the box says each verdict went. */
const DONE: Record<Verdict, { past: string; left: menus.Left }> = {
  approve: { past: 'Approved', left: 'approved' },
  comment: { past: 'Commented on', left: 'commented on' },
  'request-changes': { past: 'Requested changes on', left: 'changes requested' },
};

export function PrCard({ item, now, lanes, usage, can, outgoing = false, queue, notice, hotkeys, autoFocus, onShowSession, onOpenSession, onShowLane, onLaneMenu, onSessionMenu, zoom }: PrCardProps) {
  const { pr } = item;
  const { report } = can;
  const [side, setSide] = useState<Side>(() => sides.get(pr.key) ?? 'review');
  const asking = side === 'ask';
  const [reviewText, setReviewText] = useState(() => drafts.get(pr.key) ?? '');
  const [askText, setAskText] = useState(() => askDrafts.get(pr.key) ?? '');
  const text = asking ? askText : reviewText;
  const setText = asking ? setAskText : setReviewText;
  // A review was sent from here: the card is on its way out, and a second ⌘⏎ sends nothing.
  const sent = useRef(false);
  const [note, setNote] = useState<Notice | null>(null);
  const box = useRef<HTMLTextAreaElement>(null);
  const accounts = useStartAccounts();
  const onDesktop = menus.useDesktopPins();
  const requested = pr.reviewRequested && prIsOpen(pr);
  const written = reviewText.trim().length > 0;

  useEffect(() => {
    drafts.set(pr.key, reviewText);
  }, [pr.key, reviewText]);
  useEffect(() => {
    askDrafts.set(pr.key, askText);
  }, [pr.key, askText]);
  useEffect(() => {
    sides.set(pr.key, side);
  }, [pr.key, side]);
  useEffect(() => {
    if (autoFocus) box.current?.focus();
  }, [autoFocus, pr.key]);

  // Ask's conversation, and the account that answers: the open conversation's, else the one
  // picked, else Auto's.
  const conversation = useSide(pr.key);
  const shown = useSideView(pr.key);
  const kept = useKept(pr.key);
  const [chosen, setChosen] = useState<string | null>(() => readStored(ASK_ACCOUNT, asText, null));
  const answerer = conversation && !conversation.ended ? (conversation.account ?? null) : askAccount(accounts, usage, now, chosen);
  const [starting, setStarting] = useState(false);

  const switchTo = (to: Side) => {
    setSide(to);
    setNote(null);
    box.current?.focus();
  };

  const open = (url: string) => void openUrl(url).catch((e: unknown) => report.error(errorText(e)));
  const show = (spec: MenuSpec, at?: { x: number; y: number }) => void showContextMenu(spec, at).catch((e: unknown) => report.error(errorText(e)));

  /** Sends a review with what's in the box. The PR has left what waits on you at once; the
   *  review goes behind it, and one that doesn't go brings the PR back with what was written. */
  const review = (verdict: Verdict) => {
    if (sent.current) return;
    const body = reviewText.trim();
    if (!body && verdict !== 'approve') {
      setSide('review');
      setNote({ text: verdict === 'comment' ? 'Write the comment first.' : 'Say what to change first.', error: true });
      box.current?.focus();
      return;
    }
    sent.current = true;
    setNote(null);
    drafts.delete(pr.key);
    can.left?.(DONE[verdict].left);
    reviewPr(pr.repoSlug, pr.number, verdict, body).then(
      () => report.info(`${DONE[verdict].past} #${pr.number}.`),
      (e: unknown) => {
        drafts.set(pr.key, reviewText);
        const why = `Didn’t ${verdict === 'approve' ? 'approve' : verdict === 'comment' ? 'comment on' : 'request changes on'} #${pr.number}: ${errorText(e)}`;
        if (can.failed) can.failed(why);
        else report.error(why);
      },
    );
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
    prSideAsk(pr.key, answerer, question)
      .catch((e: unknown) => {
        setAskText((now_) => [question, now_].filter((t) => t.trim()).join('\n\n'));
        notAsked(pr.key);
        setNote({ text: errorText(e), error: true });
      })
      .finally(() => setStarting(false));
  };

  /** Picks the account that answers, from beside Ask: Auto, or one of those that can. */
  const pickAnswerer = (el: Element) => {
    const can = askAccounts(accounts);
    const auto = askAccount(accounts, usage, now, null);
    const pick = (name: string | null) => {
      setChosen(name);
      writeStored(ASK_ACCOUNT, asText, name ?? '');
      box.current?.focus();
    };
    const items: MenuSpec = [{ label: auto ? `Auto (${auto})` : 'Auto', checked: !chosen || !can.some((a) => a.name === chosen), run: () => pick(null) }, 'separator', ...can.map((a) => ({ label: a.name, checked: a.name === chosen, run: () => pick(a.name) }))];
    show(items, below(el));
  };

  const reviewWith = reviewItems(pr, lanes, accounts, usage, report, () => can.left?.('review started'), now);
  const actions: menus.PrActions = {
    approve: requested ? { label: `Approve #${pr.number}`, run: () => review('approve') } : undefined,
    comment: requested ? { label: 'Comment', run: () => review('comment') } : undefined,
    changes: requested ? { label: 'Request changes', run: () => review('request-changes') } : undefined,
    review: reviewWith.length > 0 ? { label: 'Review with', items: reviewWith } : undefined,
    // As a session's snooze settles (menus.ts `settle`): the review has left what waits on you
    // at once, and what goes wrong is reported after.
    snooze: reviewSnoozeMenu(
      pr,
      now,
      (act) => {
        can.left?.('snoozed');
        act.catch((e: unknown) => report.error(errorText(e)));
      },
      true,
    ),
    github: { label: 'Open on GitHub', run: () => open(pr.url) },
  };

  // The PR's chords, for the card shown, and ⌘= ⌘- ⌘0, which zoom it as a session's card. Not for
  // one on its way out.
  const latest = useRef(actions);
  latest.current = actions;
  const leaving = useRef(outgoing);
  leaving.current = outgoing;
  const reportNow = useLatest(report);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.isComposing) return;
      if (zoomKey(e, reportNow.current)) return;
      const action = menus.prChorded(latest.current, e);
      if (!action) return;
      e.preventDefault();
      if (leaving.current) return;
      action.run();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [reportNow]);

  const menu = (at?: { x: number; y: number }) =>
    show(
      menus.prMenu(pr, {
        report,
        open: () => open(pr.url),
        review: actions.review?.items.filter((x): x is MenuAction => x !== 'separator' && !('items' in x)),
        reveal: can.reveal,
        approve: actions.approve?.run,
        snooze: actions.snooze,
        desktop: onDesktop,
      }),
      at,
    );

  const sign = (name: keyof typeof menus.PR_CHORDS) => menus.chordSigns(menus.PR_CHORDS[name] ?? '');
  const snooze = actions.snooze;
  const buttons: ReactNode = (
    <>
      <Hotkey label="Approve" keys={sign('approve')} title={actions.approve ? 'Approve it, with what’s written on Review as the review’s text, and go on' : 'It doesn’t ask for your review'} run={actions.approve && (() => actions.approve?.run())} />
      <Hotkey label="Comment" keys={sign('comment')} title={actions.comment ? 'Comment on it with what’s written on Review, and go on' : 'It doesn’t ask for your review'} run={actions.comment && (() => actions.comment?.run())} />
      <Hotkey label="Snooze" keys={snoozeRange()} title={snooze ? `Snooze the review: it waits apart until its time comes. ${snoozeKeys(snooze)}` : 'The review can’t be snoozed'} run={snooze && ((el) => void showContextMenu(snooze.items, below(el)).catch(() => {}))} />
      <Hotkey label="Review with" keys={sign('review')} title={actions.review ? `Start a review on an account; the key starts it ${actions.review.items[0] && actions.review.items[0] !== 'separator' && !('items' in actions.review.items[0]) ? actions.review.items[0].label.replace(/^Auto \([^)]*\) /, 'on Auto ') : ''}` : 'No account can start a review here'} run={actions.review && ((el) => void showContextMenu(actions.review?.items ?? [], below(el)).catch(() => {}))} />
      <Hotkey label="GitHub" keys={sign('github')} title="Open it on GitHub" run={() => actions.github.run()} />
    </>
  );

  const verdicts = requested ? `${sign('approve')} approves and goes on · ${sign('comment')} comments` : 'Your review isn’t asked for; a comment still goes to the PR';
  const answering = conversation?.running && !asking ? `${conversation.account ?? 'Ask'} is answering on Ask · ` : '';
  const said = note?.text ?? (starting ? 'Reading the PR…' : asking ? [`⏎ asks${answerer ? ` ${answerer}` : ''}`, requested && `${sign('approve')} approves and goes on`].filter(Boolean).join(' · ') : answering + verdicts);
  // Ask's earlier questions, while nothing about them shows above the box.
  const earlier = kept.filter((k) => k.id !== conversation?.id);
  const sideShown = !!conversation || (!!shown && shown.kind !== 'starting') || starting;
  return (
    <div
      className={cx('ca-reply', 'ca-prreview')}
      style={cardZoom(zoom)}
      onContextMenu={(e) => {
        if (e.defaultPrevented || (e.target instanceof Element && e.target.closest('input, textarea, [contenteditable="true"]'))) return;
        e.preventDefault();
        menu({ x: e.clientX, y: e.clientY });
      }}
    >
      <QueueStrip queue={queue} notice={notice} hotkeys={hotkeys} buttons={buttons} />
      <div className="ca-prreview-top">
        <Button variant="ghost" size="sm" icon="dots" aria-label="More" aria-haspopup="menu" title="Snooze, review with, pin, copy and more" onClick={(e) => menu(below(e.currentTarget))} />
      </div>
      <div className="ca-prreview-body ca-prinsp">
        <PrDetails item={item} now={now} keys onShowSession={onShowSession} onOpenSession={onOpenSession} onShowLane={onShowLane} onLaneMenu={onLaneMenu} onSessionMenu={onSessionMenu} />
      </div>
      {asking && <SideStrip key={`side:${pr.key}`} sideKey={pr.key} words={prSides(pr, answerer)} />}
      <div className={cx('ca-composer', 'ca-prreview-box', asking && 'is-aside')}>
        <div
          className="ca-composer-field"
          onMouseDown={(e) => {
            if ((e.target as Element).closest('button, textarea')) return;
            e.preventDefault();
            box.current?.focus();
          }}
        >
          <BoxText
            ref={box}
            value={text}
            lines={6}
            hint={asking ? `Ask about #${pr.number}: ${answerer ?? 'an agent'} reads it and its code to answer` : requested ? 'Your review, if it says anything' : `Comment on #${pr.number}`}
            onChange={(e) => {
              setText(e.target.value);
              if (note?.error) setNote(null);
            }}
            onKeyDown={(e) => {
              // ⏎ asks on Ask, and runs `/side` on Review; ⇧⏎ is a new line, and ⌘⏎ and ⌥⌘⏎
              // are the card's.
              if (e.key !== 'Enter' || e.shiftKey || e.metaKey || e.ctrlKey || e.altKey || e.nativeEvent.isComposing) return;
              const typed = e.currentTarget.value.trim();
              const command = asking ? null : SIDE_COMMAND.exec(typed);
              if (!asking && !command) return;
              e.preventDefault();
              if (command) {
                setReviewText('');
                ask((command[1] ?? '').trim());
              } else if (typed) ask(typed);
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
              value={side}
              onPick={switchTo}
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
                <span className="ca-prreview-who" title="The open conversation’s agent answers. Close it above to ask another account.">
                  {conversation.account}
                </span>
              ) : (
                <button type="button" className="ca-modelmenu-chip" title="Which account answers: Auto’s, or the one you pick" aria-haspopup="menu" disabled={askAccounts(accounts).length === 0} onClick={(e) => pickAnswerer(e.currentTarget)}>
                  <span>{answerer ?? 'No account'}</span>
                  <Icon name="chevron-down" size={12} />
                </button>
              )}
              <button type="button" className="ca-composer-send" aria-label="Ask" title="Ask (⏎)" disabled={!askText.trim() || starting} onClick={() => ask(askText.trim())}>
                <Icon name="help" size={16} stroke={2} />
              </button>
            </div>
          ) : (
            <div className="ca-composer-buttons ca-prreview-buttons">
              {actions.changes && (
                <Button size="sm" disabled={!written} title="Ask for changes, with what’s written" onClick={() => actions.changes?.run()}>
                  Request changes
                </Button>
              )}
              <Button size="sm" disabled={!written} title={actions.comment ? `Comment with what’s written (${sign('comment')})` : 'Comment on the PR'} onClick={() => review('comment')}>
                Comment
              </Button>
              {actions.approve && (
                <Button variant="primary" size="sm" icon="check" title={`Approve (${sign('approve')})`} onClick={() => actions.approve?.run()}>
                  Approve
                </Button>
              )}
            </div>
          )}
        </div>
        <span className={cx('ca-composer-note', note?.error && 'is-error')} role="status">
          {said}
        </span>
      </div>
    </div>
  );
}
