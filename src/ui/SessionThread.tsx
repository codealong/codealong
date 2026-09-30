// A session's conversation in its panel: your prompts, the agent's replies, and the tools it ran
// with what they gave back, read from the session's own record a page at a time (transcript.rs).
// It opens at the newest and loads older pages as you scroll up, holding your place; new steps
// come in at the bottom as the session goes on, and the view follows them if you're at the end.
// A tool call is a line; click it for its input and output. A run of calls folds into one line
// that keeps its latest call in view.
//
// A message you sent shows at the end as it goes (ahead.ts): sending, then sent, until the
// session's record has it and it shows from there. One that went but that the session shows no
// sign of taking, well after, says so, with a way to open the session and look.
//
// A prompt the session took back before its agent saw it (Claude Code does when a turn is
// stopped before any reply, claude.rs) stays in view, faint, saying so. Claude Code puts it back
// in its own input box, and the latest one goes back in the box below too, once.

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { delivered, dismiss, inRecord, useAhead, type Ahead } from '../ahead';
import { onHostedStream, sessionNewer, sessionPage, sessionStream } from '../api';
import { putBack } from '../drafts';
import { absolute, agoPhrase } from '../format';
import type { HostedStream, Session, Status, TranscriptItem } from '../types';
import { CodeBlock } from './CopyButton';
import { DescriptionView } from './DescriptionEditor';
import { Markdown } from './Markdown';
import { Icon, Rich, StatusGlyph } from './primitives';
import { RunContext, type RunPlace } from './RunButton';

/** Loads the page before once you're this close to the top. */
const NEAR_TOP = 240;
/** Tool calls in a row before they fold. */
const FOLD_AT = 3;

type Row =
  | { kind: 'item'; item: TranscriptItem; result?: TranscriptItem }
  | { kind: 'tools'; calls: { item: TranscriptItem; result?: TranscriptItem }[] };

const keyOf = (it: TranscriptItem) => `${it.cursor}:${it.n}`;

/** The items as rows: results joined to their calls, runs of calls folded. */
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
      run.push({ item: it, result: it.id ? results.get(it.id) : undefined });
      continue;
    }
    flush();
    rows.push({ kind: 'item', item: it });
  }
  flush();
  return rows;
}

/** The icon for a tool, by the start of its label. */
function toolIcon(label: string): string {
  const name = label.split(' · ')[0].toLowerCase();
  if (/bash|shell|exec|command/.test(name)) return 'terminal';
  if (/edit|write|patch|notebook/.test(name)) return 'code';
  if (/read|view/.test(name)) return 'file';
  if (/grep|glob|search|find/.test(name)) return 'search';
  if (/agent|task/.test(name)) return 'sparkles';
  return 'bolt';
}

function ToolLine({ item, result, state, onOpen }: { item: TranscriptItem; result?: TranscriptItem; state: 'done' | 'running' | 'asks' | 'open'; onOpen?: () => void }) {
  const [open, setOpen] = useState(false);
  const output = item.output ?? result?.output;
  const failed = item.failed || !!result?.failed;
  const detail = item.input || output;
  return (
    <div className={['ca-thread-tool', failed && 'is-failed', state === 'asks' && 'is-asks'].filter(Boolean).join(' ')}>
      <button type="button" className="ca-thread-toolhead" aria-expanded={open} disabled={!detail} onClick={() => setOpen(!open)}>
        <Icon name={toolIcon(item.text)} size={13} />
        <span className="ca-thread-toollabel">
          <Rich text={item.text} now={0} />
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
      {state === 'asks' && onOpen && (
        <div className="ca-thread-askrow">
          <span>It’s waiting for your approval.</span>
          <button type="button" className="ca-btn ca-btn--secondary ca-btn--sm" onClick={onOpen}>
            <Icon name="arrow-up-right" size={13} />
            <span>Open to answer</span>
          </button>
        </div>
      )}
      {open && (
        <div className="ca-thread-tooldetail">
          {item.input && <CodeBlock text={item.input} className="ca-thread-in" run={item.shell && state !== 'asks' ? { folder: item.cwd } : undefined} />}
          {output && <CodeBlock text={output} className={['ca-thread-out', failed && 'is-failed'].filter(Boolean).join(' ')} />}
        </div>
      )}
    </div>
  );
}

function ToolRun({ calls, stateOf, onOpen }: { calls: { item: TranscriptItem; result?: TranscriptItem }[]; stateOf: (it: TranscriptItem) => 'done' | 'running' | 'asks' | 'open'; onOpen?: () => void }) {
  const [open, setOpen] = useState(false);
  const last = calls[calls.length - 1];
  const earlier = calls.slice(0, -1);
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

function Said({ item, now, label, onPutBack }: { item: TranscriptItem; now: number; label: ReactNode; onPutBack?: () => void }) {
  return (
    <div className={['ca-thread-said', `is-${item.kind}`, item.withdrawn && 'is-withdrawn'].filter(Boolean).join(' ')}>
      <div className="ca-thread-meta">
        <span>{label}</span>
        <span className="ca-meta" title={absolute(item.at)}>
          {item.withdrawn ? `Taken back · ${agoPhrase(item.at, now)}` : agoPhrase(item.at, now)}
        </span>
      </div>
      {item.kind === 'you' ? <YourText text={item.text} /> : <Markdown text={item.text} className="ca-thread-agent" />}
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
    <div className={['ca-thread-said', 'is-you', 'is-ahead', ahead.doneAt == null && 'is-going', ahead.unconfirmed && 'is-unconfirmed'].filter(Boolean).join(' ')}>
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
}

export function SessionThread({ session: s, now, onOpen, fallback, onLoaded, answeredBelow, run }: SessionThreadProps) {
  const [items, setItems] = useState<TranscriptItem[]>([]);
  const [before, setBefore] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);
  const after = useRef(0);
  const loading = useRef(false);
  const scroller = useRef<HTMLDivElement>(null);
  // How the view should land after the next render: at the end, or where it was from the end.
  const anchor = useRef<{ kind: 'end' } | { kind: 'keep'; fromEnd: number } | null>(null);
  const atEnd = useRef(true);

  // A session Codealong runs streams its reply before its record has it: shown at the end
  // until the record catches up at the end of the message.
  const [stream, setStream] = useState<HostedStream | null>(null);
  useEffect(() => {
    setStream(null);
    if (!s.hosted) return;
    let live = true;
    sessionStream(s.key)
      .then((st) => live && setStream(st))
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
    setItems([]);
    setBefore(null);
    setLoaded(false);
    setError(null);
    loading.current = true;
    sessionPage(s.key, null)
      .then((p) => {
        if (read !== reading.current) return;
        anchor.current = { kind: 'end' };
        setItems(p.items);
        setBefore(p.before);
        after.current = p.after;
        landed.current = read;
        setLoaded(true);
      })
      .catch((e: unknown) => read === reading.current && (setError(String(e).replace(/^Error: /, '')), setLoaded(true)))
      .finally(() => read === reading.current && (loading.current = false));
  }, [s.key, s.continues]);

  const loadOlder = useCallback(() => {
    if (loading.current || before == null) return;
    loading.current = true;
    const read = reading.current;
    const el = scroller.current;
    sessionPage(s.key, before)
      .then((p) => {
        if (read !== reading.current) return;
        anchor.current = { kind: 'keep', fromEnd: el ? el.scrollHeight - el.scrollTop : 0 };
        setItems((old) => [...p.items, ...old]);
        setBefore(p.before);
      })
      .catch(() => {})
      .finally(() => read === reading.current && (loading.current = false));
  }, [s.key, before]);

  // As the session goes on: what's new since the last read. OpenCode rereads a message still
  // streaming, so an item already here is replaced by its place.
  useEffect(() => {
    const read = reading.current;
    if (!loaded || error || landed.current !== read) return;
    let live = true;
    sessionNewer(s.key, after.current)
      .then((p) => {
        if (!live || read !== reading.current || p.items.length === 0) return;
        after.current = p.after;
        if (atEnd.current) anchor.current = { kind: 'end' };
        const fresh = new Set(p.items.map((it) => it.cursor));
        setItems((old) => [...old.filter((it) => !fresh.has(it.cursor)), ...p.items]);
      })
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [s.lastActivity, s.status, loaded, error, s.key]);

  useLayoutEffect(() => {
    const el = scroller.current;
    const a = anchor.current;
    if (!el || !a) return;
    anchor.current = null;
    el.scrollTop = a.kind === 'end' ? el.scrollHeight : el.scrollHeight - a.fromEnd;
    // A first page too short to scroll can't be scrolled up to load more: load it now.
    if (el.scrollHeight <= el.clientHeight + NEAR_TOP) loadOlder();
  }, [items, loadOlder, stream]);

  const loadedNow = useRef(onLoaded);
  loadedNow.current = onLoaded;
  useLayoutEffect(() => {
    if (loaded) loadedNow.current?.();
  }, [loaded, s.key]);

  const onScroll = () => {
    const el = scroller.current;
    if (!el) return;
    atEnd.current = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
    if (el.scrollTop < NEAR_TOP) loadOlder();
  };


  const rows = useMemo(() => rowsOf(items), [items]);
  // Your messages to it still on their way, less the ones its record has now.
  const ahead = useAhead();
  const sends = useMemo(() => ahead.filter((a) => a.change.kind === 'send' && a.change.key === s.key), [ahead, s.key]);
  const { showing, found } = useMemo(() => inRecord(sends, items), [sends, items]);
  useEffect(() => {
    for (const id of found) delivered(id);
  }, [found]);
  // A message you just sent brings the view to the end, wherever you'd scrolled.
  const shownSends = useRef(0);
  useLayoutEffect(() => {
    const el = scroller.current;
    if (el && showing.length > shownSends.current) el.scrollTop = el.scrollHeight;
    shownSends.current = showing.length;
  }, [showing.length]);
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
      <div className="ca-thread" ref={scroller} onScroll={onScroll}>
        {!loaded && <p className="ca-empty">Reading the conversation…</p>}
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
          return <Said key={keyOf(it)} item={it} now={now} label={it.kind === 'you' ? 'You' : s.account} onPutBack={again} />;
        })}
        {showing.map((a) => (
          <Unsaid key={a.id} ahead={a} onOpen={onOpen} />
        ))}
        {streaming?.text && <Said item={{ at: now, kind: 'agent', text: streaming.text, failed: false, cursor: 0, n: 0 }} now={now} label={`${s.account} · writing`} />}
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
    </RunContext.Provider>
  );
}
