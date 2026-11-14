// A session's conversation in the window's find (WindowFind.tsx): ⌘F finds in it as in the rest
// of the window, and further back than it's laid out. It's one of the find's sources: what's laid
// out is found as it reads (the elements `roots` names, SessionThread.tsx `FIND_IN`, not the
// buttons and times around them); the pages before it are read in the background as a search
// starts and counted from their items (threadFind.ts), so the count is the whole conversation's;
// and going back past its first match laid out lays out the pages up to the one that has the
// next. A run of calls folded into one line opens to show the ones it finds (`FindContext`), and a
// call's input and output are found once you've opened it. A match is brought into view below the
// prompt pinned over the conversation's top. A session that a search (the main window's, or the
// quick panel's) found by what was said in it shows with the find open on the words it found, at
// the nearest match in its conversation, the keyboard staying where it was (`asked`).

import { useCallback, useContext, useEffect, useLayoutEffect, useMemo, useRef, useState, type RefObject } from 'react';
import { useLatest } from '../latest';
import { chainFrom, pageHits } from '../threadFind';
import type { TranscriptPage } from '../types';
import { spansIn } from '../windowFind';
import { FIND_REVEAL, FindContext, scrollToRange, useWindowFinder, type FindSource, type FoundRange } from './WindowFind';

/** The conversation the find finds in, as SessionThread.tsx has it. */
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

/** What a search that found the session by what was said in it asks the find to find: `words`
 *  (threadFind.ts `findFor`), and `n`, which a window changes to ask again for the same words (the
 *  quick panel showing the session once more). */
export interface FindAsk {
  words: string;
  n: number;
}

/** Inside a root but not the conversation's words: the buttons on a code block, what it says
 *  when a run fails. */
const SKIP = '.ca-codeblock-tools, .ca-codeblock-note, [aria-hidden="true"]';
/** What a line of a message is laid out in: text in two of them reads with a break between it, as
 *  the pages further back are counted (threadFind.ts), where their text nodes would run together. */
const BLOCK = 'p, li, pre, blockquote, td, th, div';

/** Hands `each` every match laid out, in the order they read, until it says it has enough. A
 *  match can run across the elements a line is made of (a word in bold, a link), and from one
 *  paragraph or item into the next, as the words read across a line break, but not from one root
 *  into the next. */
function matchesIn(scroller: HTMLElement, roots: string, pattern: RegExp, each: (found: FoundRange) => boolean): void {
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
    let block: Element | null = null;
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      const its = node.parentElement?.closest(BLOCK) ?? null;
      if (nodes.length > 0 && its !== block) text += '\n';
      block = its;
      nodes.push(node as Text);
      starts.push(text.length);
      text += (node as Text).data;
    }
    let n = 0;
    for (const s of spansIn(text, starts, pattern)) {
      const range = document.createRange();
      range.setStart(nodes[s.first], s.from);
      range.setEnd(nodes[s.last], s.to);
      if (!each({ range, key: root, n: n++ })) return;
    }
  }
}

/** Makes the conversation `host` lays out one of the window's find's sources, asked to find
 *  `asked` when a search found it by what was said in it. */
export function useThreadFind(host: FindHost, asked?: FindAsk | null): void {
  const finder = useWindowFinder();
  const pattern = useContext(FindContext);
  const hostNow = useLatest(host);
  const patternNow = useLatest(pattern);

  // The pages before what's laid out, read in the background once there's something to find,
  // and how often each says what's being found, worked out once a query.
  const scannedNow = useRef<ReadonlyMap<number, TranscriptPage>>(new Map());
  const [scanned, setScanned] = useState(scannedNow.current);
  const [stalled, setStalled] = useState(false);
  const stalledNow = useLatest(stalled);
  const counts = useRef(new WeakMap<TranscriptPage, { pattern: RegExp; hits: number }>());
  const hitsIn = useCallback((p: RegExp, page: TranscriptPage) => {
    const known = counts.current.get(page);
    if (known?.pattern === p) return known.hits;
    const hits = pageHits(p, page.items);
    counts.current.set(page, { pattern: p, hits });
    return hits;
  }, []);

  // The find waits on this conversation to lay out what's further back (`seek`), until it has or
  // there's nothing there to.
  const wanted = useRef(false);
  const trySeekNow = useRef<() => void>(() => {});
  const source = useMemo<FindSource>(
    () => ({
      el: () => hostNow.current.scroller.current,
      hits: (p, each) => {
        const el = hostNow.current.scroller.current;
        if (el) matchesIn(el, hostNow.current.roots, p, each);
      },
      earlier: 0,
      partial: false,
      reading: false,
      seek: () => {
        wanted.current = true;
        trySeekNow.current();
      },
    }),
    [hostNow],
  );
  useEffect(() => finder?.register(source), [finder, source]);

  /** Lays out the pages back to the nearest one that says it, or waits for the next to be read. */
  const trySeek = useCallback(() => {
    const p = patternNow.current;
    if (!wanted.current || !p) return;
    const h = hostNow.current;
    const chain = chainFrom(h.before, scannedNow.current);
    const to = chain.pages.findIndex((c) => hitsIn(p, c.page) > 0);
    if (to >= 0) {
      // A page on its way in comes first; this goes on once it's laid out.
      if (!h.show(chain.pages.slice(0, to + 1))) return void window.setTimeout(() => trySeekNow.current(), 100);
      wanted.current = false;
      finder?.shown(source);
      return;
    }
    // Nothing further back says it, or nothing more can be read to know.
    if (chain.whole || stalledNow.current) {
      wanted.current = false;
      finder?.noneBefore(source);
    }
  }, [finder, source, hitsIn, hostNow, patternNow, stalledNow]);
  trySeekNow.current = trySeek;

  // A new query, or none: the way back the find asked for is done with.
  useEffect(() => {
    wanted.current = false;
  }, [pattern]);

  // Read from its start again: what was read before is of another record.
  useEffect(() => {
    scannedNow.current = new Map();
    setScanned(scannedNow.current);
    setStalled(false);
  }, [host.key, host.read]);

  // The pages before what's laid out, read one at a time while there's something to find and the
  // conversation shows.
  const finding = !!pattern && host.ready;
  useEffect(() => {
    if (!finding || !hostNow.current.scroller.current?.getClientRects().length) return;
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

  // A way back waiting for pages to be read goes on as they come, or stops once none can be.
  useEffect(() => {
    trySeekNow.current();
  }, [scanned, stalled]);

  // What's further back: how often the pages read there say it, and whether there's more to read.
  const chain = useMemo(() => chainFrom(host.before, scanned), [host.before, scanned]);
  const earlier = useMemo(() => (pattern ? chain.pages.reduce((n, c) => n + hitsIn(pattern, c.page), 0) : 0), [pattern, chain, hitsIn]);
  const partial = !!pattern && !chain.whole;
  const reading = partial && !stalled;
  // Told before the find reads the window for a new query (its effects run after this one's).
  useLayoutEffect(() => {
    if (source.earlier === earlier && source.partial === partial && source.reading === reading) return;
    source.earlier = earlier;
    source.partial = partial;
    source.reading = reading;
    finder?.changed();
  }, [finder, source, earlier, partial, reading]);

  // A match brought into view: below the prompt pinned over the top and clear of the bottom, then
  // the conversation itself into view in whatever scrolls around it.
  useEffect(() => {
    const el = hostNow.current.scroller.current;
    if (!el) return;
    const reveal = (e: Event) => {
      e.preventDefault();
      const range = (e as CustomEvent<Range>).detail;
      const pinned = el.querySelector<HTMLElement>('.ca-thread-pinned')?.offsetHeight ?? 0;
      scrollToRange(range, el, pinned);
    };
    el.addEventListener(FIND_REVEAL, reveal);
    return () => el.removeEventListener(FIND_REVEAL, reveal);
  }, [host.key, host.ready, hostNow]);

  // Asked to find something: the find opens on the words and goes to their nearest match here,
  // once the conversation is laid out if it isn't yet. Asked again only when the words or `n`
  // change, so a find closed stays closed as the conversation is read again.
  const askWords = asked?.words;
  const askN = asked?.n;
  const askedLast = useRef<string | null>(null);
  const nearOnReady = useRef(false);
  const here = useCallback(() => {
    const at = hostNow.current.scroller.current;
    return at?.closest('[data-find]') ?? at;
  }, [hostNow]);
  useEffect(() => {
    if (!askWords || askN === undefined || !finder) return;
    const ask = `${askN}\u0000${askWords}`;
    if (askedLast.current === ask) return;
    askedLast.current = ask;
    nearOnReady.current = !hostNow.current.ready;
    finder.ask(askWords, here(), source);
  }, [askWords, askN, finder, hostNow, here, source]);
  // Another session shown here, or none: a bar opened for this one goes.
  useEffect(() => () => finder?.left(source), [host.key, finder, source]);
  useEffect(() => {
    if (!host.ready || !nearOnReady.current) return;
    nearOnReady.current = false;
    finder?.near(here());
  }, [host.ready, finder, here]);
}
