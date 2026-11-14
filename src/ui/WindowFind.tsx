// Finding words anywhere in a window, as a browser's find does: ⌘F opens a bar at the window's
// top right, and what's typed there is marked wherever the window shows it (the list, a panel,
// the sidebar, a card), the match you're on more strongly, and brought into view. ⏎ and ⌘G go on
// to the next match, ⇧⏎ and ⇧⌘G back to the one before, round from the last to the first; Escape
// in the bar closes it and gives the keyboard back to where it was. It narrows nothing: filtering
// a list, or a PR's files, is ⌥⌘F's (findKeys.ts). A dialog keeps ⌘F (Settings searches itself,
// SettingsFind.tsx), and so does the quick panel, whose ⌘F is Send to.
//
// It finds what's laid out and shown, in the order it reads: not what an element hides (by
// display, visibility or opacity), what a field holds, what's marked `data-find-skip`, nor a row
// a list hasn't drawn. A new find starts where you are: the first match in view in the place the
// keyboard or a click was in last (findKeys.ts `findArea`), else in the window. A conversation
// finds more than it lays out (ThreadFind.tsx): it's a source (`FindSource`), whose own text is
// found as it reads, whose pages further back are counted, and laid out as a step goes back to
// them. Its older matches are reached by going back from its first one laid out, so a step on
// from what's above it lands on that one, and the count says where you are among them all. What
// folds a match away opens to show it (`FindContext`), and what needs more than scrolling to bring
// one into view (the canvas pans to it) takes `FIND_REVEAL`.
//
// The marks are the CSS highlight API's, so nothing in the window is rendered again for them. The
// window's text is read once a search, and again as it changes, at most every `SCAN_MS` while it
// keeps changing (a running session's line, a time going on), and not at all while the bar is
// closed.

import { createContext, useCallback, useContext, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { findArea } from '../findKeys';
import { useLatest } from '../latest';
import { appKeyInTerminal, held, keysIn, typed } from '../platform';
import { countWords, findPattern, MOST, spansIn, startOf, stepTo, type Place } from '../windowFind';
import { scaleOf } from '../zoom';
import { ALL, clipsIn, type Clip } from './clips';
import { dialogUp } from './focusTrap';
import { Button, Icon } from './primitives';

/** What's being found while the bar is open, for what folds lines away to show the ones it finds. */
export const FindContext = createContext<RegExp | null>(null);

/** Sent, bubbling, from the element a match is in before it's brought into view, with the match's
 *  range: what moves its content some other way than by scrolling (the canvas) or keeps part of
 *  its view covered (a conversation's pinned prompt) brings it into view itself, and cancels it. */
export const FIND_REVEAL = 'ca-find-reveal';

/** How long typing rests before a query found only further back in a conversation goes there. */
const SETTLE_MS = 300;
/** The soonest the window's text is read again after it changes, and the most often while it
 *  keeps changing. */
const SOON_MS = 40;
const SCAN_MS = 200;

/** A match laid out: its text, and which of its line's (or root's) matches it is, to know it again
 *  once the window has changed around it. */
export interface FoundRange {
  range: Range;
  key: Node;
  n: number;
}

/** What finds in itself more than its text laid out (a conversation, ThreadFind.tsx): the walk
 *  passes over its element and takes its matches from `hits`, and what it says of what's further
 *  back goes into the count. */
export interface FindSource {
  el: () => HTMLElement | null;
  /** Hands `each` its matches laid out, in order, until `each` says it has enough. */
  hits: (pattern: RegExp, each: (found: FoundRange) => boolean) => void;
  /** Matches further back than what's laid out, in what's been read of it so far. */
  earlier: number;
  /** Not all read yet, and reading on (it can't when a page couldn't be read). */
  partial: boolean;
  reading: boolean;
  /** Lays out what's further back up to the nearest page that says it, then tells the find
   *  `shown`, or `noneBefore` when nothing further back does. */
  seek: () => void;
}

/** The window's find, as a source and a search asking for words have it. */
export interface WindowFinder {
  /** Adds a source; what it returns takes it away. */
  register: (source: FindSource) => () => void;
  /** A source's `earlier`, `partial` or `reading` changed. */
  changed: () => void;
  /** A source laid out what `seek` asked for. */
  shown: (source: FindSource) => void;
  /** Nothing further back in a source says it. */
  noneBefore: (source: FindSource) => void;
  /** Opens the find on `words`, leaving the keyboard where it is, at the nearest match in
   *  `region`: a conversation (`by`) a search found by what was said in it. */
  ask: (words: string, region: Element | null, by: FindSource) => void;
  /** The conversation `by` shows went, or is another one now: a bar its search opened, that you
   *  haven't taken up since, goes with it. */
  left: (by: FindSource) => void;
  /** Goes to the nearest match in `region` again, while the find is open: what was asked laid
   *  out since. */
  near: (region: Element | null) => void;
}

const FinderContext = createContext<WindowFinder | null>(null);

/** The window's find, for a source; null in a window without one. */
export function useWindowFinder(): WindowFinder | null {
  return useContext(FinderContext);
}

interface Hit extends FoundRange {
  source: FindSource | null;
}

const same = (a: FoundRange, b: FoundRange) => a.key === b.key && a.n === b.n;

/** Elements whose text isn't the window's to find: what a field holds, pictures, code. */
const UNREAD = new Set(['SCRIPT', 'STYLE', 'NOSCRIPT', 'TEMPLATE', 'TEXTAREA', 'INPUT', 'SELECT', 'OPTION', 'CANVAS', 'IFRAME', 'VIDEO', 'AUDIO', 'IMG', 'svg']);
/** Elements a line is laid out in: text in two of them is two lines, which a match doesn't run
 *  across; text in a span, a link or bold within one runs on. */
const LINES = new Set(['P', 'LI', 'PRE', 'BLOCKQUOTE', 'TD', 'TH', 'TR', 'DIV', 'BUTTON', 'H1', 'H2', 'H3', 'H4', 'H5', 'H6', 'SECTION', 'HEADER', 'FOOTER', 'ASIDE', 'MAIN', 'NAV', 'ARTICLE', 'UL', 'OL', 'DL', 'DT', 'DD', 'LABEL', 'SUMMARY', 'DETAILS', 'FIGCAPTION', 'TABLE', 'FORM', 'FIELDSET']);

function passedOver(el: Element): boolean {
  return UNREAD.has(el.tagName) || el.hasAttribute('hidden') || el.hasAttribute('inert') || el.hasAttribute('data-find-skip') || el.getAttribute('aria-hidden') === 'true';
}

/** Whether what's in `el` shows: not hidden by display, visibility or opacity, its own or an
 *  ancestor's. An element laid out as its children (`display: contents`) has no box of its own,
 *  so it shows as its parent does. */
function isShown(el: Element): boolean {
  if (typeof el.checkVisibility !== 'function') return el.getClientRects().length > 0;
  if (el.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true, opacityProperty: true, visibilityProperty: true })) return true;
  return getComputedStyle(el).display === 'contents' && !!el.parentElement && isShown(el.parentElement);
}

/** Every match shown in `root`, in the order it reads, at most `MOST`, the sources' own where
 *  their elements are; `capped` when there were more. Not one cut off where no scrolling reaches
 *  it (past an ellipsis, under a message cut to its first lines): it couldn't be shown. */
function scan(root: Element, pattern: RegExp, sources: ReadonlyMap<Element, FindSource>): { hits: Hit[]; capped: boolean } {
  const hits: Hit[] = [];
  let capped = false;
  const cutOf = clipsIn(null, ALL, true);
  const inSight = (range: Range) => {
    const r = range.getBoundingClientRect();
    if (r.width === 0 && r.height === 0) return false;
    const cut = cutOf(range.startContainer.parentElement);
    return r.right > cut.left && r.left < cut.right && r.bottom > cut.top && r.top < cut.bottom;
  };
  const shown = new Map<Element, boolean>();
  const visible = (el: Element) => {
    let v = shown.get(el);
    if (v === undefined) shown.set(el, (v = isShown(el)));
    return v;
  };
  // The line being read: its text nodes, where each starts in its text, and the element it's in.
  const counted = new Map<Node, number>();
  let nodes: Text[] = [];
  let starts: number[] = [];
  let text = '';
  let line: Element = root;
  const take = (f: FoundRange, source: FindSource | null) => {
    if (capped || !inSight(f.range)) return;
    if (hits.length >= MOST) capped = true;
    else hits.push({ ...f, source });
  };
  const flush = () => {
    if (nodes.length > 0 && !capped) {
      let n = counted.get(line) ?? 0;
      for (const s of spansIn(text, starts, pattern)) {
        const range = document.createRange();
        range.setStart(nodes[s.first], s.from);
        range.setEnd(nodes[s.last], s.to);
        take({ range, key: line, n: n++ }, null);
        if (capped) break;
      }
      counted.set(line, n);
    }
    nodes = [];
    starts = [];
    text = '';
  };
  const visit = (el: Element, at: Element) => {
    for (let c = el.firstChild; c && !capped; c = c.nextSibling) {
      if (c.nodeType === Node.TEXT_NODE) {
        if (at !== line) {
          flush();
          line = at;
        }
        if (!visible(el)) continue;
        const t = c as Text;
        nodes.push(t);
        starts.push(text.length);
        text += t.data;
      } else if (c.nodeType === Node.ELEMENT_NODE) {
        const child = c as Element;
        if (passedOver(child)) continue;
        const source = sources.get(child);
        if (source) {
          flush();
          if (visible(child))
            source.hits(pattern, (f) => {
              take(f, source);
              return !capped;
            });
          continue;
        }
        visit(child, LINES.has(child.tagName) ? child : at);
      }
    }
  };
  visit(root, root);
  flush();
  return { hits, capped };
}

/** What the window leaves in sight of what's in an element, by the boxes around it that clip. */
const clipsInWindow = () => clipsIn(null, { left: 0, top: 0, right: window.innerWidth, bottom: window.innerHeight });

/** Where a match is against what its scrollers leave in sight: above, in sight, or below (one
 *  off to a side counts as further on, out of sight). */
function placeOf(range: Range, clipOf: (el: Element | null) => Clip): Place {
  const r = range.getBoundingClientRect();
  const view = clipOf(range.startContainer.parentElement);
  if (r.bottom <= view.top) return -1;
  if (r.top >= view.bottom || r.right <= view.left || r.left >= view.right) return 1;
  return 0;
}

/** What of `b`, a box over a line of `range`, isn't under something else drawn over it: the
 *  strip stuck over a list, a conversation's pinned prompt, a popover, the find's own bar. Read at
 *  its top and bottom edges, across the middle, so what covers a part of it cuts it there; null
 *  when both are covered. What's in the match's own element (a word of it in bold, a highlighted
 *  token) isn't over it, nor is what doesn't take the pointer (the marks). */
function uncovered(b: Clip, range: Range): Clip | null {
  const common = range.commonAncestorContainer;
  const own = common instanceof Element ? common : common.parentElement;
  if (!own) return b;
  const x = (b.left + b.right) / 2;
  // What's drawn over it, whole: the point can find a part of it (a button on the strip), whose
  // edge isn't the strip's.
  const over = (y: number) => {
    let el = document.elementFromPoint(x, y);
    if (!el || own.contains(el) || el.contains(own)) return null;
    while (el.parentElement && !el.parentElement.contains(own)) el = el.parentElement;
    return el;
  };
  const top = over(b.top + 1);
  const bottom = over(b.bottom - 1);
  if (top && bottom) return null;
  const left = { ...b };
  if (top) left.top = Math.max(left.top, top.getBoundingClientRect().bottom);
  if (bottom) left.bottom = Math.min(left.bottom, bottom.getBoundingClientRect().top);
  return left.bottom > left.top ? left : null;
}

/** The match a new find starts on among `hits` (none empty), where you are (`startOf`). */
function pick(hits: Hit[]): Hit {
  const clipOf = clipsInWindow();
  return hits[Math.max(0, startOf(hits.map((h) => placeOf(h.range, clipOf))))];
}

/** Scrolls each box that scrolls between `from` and the window, the innermost first, so the match
 *  shows a third of the way down it, unless it shows already. `top` is how much of `from`'s own
 *  view is covered at its top (a conversation's pinned prompt). */
export function scrollToRange(range: Range, from: Element | null, top = 0): void {
  for (let el = from as HTMLElement | null; el; el = el.parentElement) {
    const tall = el.scrollHeight > el.clientHeight;
    const wide = el.scrollWidth > el.clientWidth;
    if (!tall && !wide) continue;
    const style = getComputedStyle(el);
    const y = tall && /(auto|scroll)/.test(style.overflowY);
    const x = wide && /(auto|scroll)/.test(style.overflowX);
    if (!x && !y) continue;
    const r = range.getBoundingClientRect();
    if (r.width === 0 && r.height === 0) return;
    const box = el.getBoundingClientRect();
    const scale = scaleOf(el);
    const covered = el === from ? top : 0;
    if (y) {
      const at = (r.top - box.top) / scale;
      const end = (r.bottom - box.top) / scale;
      // In sight but under what's drawn over it, as the strip stuck over a list is, it goes a
      // third of the way down too.
      const whole = { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
      const shown = uncovered(whole, range);
      const under = !shown || shown.top !== whole.top || shown.bottom !== whole.bottom;
      if (at < covered + 8 || end > el.clientHeight - 16 || under) el.scrollTop = Math.max(0, el.scrollTop + at - el.clientHeight / 3);
    }
    if (x) {
      const at = (r.left - box.left) / scale;
      const end = (r.right - box.left) / scale;
      if (at < 8 || end > el.clientWidth - 8) el.scrollLeft = Math.max(0, el.scrollLeft + at - el.clientWidth / 3);
    }
  }
}

/** Brings a match into view: what it's in can do it itself (`FIND_REVEAL`), else it's scrolled to. */
function bringIntoView(range: Range): void {
  const el = range.startContainer.parentElement;
  if (!el) return;
  const asked = new CustomEvent<Range>(FIND_REVEAL, { bubbles: true, cancelable: true, detail: range });
  if (el.dispatchEvent(asked)) scrollToRange(range, el);
}

/**
 * Marks the matches: the one you're on with a CSS highlight, drawn in ink as a selection is, and
 * every other one in sight with a box over it in `layer`, which covers the window, none where
 * something else is drawn over the text. Not all of them
 * with highlights: WebKit checks every highlighted range for each piece of text it draws, some
 * 5 ms a range (.claude/rules/windows.md), so a find that marks hundreds of places would hold the
 * window for seconds at each repaint, as Settings' search did (SettingsFind.tsx `paint`). A box
 * is cut off where the boxes around its text clip it (a list scrolled, an ellipsis), and the parts
 * of a match on one line (a word half in bold) are one box. The boxes are reused as they're drawn
 * again, for each scroll.
 */
function paint(layer: HTMLElement | null, all: Hit[], at: Hit | null): void {
  const reg = typeof CSS !== 'undefined' ? CSS.highlights : undefined;
  if (reg && at) {
    if (reg.get('ca-find-at')?.has(at.range) !== true) reg.set('ca-find-at', new Highlight(at.range));
  } else reg?.delete('ca-find-at');
  if (!layer) return;
  layer.dataset.shown = String(all.length);
  const clipOf = clipsInWindow();
  const boxes: Clip[] = [];
  // Cut off where the text is, and none where something's drawn over it: the layer is over the
  // whole window, so a box would show over the strip stuck over a list, or a popover.
  const cut = (b: Clip, clip: Clip, range: Range) => {
    const box = { left: Math.max(b.left, clip.left), top: Math.max(b.top, clip.top), right: Math.min(b.right, clip.right), bottom: Math.min(b.bottom, clip.bottom) };
    const shown = box.right > box.left && box.bottom > box.top ? uncovered(box, range) : null;
    if (shown) boxes.push(shown);
  };
  for (const h of all) {
    if (h === at) continue;
    const clip = clipOf(h.range.startContainer.parentElement);
    if (clip.right <= clip.left || clip.bottom <= clip.top) continue;
    let line: Clip | null = null;
    for (const r of h.range.getClientRects()) {
      if (r.width === 0 && r.height === 0) continue;
      if (line && Math.abs(r.top - line.top) < 1 && Math.abs(r.bottom - line.bottom) < 1 && r.left <= line.right + 1) {
        line.right = Math.max(line.right, r.right);
        continue;
      }
      if (line) cut(line, clip, h.range);
      line = { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
    }
    if (line) cut(line, clip, h.range);
  }
  boxes.forEach((b, i) => {
    const el = (layer.children[i] as HTMLElement | undefined) ?? layer.appendChild(document.createElement('span'));
    el.style.cssText = `left:${b.left}px;top:${b.top}px;width:${b.right - b.left}px;height:${b.bottom - b.top}px`;
  });
  while (layer.children.length > boxes.length) layer.lastElementChild!.remove();
}
function unpaint(layer: HTMLElement | null): void {
  if (typeof CSS !== 'undefined') CSS.highlights?.delete('ca-find-at');
  layer?.replaceChildren();
  if (layer) layer.dataset.shown = '0';
}

/** Still in the window and shown, to count in the find. */
const showing = (s: FindSource) => {
  const el = s.el();
  return !!el && el.isConnected && el.getClientRects().length > 0;
};

/** Whether `el` is on the way of a step from `from` to `to` in the order the window reads: after
 *  one and before the other, going on (1) or back (-1), and round the window when the step goes
 *  round it. With nothing gone to yet, the step starts at the window's start going on, its end
 *  going back; with nothing laid out to go to, everything's on the way. */
function between(el: Element, from: Hit | undefined, to: Hit | undefined, dir: -1 | 1): boolean {
  const precedes = (a: Node, b: Node) => a !== b && !!(a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING);
  const a = from?.range.startContainer;
  const b = to?.range.startContainer;
  if (!b) return true;
  if (!a) return dir > 0 ? precedes(el, b) : precedes(b, el);
  const [lo, hi] = dir > 0 ? [a, b] : [b, a];
  const round = dir > 0 ? !precedes(a, b) : !precedes(b, a);
  return round ? precedes(lo, el) || precedes(el, hi) : precedes(lo, el) && precedes(el, hi);
}

/** A source shown that says it further back, or is still reading to find out. */
const further = (s: FindSource | undefined): s is FindSource => !!s && (s.earlier > 0 || s.reading) && showing(s);

/** The window's find, around everything the window shows: the bar, the keys, and what a source or
 *  a search asking for words reach it through (`useWindowFinder`). */
export function WindowFind({ children }: { children: ReactNode }) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const pattern = useMemo(() => (open ? findPattern(query) : null), [open, query]);
  const patternNow = useLatest(pattern);
  const openNow = useLatest(open);
  const queryNow = useLatest(query);
  const input = useRef<HTMLInputElement>(null);
  const bar = useRef<HTMLDivElement>(null);
  // The boxes over the matches but the one you're on (`paint`).
  const layer = useRef<HTMLDivElement>(null);
  // Where the keyboard was before ⌘F, for Escape to give it back.
  const back = useRef<HTMLElement | null>(null);
  // An open asked for rather than typed for, which leaves the keyboard where it is; and each ⌘F,
  // which gives the bar the keyboard with its words ready to type over.
  const quiet = useRef(false);
  const [focusAsk, setFocusAsk] = useState(0);

  const sources = useRef(new Set<FindSource>());
  // A source's counts changed: the bar says them again.
  const [, setCounted] = useState(0);

  // The matches laid out, whether there were more, the one you're on, and what the bar says of them.
  const hits = useRef<Hit[]>([]);
  const capped = useRef(false);
  const current = useRef<Hit | null>(null);
  const [view, setView] = useState<{ at: number | null; shown: number; capped: boolean }>({ at: null, shown: 0, capped: false });
  // Going back into a source's pages further back, from the match you were on (null: from its
  // end), and whether the pages it asked for are being laid out.
  // `then`: where a step that went into it on its way goes when nothing further back says it.
  const seek = useRef<{ source: FindSource; from: Hit | null; then: Hit | null; showing: boolean } | null>(null);
  const [looking, setLooking] = useState(false);
  // Where the next new query starts, when a search asked for it there; and where you were as ⌘F
  // opened the bar, before the keyboard went into it (findKeys.ts `findArea`).
  const askedAt = useRef<Element | null>(null);
  const openedIn = useRef<Element | null>(null);
  // The conversation a search opened the bar for, until you take it up (type in it, step, ⌘F).
  const askedBy = useRef<FindSource | null>(null);
  const settle = useRef<number | null>(null);

  const show = useCallback((at: Hit | null) => {
    current.current = at;
    const i = at ? hits.current.indexOf(at) : -1;
    setView({ at: i < 0 ? null : i, shown: hits.current.length, capped: capped.current });
    paint(layer.current, hits.current, at);
  }, []);

  const goTo = useCallback(
    (i: number) => {
      const h = hits.current[i];
      if (!h) return;
      show(h);
      bringIntoView(h.range);
    },
    [show],
  );

  const read = useCallback((p: RegExp) => {
    const at = new Map<Element, FindSource>();
    for (const s of sources.current) {
      const el = s.el();
      if (el) at.set(el, s);
    }
    const found = scan(document.body, p, at);
    hits.current = found.hits;
    capped.current = found.capped;
    return found.hits;
  }, []);

  const startSeek = useCallback((source: FindSource, from: Hit | null, then: Hit | null = null) => {
    seek.current = { source, from, then, showing: false };
    setLooking(true);
    source.seek();
  }, []);

  /** Goes back into `source` once typing rests, as long as nothing's been gone to meanwhile. */
  const settleSeek = useCallback(
    (source: FindSource) => {
      if (settle.current != null) window.clearTimeout(settle.current);
      const p = patternNow.current;
      settle.current = window.setTimeout(() => {
        settle.current = null;
        if (patternNow.current !== p || current.current || seek.current) return;
        startSeek(source, null);
      }, SETTLE_MS);
    },
    [patternNow, startSeek],
  );

  /** Where a new query goes: the first match in view where you are (or where a search asked),
   *  else further back in the conversation you're in, else the first in view in the window, else
   *  further back in any conversation shown. Where a search asked has it alone: nothing else in
   *  the window is gone to for it (its conversation may still be on its way). */
  const start = useCallback(
    (found: Hit[]) => {
      const asked = askedAt.current?.isConnected ? askedAt.current : null;
      askedAt.current = null;
      // Typed in the bar, the keyboard's there: where you are is where you were as it opened.
      const opened = bar.current?.contains(document.activeElement) && openedIn.current?.isConnected ? openedIn.current : null;
      const region = asked ?? opened ?? findArea();
      const mine = region ? found.filter((h) => region.contains(h.range.startContainer)) : [];
      if (mine.length > 0) return goTo(found.indexOf(pick(mine)));
      show(null);
      const here = region
        ? [...sources.current].find((s) => {
            const el = s.el();
            return !!el && (region.contains(el) || el.contains(region));
          })
        : undefined;
      if (further(here)) return settleSeek(here);
      if (asked) return;
      if (found.length > 0) return goTo(found.indexOf(pick(found)));
      const any = [...sources.current].find(further);
      if (any) settleSeek(any);
    },
    [goTo, show, settleSeek],
  );

  /** Reads the window's matches again. `near` (a new query) goes to where it starts (`start`);
   *  otherwise the one you were on stays, and a way back that was waiting on pages goes on. */
  const refresh = useCallback(
    (near: boolean) => {
      const p = patternNow.current;
      if (!p) {
        hits.current = [];
        capped.current = false;
        current.current = null;
        setView({ at: null, shown: 0, capped: false });
        unpaint(layer.current);
        return;
      }
      // Where you were among them, for when what it was in is drawn anew.
      const was = current.current;
      const wasAt = was ? hits.current.indexOf(was) : -1;
      const found = read(p);
      const s = seek.current;
      if (s?.showing) {
        const mine = found.filter((h) => h.source === s.source);
        const from = s.from ? mine.findIndex((h) => same(h, s.from!)) : mine.length;
        if (from > 0) {
          seek.current = null;
          setLooking(false);
          return goTo(found.indexOf(mine[from - 1]));
        }
        if (from === 0) {
          // What was laid out says it only where it doesn't show (a call's output): further back.
          s.showing = false;
          show(s.from ? (found.find((h) => same(h, s.from!)) ?? null) : null);
          return s.source.seek();
        }
        // The match it went back from is gone: it stops where it is.
        seek.current = null;
        setLooking(false);
      }
      if (near) return start(found);
      show(was ? (found.find((h) => same(h, was)) ?? found[Math.min(wasAt, found.length - 1)] ?? null) : null);
    },
    [patternNow, read, goTo, show, start],
  );
  const refreshNow = useLatest(refresh);

  /** On to the next match (1) or back to the one before (-1), round the window; back from the
   *  first a conversation lays out goes into its pages further back first, and a step past a
   *  conversation that says it only further back goes into it. */
  const step = useCallback(
    (dir: -1 | 1) => {
      askedBy.current = null;
      if (seek.current) return;
      const found = hits.current;
      const at = current.current;
      if (dir < 0) {
        const src = at?.source ?? undefined;
        if (at && src && further(src) && found.find((h) => h.source === src) === at) return startSeek(src, at);
      }
      const from = at ? found.indexOf(at) : -1;
      const i = stepTo(from < 0 ? null : from, found.length, dir);
      const passed = [...sources.current].find((s) => further(s) && !found.some((h) => h.source === s) && between(s.el()!, found[from], found[i], dir));
      if (passed) return startSeek(passed, null, found[i] ?? null);
      if (i >= 0) goTo(i);
    },
    [goTo, startSeek],
  );
  const stepNow = useLatest(step);

  const close = useCallback(() => {
    setOpen(false);
    seek.current = null;
    setLooking(false);
    askedAt.current = null;
    askedBy.current = null;
    if (settle.current != null) window.clearTimeout(settle.current);
    settle.current = null;
    const to = back.current;
    back.current = null;
    if (to?.isConnected) to.focus();
  }, []);

  const finder = useMemo<WindowFinder>(
    () => ({
      register: (source) => {
        sources.current.add(source);
        return () => {
          sources.current.delete(source);
          if (seek.current?.source === source) {
            seek.current = null;
            setLooking(false);
          }
        };
      },
      changed: () => setCounted((n) => n + 1),
      shown: (source) => {
        const s = seek.current;
        if (s?.source !== source) return;
        s.showing = true;
        // Found again as the pages change the window; this in case they show nothing new.
        window.setTimeout(() => refreshNow.current(false), 150);
      },
      noneBefore: (source) => {
        const s = seek.current;
        if (s?.source !== source) return;
        seek.current = null;
        setLooking(false);
        const found = hits.current;
        if (s.from) {
          // Back from its first match: on to the one before it in the window.
          const i = found.findIndex((h) => same(h, s.from!));
          const j = stepTo(i, found.length, -1);
          if (j >= 0 && j !== i) goTo(j);
        } else if (s.then) {
          // A step that went into it on its way: on to where it was going.
          const i = found.findIndex((h) => same(h, s.then!));
          if (i >= 0) goTo(i);
        } else if (!current.current && found.length > 0) goTo(found.indexOf(pick(found)));
      },
      ask: (words, region, by) => {
        askedAt.current = region;
        if (!openNow.current) {
          quiet.current = true;
          askedBy.current = by;
          setOpen(true);
        } else {
          if (askedBy.current) askedBy.current = by;
          if (queryNow.current === words) refreshNow.current(true);
        }
        setQuery(words);
      },
      left: (by) => {
        if (askedBy.current === by) close();
      },
      near: (region) => {
        if (!openNow.current || !patternNow.current) return;
        askedAt.current = region;
        refreshNow.current(true);
      },
    }),
    [goTo, close, openNow, patternNow, queryNow, refreshNow],
  );

  // A new query, or the bar opened or closed: found again, and where it starts brought into view.
  useLayoutEffect(() => {
    seek.current = null;
    setLooking(false);
    if (settle.current != null) window.clearTimeout(settle.current);
    settle.current = null;
    refreshNow.current(true);
  }, [pattern, refreshNow]);

  // As the window changes (a list's rows, a page laid out, a reply streaming in, a panel opened),
  // its matches are found again and the one you're on stays; not for the bar's own changes, nor
  // the marks'. What moves the text without changing it (a scroll, the canvas's camera) draws the
  // boxes again where it's gone, once a frame.
  useEffect(() => {
    if (!pattern) return;
    let timer: number | null = null;
    let last = 0;
    let frame: number | null = null;
    const redraw = () => {
      if (frame != null) return;
      frame = window.requestAnimationFrame(() => {
        frame = null;
        paint(layer.current, hits.current, current.current);
      });
    };
    const later = () => {
      if (timer != null) return;
      timer = window.setTimeout(
        () => {
          timer = null;
          last = Date.now();
          refreshNow.current(false);
        },
        Math.max(SOON_MS, SCAN_MS - (Date.now() - last)),
      );
    };
    const observer = new MutationObserver((records) => {
      if (records.every((r) => bar.current?.contains(r.target) || layer.current?.contains(r.target))) return;
      redraw();
      later();
    });
    observer.observe(document.body, { childList: true, subtree: true, characterData: true, attributes: true, attributeFilter: ['class', 'style', 'hidden', 'aria-hidden', 'open'] });
    const resized = () => {
      redraw();
      later();
    };
    window.addEventListener('resize', resized);
    document.addEventListener('scroll', redraw, { capture: true, passive: true });
    return () => {
      observer.disconnect();
      window.removeEventListener('resize', resized);
      document.removeEventListener('scroll', redraw, { capture: true });
      if (timer != null) window.clearTimeout(timer);
      if (frame != null) window.cancelAnimationFrame(frame);
    };
  }, [pattern, refreshNow]);

  // ⌘F opens the bar (or takes the keyboard back to it), and ⌘G and ⇧⌘G step while it's open,
  // ahead of the window's own keys. Not while a dialog is up: what's typed is the dialog's.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.isComposing) return;
      const key = e.key.toLowerCase();
      const find = key === 'f' && held(e, 'Cmd');
      const again = key === 'g' && (held(e, 'Cmd') || held(e, 'Shift', 'Cmd'));
      if ((!find && !again) || dialogUp()) return;
      // A terminal of Codealong's own keeps the keys that are its program's (Ctrl+F on Linux).
      if (document.activeElement?.closest('.ca-term') && !appKeyInTerminal(e)) return;
      if (again) {
        if (!openNow.current || !patternNow.current) return;
        e.preventDefault();
        stepNow.current(e.shiftKey ? -1 : 1);
        return;
      }
      e.preventDefault();
      if (!openNow.current) {
        const at = document.activeElement;
        back.current = at instanceof HTMLElement && at !== document.body ? at : null;
      }
      // Again from the bar itself, it's where you were when it opened.
      if (!bar.current?.contains(document.activeElement)) openedIn.current = findArea();
      quiet.current = false;
      askedBy.current = null;
      setOpen(true);
      setFocusAsk((n) => n + 1);
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [openNow, patternNow, stepNow]);

  // Opened, or ⌘F again: the keyboard goes to the bar, with what was found last ready to type
  // over, unless the open was asked for (`ask`).
  useLayoutEffect(() => {
    if (!open) return;
    if (quiet.current) return void (quiet.current = false);
    input.current?.focus();
    input.current?.select();
  }, [open, focusAsk]);

  useEffect(
    () => () => {
      unpaint(null);
      if (settle.current != null) window.clearTimeout(settle.current);
    },
    [],
  );

  // What the conversations say further back counts too: before a match, those whose pages come
  // before it in the window (its own among them).
  const counting = pattern ? [...sources.current].filter(showing) : [];
  const earlier = counting.reduce((n, s) => n + s.earlier, 0);
  const node = current.current?.range.startContainer;
  const before = node ? counting.filter((s) => s.el()!.contains(node) || !!(s.el()!.compareDocumentPosition(node) & Node.DOCUMENT_POSITION_FOLLOWING)).reduce((n, s) => n + s.earlier, 0) : 0;
  const total = earlier + view.shown;
  const partial = counting.some((s) => s.partial);
  const reading = counting.some((s) => s.reading);
  return (
    <FinderContext.Provider value={finder}>
      <FindContext.Provider value={pattern}>{children}</FindContext.Provider>
      {open && <div className="ca-findmarks" ref={layer} aria-hidden="true" data-find-skip />}
      {open && (
        <div className="ca-findbar" role="search" ref={bar} data-find-skip>
          <label className="ca-findbar-field">
            <Icon name="search" size={12} />
            <input
              ref={input}
              value={query}
              placeholder="Find"
              aria-label="Find in the window"
              spellCheck={false}
              autoCapitalize="off"
              autoCorrect="off"
              onChange={(e) => {
                askedBy.current = null;
                setQuery(e.target.value);
              }}
              onFocus={() => (askedBy.current = null)}
              onKeyDown={(e) => {
                if (e.nativeEvent.isComposing || !typed(e)) return;
                if (e.key === 'Enter') {
                  e.preventDefault();
                  step(e.shiftKey ? -1 : 1);
                } else if (e.key === 'Escape') {
                  // The bar's, not the window's (which would put a panel or the window away).
                  e.preventDefault();
                  close();
                }
              }}
            />
          </label>
          {pattern && (
            <span className="ca-findbar-count" role="status">
              {countWords({ at: view.at == null ? null : before + view.at, total, capped: view.capped, partial, reading, looking })}
            </span>
          )}
          {/* The cursor stays in the field. */}
          <Button variant="ghost" size="sm" icon="chevron-up" title={keysIn('Previous match (⇧⏎)')} aria-label="Previous match" disabled={!pattern || looking || (total === 0 && !reading)} onMouseDown={(e) => e.preventDefault()} onClick={() => step(-1)} />
          <Button variant="ghost" size="sm" icon="chevron-down" title={keysIn('Next match (⏎)')} aria-label="Next match" disabled={!pattern || looking || view.shown === 0} onMouseDown={(e) => e.preventDefault()} onClick={() => step(1)} />
          <Button variant="ghost" size="sm" icon="x" title="Close (esc)" aria-label="Close find" onClick={close} />
        </div>
      )}
    </FinderContext.Provider>
  );
}
