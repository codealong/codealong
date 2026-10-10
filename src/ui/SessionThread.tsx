// A session's conversation in its panel: your prompts, the agent's replies, and the tools it ran
// with what they gave back, read from the session's own record a page at a time (transcript.rs).
// It opens at the newest and loads older pages as you scroll up, holding your place; new steps
// come in at the bottom as the session goes on, and the view follows them if you're at the end.
// Who said a message and when heads a run of them, not each one: your prompt, then the agent's
// reply, whose later messages (between its tool calls) go on under that heading. A heading comes
// again where the speaker changes, after a note, or after a pause (`headingsOf`). A right-click on
// a message copies it as it was written, markdown and all, or what you'd selected in it; in a
// card the session's own menu follows.
//
// A tool call is a line; click it for its input and output. A run of calls folds into one line
// that keeps its latest call in view. Questions it asks you (Claude Code's AskUserQuestion) open
// as questions with their choices rather than the call's input, marked with your picks once
// they're answered. A call that made a page on the web (an artifact Claude Code published) says
// so under its line, as a link to it, and stays out of a fold; web links in a tool's output open
// too.
//
// A prompt of yours shows its images in place, whichever way they went: sent from Codealong,
// pasted into the session's terminal, or attached in its harness's app. The engine keeps the
// ones a record holds with Codealong's own and writes each as text refers to a kept image
// (transcript.rs), so they show as an image in a todo's description does; a click opens one.
//
// A message you sent shows at the end as it goes (ahead.ts): sending, then sent, until the
// session's record has it and it shows from there. One that went but that the session shows no
// sign of taking, well after, says so, with a way to open the session and look.
//
// A prompt the session took back before its agent saw it (Claude Code does when a turn is
// stopped before any reply, claude/conversation.rs) stays in view, faint, saying so. Claude Code puts it back
// in its own input box, and the latest one goes back in the box below too, once.
//
// ⌘⇧↑ and ⌘⇧↓ bring your previous or next prompt to the top of the view, as Ghostty and iTerm2
// jump between prompts, from the box below too; ⌘↑ and ⌘↓ do it (Terminal's keys) while the
// cursor isn't in a text box, where they go to its start or end. Up past what's been read reads
// earlier pages until it finds one. The prompt behind what's in view, once it's gone off the top,
// stays pinned there, two lines of it (more on hover), so what you asked stays in mind while you
// read what came of it; a click brings the whole of it up. A long turn can write more than a page,
// so when the first page doesn't start with a prompt, the pages before it are read (and kept for
// scrolling up) until one does: it's pinned from the start.
//
// ⌘F finds words in it (ThreadFind.tsx): what `FIND_IN` names is what's searched, the whole
// conversation's pages read for it, and a run of calls opens to show the ones it finds.

import { useCallback, useContext, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { delivered, dismiss, inRecord, useAhead, type Ahead } from '../ahead';
import { onHostedStream, sessionNewer, sessionPage, sessionStream } from '../api';
import { putBack } from '../drafts';
import { snippet } from '../todos';
import { absolute, agoPhrase } from '../format';
import type { HostedStream, Question, Session, Status, TranscriptItem, TranscriptPage } from '../types';
import { selectedForMenu, type MenuSpec } from '../contextMenu';
import { CodeBlock } from './CopyButton';
import { DescriptionView } from './DescriptionEditor';
import { Link, linked, Markdown } from './Markdown';
import { cx, Icon, Rich, StatusGlyph } from './primitives';
import { QuestionList } from './QuestionList';
import { RunContext, type RunPlace } from './RunButton';
import { errorText } from '../report';
import { clearNotice, failedNotice, reportNotice } from '../notices';
import { says, shownText } from '../threadFind';
import { scaleOf } from '../zoom';
import { FindContext, ThreadFind } from './ThreadFind';
import { Tip } from './Tip';

/** Loads the page before once you're this close to the top. */
const NEAR_TOP = 240;
/** Tool calls in a row before they fold. */
const FOLD_AT = 3;
/** Pages read back past the first, looking for the prompt behind it, before giving up. */
const LOOK_BACK = 16;
/** A message this long after the step before it heads itself again, though the same speaker said
 *  the one before: the agent woken by a background job's report long after it went quiet. */
const PAUSE = 10 * 60_000;
/** What ⌘F searches: your prompts, the replies, notes, the calls' lines, a call's input and output
 *  once it's open, the commands you ran with what they printed, and the page a call made. */
const FIND_IN = '.ca-thread-you, .ca-thread-agent, .ca-thread-note, .ca-thread-toollabel, .ca-thread-tooldetail, .ca-thread-ran, .ca-thread-madetitle';

type Row =
  | { kind: 'item'; item: TranscriptItem; result?: TranscriptItem }
  | { kind: 'tools'; calls: { item: TranscriptItem; result?: TranscriptItem }[] };

const keyOf = (it: TranscriptItem) => `${it.cursor}:${it.n}`;

/** The items as rows: results joined to their calls, and to commands you ran in the session's
 *  shell, runs of calls folded. */
export function rowsOf(items: TranscriptItem[]): Row[] {
  const results = new Map<string, TranscriptItem>();
  for (const it of items) if (it.kind === 'result' && it.id) results.set(it.id, it);
  const rows: Row[] = [];
  let run: { item: TranscriptItem; result?: TranscriptItem }[] = [];
  const flush = () => {
    if (run.length >= FOLD_AT) rows.push({ kind: 'tools', calls: run });
    else rows.push(...run.map((c) => ({ kind: 'item' as const, ...c })));
    run = [];
  };
  for (const it of items) {
    if (it.kind === 'result') continue;
    if (it.kind === 'tool') {
      const call = { item: it, result: it.id ? results.get(it.id) : undefined };
      // A call that made a page is one to open: the calls around it fold, and it stays in view.
      if (it.link || call.result?.link) {
        flush();
        rows.push({ kind: 'item', ...call });
      } else run.push(call);
      continue;
    }
    flush();
    rows.push({ kind: 'item', item: it, result: it.kind === 'you' && it.id ? results.get(it.id) : undefined });
  }
  flush();
  return rows;
}

/** Who spoke last, and when the last step was (0 when its record didn't say). */
export interface Speaker {
  kind: 'you' | 'agent' | 'codealong' | null;
  at: number;
}

/** Whether a message heads itself, after `last`: where the speaker changes, or after a pause. */
export function heads(last: Speaker, kind: 'you' | 'agent' | 'codealong', at: number): boolean {
  return kind !== last.kind || (at > 0 && last.at > 0 && at - last.at >= PAUSE);
}

/** The messages that show who said them and when, and who spoke last, for one streaming in after
 *  them. A note between two messages (a message from another session, the conversation
 *  compacted) starts afresh, and a prompt the session took back always heads itself, to say so.
 *  The agent's tool calls are its own, so they don't. */
export function headingsOf(items: TranscriptItem[]): { headed: Set<string>; last: Speaker } {
  const headed = new Set<string>();
  const last: Speaker = { kind: null, at: 0 };
  for (const it of items) {
    if (it.kind === 'you' || it.kind === 'agent' || it.kind === 'codealong') {
      if (heads(last, it.kind, it.at) || it.withdrawn) headed.add(keyOf(it));
      last.kind = it.kind;
    } else if (it.kind === 'note') last.kind = null;
    if (it.at > 0) last.at = it.at;
  }
  return { headed, last };
}

/** Marks an element for a moment with a class whose animation shows it, again if it's marked. */
function flash(el: HTMLElement, mark: string) {
  el.classList.remove(mark);
  void el.offsetWidth;
  el.classList.add(mark);
}

/** The icon for a tool, by the start of its label. */
function toolIcon(label: string): string {
  const name = label.split(' · ')[0].toLowerCase();
  if (/bash|shell|exec|command/.test(name)) return 'terminal';
  if (/edit|write|patch|notebook/.test(name)) return 'code';
  if (/read|view/.test(name)) return 'file';
  if (/grep|glob|search|find/.test(name)) return 'search';
  if (/agent|task/.test(name)) return 'sparkles';
  if (/artifact/.test(name)) return 'world';
  return 'bolt';
}

/** A question call's line: what it asks about, by the questions' headers when they all have one. */
export function questionsLine(questions: Question[]): string {
  const headers = questions.map((q) => q.header).filter(Boolean);
  const about = headers.length === questions.length ? headers.join(', ') : questions[0].question;
  return `Asks you · ${about}`;
}

function ToolLine({ item, result, state, onOpen }: { item: TranscriptItem; result?: TranscriptItem; state: 'done' | 'running' | 'asks' | 'open'; onOpen?: () => void }) {
  const questions = item.questions?.length ? item.questions : null;
  // Questions it waits on you for show without a click, unless the reply area below shows them.
  const [open, setOpen] = useState(!!questions && state === 'asks' && !!onOpen);
  const output = item.output ?? result?.output;
  const failed = item.failed || !!result?.failed;
  const detail = questions || item.input || output;
  const link = item.link ?? result?.link;
  return (
    <div className={cx('ca-thread-tool', failed && 'is-failed', state === 'asks' && 'is-asks')}>
      <button type="button" className="ca-thread-toolhead" aria-expanded={open} disabled={!detail} onClick={() => setOpen(!open)}>
        <Icon name={questions ? 'help' : toolIcon(item.text)} size={13} />
        <span className="ca-thread-toollabel">
          <Rich text={questions ? questionsLine(questions) : item.text} now={0} />
        </span>
        {state === 'running' ? (
          <StatusGlyph status="working" size={11} />
        ) : state === 'asks' ? (
          <span className="ca-thread-asks">Waiting for you</span>
        ) : failed ? (
          <Icon name="x" size={12} stroke={2.25} title="Failed" />
        ) : result || item.output != null ? (
          <Icon name="check" size={12} stroke={2.25} title="Done" />
        ) : null}
        {detail && <Icon name={open ? 'chevron-down' : 'chevron-right'} size={12} />}
      </button>
      {link && (
        <div className="ca-thread-made">
          <Link url={link.url} plain>
            <span className="ca-thread-madetitle">{link.title}</span>
            <Icon name="arrow-up-right" size={12} />
          </Link>
          {link.note && <span className="ca-meta">{link.note}</span>}
        </div>
      )}
      {state === 'asks' && onOpen && (
        <div className="ca-thread-askrow">
          <span>{questions ? 'It’s asking you.' : 'It’s waiting for your approval.'}</span>
          <button type="button" className="ca-btn ca-btn--secondary ca-btn--sm" onClick={onOpen}>
            <Icon name="arrow-up-right" size={13} />
            <span>Open to answer</span>
          </button>
        </div>
      )}
      {open && questions && (
        <div className="ca-thread-tooldetail">
          <QuestionList questions={questions} answers={result?.answers} />
          {/* Declined, or refused: why, from its result. Answered, the picks above say it all. */}
          {failed && output && <CodeBlock text={output} className="ca-thread-out is-failed" />}
        </div>
      )}
      {open && !questions && (
        <div className="ca-thread-tooldetail">
          {item.input && <CodeBlock text={item.input} className="ca-thread-in" run={item.shell && state !== 'asks' ? { folder: item.cwd } : undefined} />}
          {output && (
            <CodeBlock text={output} className={cx('ca-thread-out', failed && 'is-failed')}>
              {linked(output)}
            </CodeBlock>
          )}
        </div>
      )}
    </div>
  );
}

function ToolRun({ calls, stateOf, onOpen }: { calls: { item: TranscriptItem; result?: TranscriptItem }[]; stateOf: (it: TranscriptItem) => 'done' | 'running' | 'asks' | 'open'; onOpen?: () => void }) {
  // Folded until you open it, or while ⌘F finds one of the calls it folds away; a click says
  // which from then on.
  const [opened, setOpen] = useState<boolean | null>(null);
  const last = calls[calls.length - 1];
  const earlier = calls.slice(0, -1);
  const finding = useContext(FindContext);
  const open = opened ?? (!!finding && earlier.some((c) => says(finding, shownText(c.item))));
  const names = [...new Set(earlier.map((c) => c.item.text.split(' · ')[0]))].slice(0, 4).join(', ');
  const failed = earlier.filter((c) => c.item.failed || c.result?.failed).length;
  return (
    <div className="ca-thread-run">
      <button type="button" className="ca-thread-runhead" aria-expanded={open} onClick={() => setOpen(!open)}>
        <Icon name={open ? 'chevron-down' : 'chevron-right'} size={12} />
        <span>
          {earlier.length} tool calls · {names}
          {failed > 0 ? ` · ${failed} failed` : ''}
        </span>
      </button>
      {open && earlier.map((c) => <ToolLine key={keyOf(c.item)} item={c.item} result={c.result} state={stateOf(c.item)} onOpen={onOpen} />)}
      <ToolLine item={last.item} result={last.result} state={stateOf(last.item)} onOpen={onOpen} />
    </div>
  );
}

/** Something said: yours or the agent's, under who said it and when where it `heads` a run of
 *  them. A command you ran in the session's shell with `!` shows as the command, which can run
 *  again, and what it printed (`result`). A right-click gives `onMenu` Copy for what you'd
 *  selected and Copy message for the message as it was written, markdown and all, which marks
 *  the message for a moment once it's copied. A command has its own Copy. */
function Said({ item, result, now, label, head, onPutBack, onMenu }: { item: TranscriptItem; result?: TranscriptItem; now: number; label: ReactNode; head: boolean; onPutBack?: () => void; onMenu?: (items: MenuSpec) => void }) {
  const ran = item.kind === 'you' && item.input != null;
  // Copying it went wrong: said under it for a moment, where it was asked for.
  const [unCopied, setUnCopied] = useState(false);
  useEffect(() => {
    if (!unCopied) return;
    const id = window.setTimeout(() => setUnCopied(false), 4000);
    return () => window.clearTimeout(id);
  }, [unCopied]);
  const menu = (e: React.MouseEvent<HTMLDivElement>) => {
    if (!onMenu || e.defaultPrevented) return;
    const el = e.currentTarget;
    const copy = (text: string, mark: boolean) =>
      void navigator.clipboard.writeText(text).then(
        () => mark && flash(el, 'is-copied'),
        () => setUnCopied(true),
      );
    const selected = selectedForMenu();
    const items: MenuSpec = [];
    if (selected) items.push({ label: 'Copy', accelerator: 'Cmd+C', run: () => copy(selected, false) });
    if (item.text.trim() && !ran) items.push({ label: 'Copy message', run: () => copy(item.text, true) });
    if (items.length === 0) return;
    e.preventDefault();
    onMenu(items);
  };
  return (
    <div className={cx('ca-thread-said', `is-${item.kind}`, item.withdrawn && 'is-withdrawn')} data-prompt={item.kind === 'you' ? keyOf(item) : undefined} onContextMenu={menu}>
      {head && (
        <div className="ca-thread-meta">
          <span>{label}</span>
          <span className="ca-meta" title={absolute(item.at)}>
            {item.withdrawn ? `Taken back · ${agoPhrase(item.at, now)}` : agoPhrase(item.at, now)}
          </span>
        </div>
      )}
      {ran ? (
        <div className="ca-thread-ran">
          {/* As typed in the session's box, led by its `!`. */}
          <CodeBlock text={`! ${item.input ?? ''}`} className="ca-thread-in" run={item.shell ? { folder: item.cwd, session: true } : undefined} />
          {result?.output && (
            <CodeBlock text={result.output} className="ca-thread-out">
              {linked(result.output)}
            </CodeBlock>
          )}
        </div>
      ) : item.kind === 'you' ? (
        <YourText text={item.text} />
      ) : item.kind === 'codealong' ? (
        // Its files (a review handed back) open as yours do.
        <div className="ca-thread-codealong">
          <DescriptionView text={item.text} />
        </div>
      ) : (
        <Markdown text={item.text} className="ca-thread-agent" />
      )}
      {unCopied && (
        <p className="ca-thread-uncopied" role="alert">
          Couldn’t copy it.
        </p>
      )}
      {item.withdrawn && (
        <div className="ca-thread-askrow">
          <span>The turn was stopped before a reply, so the agent never saw this.</span>
          {onPutBack && (
            <button type="button" className="ca-btn ca-btn--secondary ca-btn--sm" onClick={onPutBack}>
              <Icon name="arrow-back-up" size={13} />
              <span>Put back in the box</span>
            </button>
          )}
        </div>
      )}
    </div>
  );
}

/** What you wrote, as you wrote it, with the images you sent shown in place and files as chips. */
function YourText({ text }: { text: string }) {
  return (
    <div className="ca-thread-you">
      <DescriptionView text={text} />
    </div>
  );
}

/** A message of yours on its way, or one the session shows no sign of having taken. */
function Unsaid({ ahead, onOpen }: { ahead: Ahead; onOpen: () => void }) {
  const c = ahead.change;
  if (c.kind !== 'send') return null;
  const state = ahead.doneAt == null ? 'Sending…' : ahead.unconfirmed ? 'Not in the session yet' : 'Sent';
  return (
    <div className={cx('ca-thread-said', 'is-you', 'is-ahead', ahead.doneAt == null && 'is-going', ahead.unconfirmed && 'is-unconfirmed')} data-prompt={`ahead:${ahead.id}`}>
      <div className="ca-thread-meta">
        <span>You</span>
        <span className="ca-meta">{state}</span>
      </div>
      <YourText text={c.text} />
      {ahead.unconfirmed && (
        <div className="ca-thread-askrow">
          <span>It went, but the session hasn’t shown it. It may not have reached it.</span>
          <button type="button" className="ca-btn ca-btn--secondary ca-btn--sm" onClick={onOpen}>
            <Icon name="arrow-up-right" size={13} />
            <span>Open to check</span>
          </button>
          <button type="button" className="ca-btn ca-btn--ghost ca-btn--sm" title="Stop showing this" onClick={() => dismiss(ahead.id)}>
            <Icon name="x" size={13} />
          </button>
        </div>
      )}
    </div>
  );
}

interface SessionThreadProps {
  session: Session;
  now: number;
  /** Opens the session where it lives, to answer what it asks. */
  onOpen: () => void;
  /** Shown when the record can't be read (Antigravity, or a transcript that's gone). */
  fallback: ReactNode;
  /** Once the first page is laid out, or the record couldn't be read: the peek shows then. */
  onLoaded?: () => void;
  /** The panel's reply area shows what the session waits on (Allow and Deny, or where to answer
   *  it), so the thread only marks the call and doesn't ask again. */
  answeredBelow?: boolean;
  /** Where its shell commands run, for a session on this Mac: its blocks get a Run button. */
  run?: RunPlace;
  /** Shows the menu a right-click on a message asks for, the window's way (SessionPanel.tsx
   *  `onThreadMenu`). Without it a message has none. */
  onMenu?: (items: MenuSpec) => void;
}

export function SessionThread({ session: s, now, onOpen, fallback, onLoaded, answeredBelow, run, onMenu }: SessionThreadProps) {
  const [items, setItems] = useState<TranscriptItem[]>([]);
  const [before, setBefore] = useState<number | null>(null);
  // `before` as soon as it's set, for laying out pages that must begin where the last ones did.
  const shownBefore = useRef<number | null>(null);
  const placeBefore = useCallback((at: number | null) => {
    shownBefore.current = at;
    setBefore(at);
  }, []);
  const [error, setError] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);
  const after = useRef(0);
  const loading = useRef(false);
  const scroller = useRef<HTMLDivElement>(null);
  // How the view should land after the next render: at the end, or where it was from the end.
  const anchor = useRef<{ kind: 'end' } | { kind: 'keep'; fromEnd: number } | null>(null);
  const atEnd = useRef(true);
  // Where the view was last put at the end. While it hasn't moved from there you're at the end,
  // whatever has taken room below since: a prompt's image can lay out, and the scroll that put
  // the view there be heard, before the image's load is.
  const endAt = useRef<number | null>(null);
  const toEnd = useCallback((el: HTMLElement) => {
    el.scrollTop = el.scrollHeight;
    endAt.current = el.scrollTop;
    atEnd.current = true;
  }, []);

  // The prompt the keys last brought up, and how far down the view it sat: the next key goes on
  // from it while it's still there, and from the top of the view once you've scrolled.
  const jumped = useRef<{ key: string; offset: number } | null>(null);
  // Up past the pages read so far, for the prompt before (null) or for one of them: the next page
  // is searched as it lands.
  const seeking = useRef<{ key: string | null } | null>(null);
  // Your prompt behind what's in view, once it's gone off the top.
  const [pinned, setPinned] = useState<{ key: string; text: string } | null>(null);
  // The prompt before the pages read, when the first doesn't start with one, and the pages read
  // looking for it, by where each ends: scrolling up takes them from here.
  const [earlier, setEarlier] = useState<TranscriptItem | null>(null);
  const readAhead = useRef(new Map<number, Promise<TranscriptPage>>());
  const pageAt = useCallback((key: string, at: number) => {
    const kept = readAhead.current;
    let p = kept.get(at);
    if (!p) {
      p = sessionPage(key, at);
      kept.set(at, p);
      p.catch(() => kept.delete(at));
    }
    return p;
  }, []);

  // A session Codealong runs streams its reply before its record has it: shown at the end
  // until the record catches up at the end of the message.
  const [stream, setStream] = useState<HostedStream | null>(null);
  useEffect(() => {
    setStream(null);
    if (!s.hosted) return;
    let live = true;
    sessionStream(s.key)
      .then((st) => live && setStream(st))
      // Only the reply ahead of its record: the record shows it once it's written.
      .catch(() => {});
    const off = onHostedStream((st) => {
      if (st.key !== s.key) return;
      if (atEnd.current) anchor.current = { kind: 'end' };
      setStream(st);
    });
    return () => {
      live = false;
      off();
    };
  }, [s.key, s.hosted]);
  const streaming = s.hosted && s.status === 'working' && stream ? stream : null;

  // Each read from the start: cursors are places in one record, and a job that went on from
  // another session's conversation writes its own at its next prompt, so the view reads from
  // the start again then, and pages still coming from the last read are dropped.
  const reading = useRef(0);
  // The read whose first page is in, which `after` is a place in.
  const landed = useRef(0);
  useEffect(() => {
    const read = ++reading.current;
    jumped.current = null;
    seeking.current = null;
    readAhead.current = new Map();
    setEarlier(null);
    setItems([]);
    placeBefore(null);
    setLoaded(false);
    setError(null);
    loading.current = true;
    sessionPage(s.key, null)
      .then((p) => {
        if (read !== reading.current) return;
        anchor.current = { kind: 'end' };
        setItems(p.items);
        placeBefore(p.before);
        after.current = p.after;
        landed.current = read;
        setLoaded(true);
        if (p.before != null && p.items[0]?.kind !== 'you') void lookBack(read, p.before);
      })
      .catch((e: unknown) => read === reading.current && (setError(errorText(e)), setLoaded(true)))
      .finally(() => read === reading.current && (loading.current = false));
    // The prompt before the first page, read back a page at a time.
    const lookBack = async (read: number, at: number | null) => {
      for (let i = 0; i < LOOK_BACK && at != null; i++) {
        // Only a heading for the prompt before the first page; without it there's none.
        const p = await pageAt(s.key, at).catch(() => null);
        if (!p || read !== reading.current) return;
        const you = [...p.items].reverse().find((it) => it.kind === 'you');
        if (you) return setEarlier(you);
        at = p.before;
      }
    };
  }, [s.key, s.continues, pageAt, placeBefore]);

  const loadOlder = useCallback(() => {
    if (loading.current || before == null) return;
    loading.current = true;
    const read = reading.current;
    const el = scroller.current;
    const at = before;
    pageAt(s.key, at)
      .then((p) => {
        if (read !== reading.current) return;
        readAhead.current.delete(at);
        anchor.current = { kind: 'keep', fromEnd: el ? el.scrollHeight - el.scrollTop : 0 };
        setItems((old) => [...p.items, ...old]);
        placeBefore(p.before);
      })
      .catch(() => (seeking.current = null))
      .finally(() => read === reading.current && (loading.current = false));
  }, [s.key, before, pageAt, placeBefore]);

  /** Lays out pages read back from where the ones shown begin, nearest first, in one go: the way
   *  back to what ⌘F found. Not while a page is on its way in, which would come in twice. */
  const show = useCallback(
    (pages: { at: number; page: TranscriptPage }[]) => {
      if (loading.current || pages.length === 0 || pages[0].at !== shownBefore.current) return false;
      const el = scroller.current;
      for (const { at } of pages) readAhead.current.delete(at);
      anchor.current = { kind: 'keep', fromEnd: el ? el.scrollHeight - el.scrollTop : 0 };
      setItems((old) => [...[...pages].reverse().flatMap((p) => p.page.items), ...old]);
      placeBefore(pages[pages.length - 1].page.before);
      return true;
    },
    [placeBefore],
  );

  // As the session goes on: what's new since the last read. OpenCode rereads a message still
  // streaming, so an item already here is replaced by its place.
  useEffect(() => {
    const read = reading.current;
    if (!loaded || error || landed.current !== read) return;
    let live = true;
    const notice = `conversation:${s.key}`;
    sessionNewer(s.key, after.current)
      .then((p) => {
        clearNotice(notice);
        if (!live || read !== reading.current || p.items.length === 0) return;
        after.current = p.after;
        if (atEnd.current) anchor.current = { kind: 'end' };
        const fresh = new Set(p.items.map((it) => it.cursor));
        setItems((old) => [...old.filter((it) => !fresh.has(it.cursor)), ...p.items]);
      })
      .catch((e: unknown) => reportNotice(failedNotice(notice, 'A conversation isn’t updating', `Couldn’t read what’s new in “${s.title}”; it’s asked again as the session goes on`, e)));
    return () => {
      live = false;
    };
  }, [s.lastActivity, s.status, s.title, loaded, error, s.key]);

  useLayoutEffect(() => {
    const el = scroller.current;
    const a = anchor.current;
    if (!el || !a) return;
    anchor.current = null;
    if (a.kind === 'end') toEnd(el);
    else el.scrollTop = el.scrollHeight - a.fromEnd;
    // A first page too short to scroll can't be scrolled up to load more: load it now.
    if (el.scrollHeight <= el.clientHeight + NEAR_TOP) loadOlder();
  }, [items, loadOlder, stream, toEnd]);

  // An image in a prompt takes its room when it loads, after the view was placed. One that loads
  // while you're at the end keeps you there, and one above what's in view leaves that where it
  // was. WebKit holds no place by itself as content above grows, and the thread asks Chrome not to
  // (overflow-anchor), so the two go the same way.
  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const onImage = (e: Event) => {
      const img = e.target;
      if (!(img instanceof HTMLImageElement)) return;
      if (atEnd.current) toEnd(el);
      else if (img.getBoundingClientRect().top < el.getBoundingClientRect().top) el.scrollTop += img.offsetHeight;
    };
    // A load doesn't bubble; it's heard on its way down.
    el.addEventListener('load', onImage, true);
    return () => el.removeEventListener('load', onImage, true);
    // The view is another element once an error has shown in its place.
  }, [error, toEnd]);

  const loadedNow = useRef(onLoaded);
  loadedNow.current = onLoaded;
  useLayoutEffect(() => {
    if (loaded) loadedNow.current?.();
  }, [loaded, s.key]);

  const onScroll = () => {
    const el = scroller.current;
    if (!el) return;
    if (endAt.current != null && Math.abs(el.scrollTop - endAt.current) >= 1) endAt.current = null;
    atEnd.current = endAt.current != null || el.scrollHeight - el.scrollTop - el.clientHeight < 40;
    if (el.scrollTop < NEAR_TOP) loadOlder();
    pin();
  };

  /** Your prompts in the view, where each starts and ends from the top of what's scrolled, in the
   *  view's own pixels as its scrollTop is: a card drawn at a zoom has rects on the screen's scale
   *  (zoom.ts `scaleOf`). */
  const promptsIn = (el: HTMLElement) => {
    const box = el.getBoundingClientRect().top;
    const scale = scaleOf(el);
    return [...el.querySelectorAll<HTMLElement>('[data-prompt]')].map((p) => {
      const r = p.getBoundingClientRect();
      return { el: p, key: p.dataset.prompt ?? '', at: (r.top - box) / scale + el.scrollTop, end: (r.bottom - box) / scale + el.scrollTop };
    });
  };
  const padOf = (el: HTMLElement) => parseFloat(getComputedStyle(el).paddingTop) || 0;

  /** Pins the prompt behind what's in view once the whole of it is above the top: one read, or
   *  the one before the pages read. */
  const pin = () => {
    const el = scroller.current;
    if (!el) return;
    const behind = promptsIn(el)
      .filter((p) => p.at < el.scrollTop + padOf(el) + 2)
      .pop();
    const off = behind ? (behind.end < el.scrollTop + 8 ? behind.key : null) : earlier && keyOf(earlier);
    const text = off && promptText.get(off);
    const next = off && text != null ? { key: off, text } : null;
    setPinned((was) => (was?.key === next?.key && was?.text === next?.text ? was : next));
  };

  /** Brings one of your prompts to the top of the view, and marks it for a moment. */
  const bringUp = (el: HTMLElement, to: { el: HTMLElement; key: string; at: number }) => {
    el.scrollTop = to.at - padOf(el);
    jumped.current = { key: to.key, offset: to.at - el.scrollTop };
    // Marked, since one near the end can't come all the way up.
    flash(to.el, 'is-landed');
  };

  /** Brings one of your prompts to the top, reading back to it when it's before the pages read. */
  const goTo = (key: string) => {
    const el = scroller.current;
    if (!el) return;
    const to = promptsIn(el).find((p) => p.key === key);
    if (to) return bringUp(el, to);
    if (before == null) return;
    seeking.current = { key };
    loadOlder();
  };

  /** Brings your previous (-1) or next (1) prompt to the top of the view. */
  const jump = (dir: -1 | 1) => {
    const el = scroller.current;
    if (!el) return;
    const pad = padOf(el);
    const prompts = promptsIn(el);
    const last = jumped.current;
    const was = last && prompts.find((p) => p.key === last.key && Math.abs(p.at - el.scrollTop - last.offset) < 2);
    // From the prompt it brought up last; from the end, where the latest prompt is the one before
    // even while it's in view; else from the top of the view.
    const from = was ? was.at : dir < 0 && atEnd.current ? Infinity : el.scrollTop + pad;
    const to = dir < 0 ? prompts.filter((p) => p.at < from - 2).pop() : prompts.find((p) => p.at > from + 2);
    if (!to) {
      if (dir < 0 && before != null) {
        seeking.current = { key: null };
        loadOlder();
        return;
      }
      jumped.current = null;
      if (dir < 0) el.scrollTop = 0;
      else toEnd(el);
      return;
    }
    bringUp(el, to);
  };
  const jumpNow = useRef(jump);
  jumpNow.current = jump;
  const goToNow = useRef(goTo);
  goToNow.current = goTo;

  // Each page read while looking further up: searched once it's laid out and the view is back
  // where it was.
  useLayoutEffect(() => {
    const want = seeking.current;
    if (!want) return;
    seeking.current = null;
    if (want.key == null) jumpNow.current(-1);
    else goToNow.current(want.key);
  }, [items]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.key !== 'ArrowUp' && e.key !== 'ArrowDown') || !e.metaKey || e.ctrlKey || e.altKey || e.isComposing || e.defaultPrevented) return;
      const el = scroller.current;
      if (!el || el.getClientRects().length === 0) return;
      // Settings and the rename sheet keep the keys while they're open.
      const modal = document.querySelector('[aria-modal="true"]');
      if (modal && !modal.contains(el)) return;
      // In a text box they move its cursor, all but ⌘⇧ in this panel's own box, where selecting
      // to the start of a reply is seldom what's meant.
      const at = document.activeElement as HTMLElement | null;
      const field = at instanceof HTMLInputElement || at instanceof HTMLTextAreaElement || at instanceof HTMLSelectElement || !!at?.isContentEditable;
      if (field && !(e.shiftKey && at.closest('.ca-composer') && el.parentElement?.contains(at))) return;
      // Ahead of the window's own keys, where ⌘↑ would move along the list.
      e.preventDefault();
      e.stopPropagation();
      jumpNow.current(e.key === 'ArrowUp' ? -1 : 1);
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, []);


  const rows = useMemo(() => rowsOf(items), [items]);
  const headings = useMemo(() => headingsOf(items), [items]);
  // Your messages to it still on their way, less the ones its record has now.
  const ahead = useAhead();
  const sends = useMemo(() => ahead.filter((a) => a.change.kind === 'send' && a.change.key === s.key), [ahead, s.key]);
  const { showing, found } = useMemo(() => inRecord(sends, items), [sends, items]);
  // Your prompts' words by key, for the pin: those read, the one before them, and yours on the way.
  const promptText = useMemo(() => {
    const words = new Map<string, string>();
    // A prompt that was only an image has no words of its own.
    const of = (text: string) => snippet(text, 1200) || 'Image';
    if (earlier) words.set(keyOf(earlier), of(earlier.text));
    for (const it of items) if (it.kind === 'you') words.set(keyOf(it), of(it.text));
    for (const a of showing) if (a.change.kind === 'send') words.set(`ahead:${a.id}`, of(a.change.text));
    return words;
  }, [items, earlier, showing]);
  useLayoutEffect(pin, [items, loaded, promptText, earlier]);
  useEffect(() => {
    for (const id of found) delivered(id);
  }, [found]);
  // A message you just sent brings the view to the end, wherever you'd scrolled.
  const shownSends = useRef(0);
  useLayoutEffect(() => {
    const el = scroller.current;
    if (el && showing.length > shownSends.current) toEnd(el);
    shownSends.current = showing.length;
  }, [showing.length, toEnd]);
  // The latest prompt the session took back goes back in the box below, as Claude Code puts it
  // back in its own, while nothing came after it and nothing of yours is on its way.
  const lastItem = items[items.length - 1];
  useEffect(() => {
    if (answeredBelow && lastItem?.kind === 'you' && lastItem.withdrawn && showing.length === 0) putBack(s.key, keyOf(lastItem), lastItem.text);
  }, [answeredBelow, lastItem, showing.length, s.key]);
  // The last call with no answer yet is what's running, or what the session waits on you for.
  const pending = useMemo(() => {
    const answered = new Set(items.filter((it) => it.kind === 'result' && it.id).map((it) => it.id));
    const last = [...items].reverse().find((it) => it.kind !== 'result');
    if (!last || last.kind !== 'tool' || last.output != null || (last.id && answered.has(last.id))) return null;
    return keyOf(last);
  }, [items]);
  const busy: Status[] = ['needs', 'working'];
  const askHere = answeredBelow ? undefined : onOpen;
  const stateOf = (it: TranscriptItem): 'done' | 'running' | 'asks' | 'open' => {
    if (keyOf(it) !== pending || !busy.includes(s.status)) return 'done';
    return s.status === 'needs' ? 'asks' : 'running';
  };

  if (error) {
    return (
      <div className="ca-thread is-fallback">
        <p className="ca-empty">{error}</p>
        {fallback}
      </div>
    );
  }
  return (
    <RunContext.Provider value={run ?? null}>
      <ThreadFind host={{ key: s.key, scroller, roots: FIND_IN, ready: loaded, read: reading.current, before, pageAt, show }}>
        <div className="ca-thread" ref={scroller} onScroll={onScroll}>
          {/* Takes no room: it floats over the top of the view while it has a prompt to show. */}
          <div className="ca-thread-pin">
            {pinned && (
              <button
                type="button"
                className="ca-thread-pinned"
                title="Go to this prompt (⌘⇧↑)"
                // The cursor stays in the box below.
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => goTo(pinned.key)}
              >
                <span className="ca-thread-pintext">{pinned.text}</span>
                <Icon name="arrow-up" size={12} />
              </button>
            )}
          </div>
          {!loaded && <p className="ca-empty">Reading the conversation…</p>}
          {!loaded && <Tip place="session" />}
          {loaded && before == null && items.length > 0 && <div className="ca-thread-start">The start of the session</div>}
          {loaded && before != null && <div className="ca-thread-start">Loading earlier…</div>}
          {rows.map((r) => {
            if (r.kind === 'tools') return <ToolRun key={keyOf(r.calls[0].item)} calls={r.calls} stateOf={stateOf} onOpen={askHere} />;
            const it = r.item;
            if (it.kind === 'tool') return <ToolLine key={keyOf(it)} item={it} result={r.result} state={stateOf(it)} onOpen={askHere} />;
            if (it.kind === 'note') {
              return (
                <div key={keyOf(it)} className="ca-thread-note">
                  {it.text}
                </div>
              );
            }
            const again = it.withdrawn && answeredBelow ? () => putBack(s.key, keyOf(it), it.text, true) : undefined;
            // What Codealong sent by itself says so, apart from yours and the agent's.
            const label = it.kind === 'you' ? 'You' : it.kind === 'codealong' ? 'Codealong' : s.account;
            return <Said key={keyOf(it)} item={it} result={r.result} now={now} label={label} head={headings.headed.has(keyOf(it))} onPutBack={again} onMenu={onMenu} />;
          })}
          {showing.map((a) => (
            <Unsaid key={a.id} ahead={a} onOpen={onOpen} />
          ))}
          {streaming?.text && (
            <Said
              item={{ at: now, kind: 'agent', text: streaming.text, failed: false, cursor: 0, n: 0 }}
              now={now}
              label={`${s.account} · writing`}
              // Yours on their way are above it.
              head={showing.length > 0 || heads(headings.last, 'agent', now)}
              onMenu={onMenu}
            />
          )}
          {streaming && !streaming.text && streaming.tool && !pending && (
            <div className="ca-thread-note">
              <Rich text={`Running \`${streaming.tool.replace(/`/g, '')}\``} now={now} />
            </div>
          )}
          {loaded && items.length === 0 && showing.length === 0 && !streaming?.text && <p className="ca-empty">Nothing said yet.</p>}
          {s.status === 'needs' && !pending && s.excerpt && (
            <div className="ca-thread-tool is-asks">
              <div className="ca-thread-askrow">
                <span>
                  <Rich text={s.excerpt} now={now} />
                </span>
                {/* A question in its reply is answered in the box below; anything else where it runs. */}
                {!s.asks && !answeredBelow && (
                  <button type="button" className="ca-btn ca-btn--secondary ca-btn--sm" onClick={onOpen}>
                    <Icon name="arrow-up-right" size={13} />
                    <span>Open to answer</span>
                  </button>
                )}
              </div>
            </div>
          )}
        </div>
      </ThreadFind>
    </RunContext.Provider>
  );
}
