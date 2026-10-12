// Finding words in a session's conversation: ⌘F in its panel (the keyboard there, or a click there
// last: findKeys.ts) opens a bar above the conversation (SessionThread.tsx), wherever the panel
// shows, in place of the prompt pinned over its top, and what's typed there is marked wherever the
// conversation says it, the one you're on more strongly, and brought into view. ⏎ goes to the
// match before, back through the conversation as ⌘⇧↑ goes back through your prompts, and ⇧⏎ to
// the one after; ⌘G and ⇧⌘G do the same from anywhere in the window, and Escape closes the bar and
// gives the keyboard back to where it was.
//
// It finds what the conversation shows: your prompts, the replies, notes and the tool calls'
// lines, a run of calls folded into one line opening to show the ones it finds (`FindContext`),
// and a call's input and output once you've opened it. What's laid out is searched as it reads
// (the thread's own text, marked through the CSS highlight API, so nothing in it is rendered
// again); the pages before it are read in the background as the search starts and counted from
// their items (threadFind.ts), so the count is the whole conversation's, and going back past the
// first match laid out lays out the pages up to the one that has the next.

import { createContext, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode, type RefObject } from 'react';
import { useLatest } from '../latest';
import { findsAt } from '../findKeys';
import { chainFrom, countWords, findPattern, pageHits } from '../threadFind';
import type { TranscriptPage } from '../types';
import { scaleOf } from '../zoom';
import { Button, Icon } from './primitives';

/** What's being found while the bar is open, for what folds lines away to show the ones it finds. */
export const FindContext = createContext<RegExp | null>(null);

/** The conversation the bar finds in, as SessionThread.tsx has it. */
export interface FindHost {
  key: string;
  /** The conversation's own element, which scrolls. */
  scroller: RefObject<HTMLDivElement | null>;
  /** The elements in it whose text is the conversation's, as a selector. */
  roots: string;
  /** Its first page is laid out. */
  ready: boolean;
  /** Changes when it's read from its start again, which makes every cursor a new one. */
  read: number;
  /** Where the pages laid out begin; null at the session's start. */
  before: number | null;
  /** Reads the page before a cursor, each once. */
  pageAt: (key: string, at: number) => Promise<TranscriptPage>;
  /** Lays out pages read back from `before`, nearest first, each with the cursor it was read at;
   *  false when that's no longer where they begin, or a page is on its way in. */
  show: (pages: { at: number; page: TranscriptPage }[]) => boolean;
}

/** A match laid out: its text, and which of its root's matches it is, to know it again once the
 *  conversation has changed around it. */
interface Match {
  range: Range;
  root: Element;
  n: number;
}

/** Inside a root but not the conversation's words: the buttons on a code block, what it says
 *  when a run fails. */
const SKIP = '.ca-codeblock-tools, .ca-codeblock-note, [aria-hidden="true"]';
/** How long typing rests before a query found only further back goes there. */
const SETTLE_MS = 300;

/** Every match laid out, in the order they read. A match can run across the elements a line is
 *  made of (a word in bold, a link), but not from one root into the next. */
function matchesIn(scroller: HTMLElement, roots: string, pattern: RegExp): Match[] {
  const out: Match[] = [];
  for (const root of scroller.querySelectorAll(roots)) {
    const nodes: Text[] = [];
    const starts: number[] = [];
    let text = '';
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
      acceptNode: (node) => {
        const skip = node.parentElement?.closest(SKIP);
        return skip && root.contains(skip) ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT;
      },
    });
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      nodes.push(node as Text);
      starts.push(text.length);
      text += (node as Text).data;
    }
    let n = 0;
    let first = 0;
    for (const m of text.matchAll(pattern)) {
      const from = m.index;
      const to = from + m[0].length;
      while (first + 1 < nodes.length && starts[first + 1] <= from) first++;
      let last = first;
      while (last + 1 < nodes.length && starts[last + 1] < to) last++;
      const range = document.createRange();
      range.setStart(nodes[first], from - starts[first]);
      range.setEnd(nodes[last], to - starts[last]);
      out.push({ range, root, n: n++ });
    }
  }
  return out;
}

const indexOf = (all: Match[], m: Match | null) => (m ? all.findIndex((x) => x.root === m.root && x.n === m.n) : -1);

/** The page's highlight registry is one for the whole window: the bar that painted last owns it. */
let painter: object | null = null;
function paint(owner: object, all: Match[], at: Match | null) {
  const reg = typeof CSS !== 'undefined' ? CSS.highlights : undefined;
  if (!reg) return;
  painter = owner;
  reg.set('ca-find', new Highlight(...all.map((m) => m.range)));
  if (!at) return void reg.delete('ca-find-at');
  const mark = new Highlight(at.range);
  mark.priority = 1;
  reg.set('ca-find-at', mark);
}
function unpaint(owner: object) {
  const reg = typeof CSS !== 'undefined' ? CSS.highlights : undefined;
  if (!reg || painter !== owner) return;
  painter = null;
  reg.delete('ca-find');
  reg.delete('ca-find-at');
}

/** The visible conversation ⌘G and ⇧⌘G step through: the one whose panel has the keyboard, else
 *  the first. ⌘F goes where you are (findKeys.ts). */
function threadFor(): Element | null {
  const shown = [...document.querySelectorAll('.ca-thread')].filter((el) => el.getClientRects().length > 0);
  const at = document.activeElement;
  return shown.find((el) => at && el.parentElement?.contains(at)) ?? shown[0] ?? null;
}

export function ThreadFind({ host, children }: { host: FindHost; children: ReactNode }) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const pattern = useMemo(() => (open ? findPattern(query) : null), [open, query]);
  const input = useRef<HTMLInputElement>(null);
  // Where the keyboard was before ⌘F, for Escape to give it back.
  const back = useRef<HTMLElement | null>(null);
  const owner = useRef({}).current;
  const hostNow = useLatest(host);
  const patternNow = useLatest(pattern);

  // The matches laid out, the one you're on, and what the bar says of them.
  const matches = useRef<Match[]>([]);
  const current = useRef<Match | null>(null);
  const [view, setView] = useState<{ at: number | null; shown: number }>({ at: null, shown: 0 });
  // Going back past the first match laid out, from the one you were on (null: from the end), and
  // whether the pages it asked for are being laid out.
  const seek = useRef<{ from: Match | null; showing?: boolean } | null>(null);
  const [looking, setLooking] = useState(false);
  // `refresh`, below, for what comes before it.
  const refreshNow = useRef<(near: boolean) => void>(() => {});

  // The pages before what's laid out, read in the background once there's something to find,
  // and how often each says what's being found, worked out once a query.
  const scannedNow = useRef<ReadonlyMap<number, TranscriptPage>>(new Map());
  const [scanned, setScanned] = useState(scannedNow.current);
  const [stalled, setStalled] = useState(false);
  const counts = useRef(new WeakMap<TranscriptPage, { pattern: RegExp; hits: number }>());
  const hitsIn = useCallback((pattern: RegExp, page: TranscriptPage) => {
    const known = counts.current.get(page);
    if (known?.pattern === pattern) return known.hits;
    const hits = pageHits(pattern, page.items);
    counts.current.set(page, { pattern, hits });
    return hits;
  }, []);

  /** Brings a match into view, below the prompt pinned over the top and clear of the bottom. */
  const reveal = useCallback(
    (m: Match) => {
      const el = hostNow.current.scroller.current;
      if (!el || m.range.getClientRects().length === 0) return;
      const box = el.getBoundingClientRect();
      const r = m.range.getBoundingClientRect();
      const scale = scaleOf(el);
      const top = (r.top - box.top) / scale + el.scrollTop;
      const bottom = (r.bottom - box.top) / scale + el.scrollTop;
      const pinned = el.querySelector<HTMLElement>('.ca-thread-pinned')?.offsetHeight ?? 0;
      if (top >= el.scrollTop + pinned + 8 && bottom <= el.scrollTop + el.clientHeight - 16) return;
      el.scrollTop = Math.max(0, top - el.clientHeight / 3);
    },
    [hostNow],
  );

  const goTo = useCallback(
    (i: number) => {
      const m = matches.current[i];
      if (!m) return;
      current.current = m;
      setView({ at: i, shown: matches.current.length });
      paint(owner, matches.current, m);
      reveal(m);
    },
    [owner, reveal],
  );

  /** Lays out the pages back to the nearest one that says it, or waits for the next to be read. */
  const trySeek = useCallback(() => {
    const s = seek.current;
    const p = patternNow.current;
    if (!s || s.showing || !p) return;
    const h = hostNow.current;
    const chain = chainFrom(h.before, scannedNow.current);
    const to = chain.pages.findIndex((c) => hitsIn(p, c.page) > 0);
    setLooking(true);
    if (to >= 0) {
      // A page on its way in comes first; this goes on once it's laid out.
      if (!h.show(chain.pages.slice(0, to + 1))) return void window.setTimeout(trySeek, 100);
      s.showing = true;
      // Found again as the pages change the conversation; this in case they show nothing new.
      window.setTimeout(() => refreshNow.current(false), 150);
      return;
    }
    // Nothing further back says it: where you are is the first.
    if (chain.whole) {
      seek.current = null;
      setLooking(false);
    }
  }, [hitsIn, hostNow, patternNow]);

  /** Finds the matches laid out again. `near` (a new query) goes to the one nearest where you're
   *  reading: the last that starts above the bottom of the view, else the first; otherwise the
   *  one you were on stays, and a way back that was waiting on a page goes on. */
  const refresh = useCallback(
    (near: boolean) => {
      const el = hostNow.current.scroller.current;
      const p = patternNow.current;
      if (!el || !p) {
        matches.current = [];
        current.current = null;
        setView({ at: null, shown: 0 });
        unpaint(owner);
        return;
      }
      const found = matchesIn(el, hostNow.current.roots, p);
      matches.current = found;
      const s = seek.current;
      const from = s?.showing ? (s.from ? indexOf(found, s.from) : found.length) : -1;
      if (s?.showing && from > 0) {
        seek.current = null;
        setLooking(false);
        return goTo(from - 1);
      }
      if (s?.showing && from === 0) {
        // What was laid out says it only where it doesn't show (a call's output): further back.
        s.showing = false;
        current.current = found[0] ?? null;
        setView({ at: found.length > 0 ? 0 : null, shown: found.length });
        paint(owner, found, current.current);
        return trySeek();
      }
      if (s?.showing && s.from) {
        // The match it went back from is gone: it stops where it is.
        seek.current = null;
        setLooking(false);
      }
      if (near) {
        if (found.length === 0) {
          current.current = null;
          setView({ at: null, shown: 0 });
          return paint(owner, found, null);
        }
        const box = el.getBoundingClientRect();
        const above = found.filter((m) => m.range.getClientRects().length > 0 && m.range.getBoundingClientRect().top <= box.bottom);
        return goTo(above.length > 0 ? found.indexOf(above[above.length - 1]) : 0);
      }
      let i = indexOf(found, current.current);
      if (i < 0 && found.length > 0) i = Math.min(view.at ?? found.length - 1, found.length - 1);
      current.current = found[i] ?? null;
      setView({ at: i < 0 ? null : i, shown: found.length });
      paint(owner, found, current.current);
    },
    [goTo, hostNow, owner, patternNow, trySeek, view.at],
  );
  refreshNow.current = refresh;

  // The bar takes its room from the top of the conversation, which keeps what's at its bottom in
  // view (the end, where you usually are) as the bar opens and closes.
  const barEl = useRef<HTMLDivElement>(null);
  const barHeight = useRef(0);
  useLayoutEffect(() => {
    const el = host.scroller.current;
    const h = open ? (barEl.current?.offsetHeight ?? 0) : 0;
    if (el && h !== barHeight.current) el.scrollTop += h - barHeight.current;
    barHeight.current = h;
  }, [open, host.scroller]);

  // A new query: found again, and the nearest match brought into view.
  useLayoutEffect(() => {
    seek.current = null;
    setLooking(false);
    refreshNow.current(true);
  }, [pattern, refreshNow]);

  // As the conversation changes (pages, new steps, a call opened, a reply streaming in), the
  // matches are found again and the one you're on stays.
  useEffect(() => {
    const el = host.scroller.current;
    if (!el || !pattern) return;
    let timer: number | null = null;
    const observer = new MutationObserver(() => {
      if (timer != null) return;
      timer = window.setTimeout(() => {
        timer = null;
        refreshNow.current(false);
      }, 40);
    });
    observer.observe(el, { childList: true, subtree: true, characterData: true });
    return () => {
      observer.disconnect();
      if (timer != null) window.clearTimeout(timer);
    };
  }, [host.scroller, host.ready, pattern, refreshNow]);

  // Read from its start again: what was read before is of another record.
  useEffect(() => {
    scannedNow.current = new Map();
    setScanned(scannedNow.current);
    setStalled(false);
  }, [host.key, host.read]);

  // The pages before what's laid out, read one at a time while there's something to find.
  const finding = !!pattern && host.ready;
  useEffect(() => {
    if (!finding) return;
    let live = true;
    void (async () => {
      for (;;) {
        const h = hostNow.current;
        const chain = chainFrom(h.before, scannedNow.current);
        const at = chain.pages.length > 0 ? chain.pages[chain.pages.length - 1].page.before : h.before;
        if (chain.whole || at == null) return;
        const page = await h.pageAt(h.key, at).catch(() => null);
        if (!live) return;
        // Counted as far as it could read; the bar says "so far" from then on, and a way back
        // waiting for more stops where it is.
        if (!page) {
          setStalled(true);
          if (seek.current && !seek.current.showing) {
            seek.current = null;
            setLooking(false);
          }
          return;
        }
        scannedNow.current = new Map(scannedNow.current).set(at, page);
        setScanned(scannedNow.current);
      }
    })();
    return () => {
      live = false;
    };
  }, [finding, host.read, hostNow]);

  // A way back waiting for pages to be read goes on as they come.
  useEffect(() => {
    if (seek.current) trySeek();
  }, [scanned, trySeek]);

  // What's further back: how often the pages read there say it, and whether there's more to read.
  const chain = useMemo(() => chainFrom(host.before, scanned), [host.before, scanned]);
  const earlier = useMemo(() => (pattern ? chain.pages.reduce((n, c) => n + hitsIn(pattern, c.page), 0) : 0), [pattern, chain, hitsIn]);
  const partial = !!pattern && !chain.whole;
  const reading = partial && !stalled;

  // A query found only further back goes there once typing rests.
  useEffect(() => {
    if (!pattern || view.shown > 0 || seek.current || (earlier === 0 && !reading)) return;
    const t = window.setTimeout(() => {
      if (patternNow.current !== pattern || matches.current.length > 0 || seek.current) return;
      seek.current = { from: null };
      trySeek();
    }, SETTLE_MS);
    return () => window.clearTimeout(t);
  }, [pattern, view.shown, earlier, reading, patternNow, trySeek]);

  /** On to the match before (-1) or after (1), laying out pages further back when the one before
   *  isn't laid out yet. */
  const step = useCallback(
    (dir: -1 | 1) => {
      const i = indexOf(matches.current, current.current);
      if (dir > 0) {
        if (i >= 0 && i + 1 < matches.current.length) goTo(i + 1);
        return;
      }
      if (i > 0) return goTo(i - 1);
      if (seek.current) return;
      seek.current = { from: i === 0 ? current.current : null };
      trySeek();
    },
    [goTo, trySeek],
  );

  const close = useCallback(() => {
    setOpen(false);
    seek.current = null;
    setLooking(false);
    unpaint(owner);
    const to = back.current;
    back.current = null;
    if (to?.isConnected) to.focus();
  }, [owner]);

  // ⌘F opens it (or takes the keyboard back to it) when it's for this conversation's panel
  // (findKeys.ts), and ⌘G and ⇧⌘G step while it's open, ahead of the window's own keys. ⇧⌘F is
  // the window's (it expands the panel in Sessions and PRs).
  const openNow = useLatest(open);
  const stepNow = useLatest(step);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.isComposing || !e.metaKey || e.ctrlKey || e.altKey) return;
      const key = e.key.toLowerCase();
      if (!(key === 'f' && !e.shiftKey) && key !== 'g') return;
      const el = hostNow.current.scroller.current;
      if (!el || (key === 'f' ? !findsAt(el) : el !== threadFor())) return;
      // Settings and the rename sheet keep the keys while they're open.
      const modal = document.querySelector('[aria-modal="true"]');
      if (modal && !modal.contains(el)) return;
      if (key === 'g') {
        if (!openNow.current || !patternNow.current) return;
        e.preventDefault();
        stepNow.current(e.shiftKey ? 1 : -1);
        return;
      }
      e.preventDefault();
      if (!openNow.current) back.current = document.activeElement instanceof HTMLElement && document.activeElement !== document.body ? document.activeElement : null;
      setOpen(true);
      input.current?.focus();
      input.current?.select();
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [hostNow, openNow, patternNow, stepNow]);

  // Opened: the keyboard goes to the bar, with what was found last ready to type over.
  useLayoutEffect(() => {
    if (!open) return;
    input.current?.focus();
    input.current?.select();
  }, [open]);

  useEffect(() => () => unpaint(owner), [owner]);

  const at = view.at == null ? null : earlier + view.at;
  const total = earlier + view.shown;
  // On the first match there is, or with none to go back to.
  const atFirst = (view.at === 0 && earlier === 0 && !reading) || (total === 0 && !reading);
  return (
    <FindContext.Provider value={pattern}>
      {open && (
        <div className="ca-thread-find" role="search" ref={barEl}>
          <label className="ca-thread-findfield">
            <Icon name="search" size={12} />
            <input
              ref={input}
              value={query}
              placeholder="Find in the conversation"
              aria-label="Find in the conversation"
              spellCheck={false}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => {
                if (e.nativeEvent.isComposing || e.metaKey || e.ctrlKey || e.altKey) return;
                if (e.key === 'Enter') {
                  e.preventDefault();
                  step(e.shiftKey ? 1 : -1);
                } else if (e.key === 'Escape') {
                  // The bar's, not the window's (which would put the panel or its window away).
                  e.preventDefault();
                  close();
                }
              }}
            />
          </label>
          {pattern && (
            <span className="ca-thread-findcount" role="status">
              {countWords({ at, total, partial, reading, looking })}
            </span>
          )}
          {/* The cursor stays in the field. */}
          <Button variant="ghost" size="sm" icon="chevron-up" title="Earlier match (⏎)" aria-label="Earlier match" disabled={!pattern || looking || atFirst} onMouseDown={(e) => e.preventDefault()} onClick={() => step(-1)} />
          <Button variant="ghost" size="sm" icon="chevron-down" title="Later match (⇧⏎)" aria-label="Later match" disabled={!pattern || looking || view.at == null || view.at + 1 >= view.shown} onMouseDown={(e) => e.preventDefault()} onClick={() => step(1)} />
          <Button variant="ghost" size="sm" icon="x" title="Close (esc)" aria-label="Close find" onClick={close} />
        </div>
      )}
      {children}
    </FindContext.Provider>
  );
}
