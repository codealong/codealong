// Searching Settings: a field over its sections (⌘F there, and the keyboard's as Settings opens
// on nothing in particular) that turns the pane into every section's settings that match, each
// section under its name, the fields themselves to change there, the words marked. The sections
// down the left say how many of their settings match and fade where none do; a click on one, or
// on a section's name over its settings, clears the search and opens the section. Escape, from
// anywhere in Settings, clears what's typed before it closes Settings. What a search finds is
// settingsFind.ts's, read from the panes as they're laid out, and read again as they change (an
// account's card comes in, say).

import { useEffect, useLayoutEffect, type RefObject } from 'react';
import { useLatest } from '../latest';
import { held, keysIn } from '../platform';
import { findWords, foundIn, spansIn, type Piece } from '../settingsFind';
import { Button, Icon } from './primitives';
import { useEscape } from './useDismiss';

/** What a search counts as one setting. */
const SETTING = '.ca-field, .ca-ucard, .ca-jev-feature';
/** What a section says now, which shows whatever the words: an error, a question to confirm, what
 *  an action did. */
const ALERT = '[role="alert"], [role="status"], .ca-form-error';
/** Elements whose text isn't the setting's: what you wrote in a box. */
const UNREAD = new Set(['TEXTAREA', 'SCRIPT', 'STYLE']);
/** Fields whose value is found as their text: a folder, a project's name or repos. */
const WRITTEN = new Set(['text', 'search', 'url', 'email']);
/** What a setting the search passes over carries, which hides it. */
const OUT = 'data-unmatched';

function pieceOf(el: Element): Piece<Element> {
  let text = el instanceof HTMLInputElement && WRITTEN.has(el.type) ? el.value : '';
  const kids: Piece<Element>[] = [];
  for (const node of el.childNodes) {
    if (node.nodeType === Node.TEXT_NODE) text += (node as Text).data;
    else if (node instanceof Element && !UNREAD.has(node.tagName)) kids.push(pieceOf(node));
  }
  return { at: el, setting: el.matches(SETTING), alert: el.matches(ALERT), text, kids };
}

/**
 * Marks the words wherever they show in `sections`, their names included, with a box over each
 * in `layer`, which sits at the top left of what scrolls. Not with CSS highlights: WebKit checks
 * every highlighted range for each piece of text it draws, some 5 ms a range on Settings, so a
 * letter typed, which marks 1,600 places, held the window for 8 s (2026-10-22). Where every
 * mark goes is read before any box goes in, so the page is laid out once.
 */
function paint(layer: HTMLElement, sections: Iterable<Element>, words: string[]) {
  const origin = layer.getBoundingClientRect();
  const range = document.createRange();
  const boxes: HTMLElement[] = [];
  for (const section of sections) {
    const walker = document.createTreeWalker(section, NodeFilter.SHOW_TEXT, {
      acceptNode: (node) => (node.parentElement?.closest(`[${OUT}], textarea`) ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT),
    });
    for (let node = walker.nextNode() as Text | null; node; node = walker.nextNode() as Text | null) {
      for (const [from, to] of spansIn(node.data, words)) {
        range.setStart(node, from);
        range.setEnd(node, to);
        // A mark the line wraps through has a box on each line.
        for (const r of range.getClientRects()) {
          const box = document.createElement('span');
          box.style.cssText = `left:${r.left - origin.left}px;top:${r.top - origin.top}px;width:${r.width}px;height:${r.height}px`;
          boxes.push(box);
        }
      }
    }
  }
  layer.replaceChildren(...boxes);
}

/**
 * Narrows the sections in `root` to the settings `query` finds, each section an element with
 * `data-section` (its id) and `data-label` (its name, which finds all of it) around its pane in
 * one with `data-pane`; `onCounts` hears how many settings each has that match. A setting that
 * showed stays while the words are the same, so changing it doesn't hide it, and so does one that
 * has the keyboard: the form a card's Edit opens in its place.
 */
export function useSettingsFind(root: RefObject<HTMLElement | null>, query: string, onCounts: (counts: Record<string, number>) => void): void {
  const counted = useLatest(onCounts);
  useLayoutEffect(() => {
    const el = root.current;
    const words = findWords(query);
    if (!el || words.length === 0) return;
    el.scrollTop = 0;
    const layer = document.createElement('div');
    layer.className = 'ca-settings-find-marks';
    layer.setAttribute('aria-hidden', 'true');
    el.append(layer);
    const sections = () => el.querySelectorAll<HTMLElement>('[data-section]');
    // The sizes the marks were drawn at: the pane's and its sections'.
    const sizes = () => [el, ...sections()].map((e) => `${e.clientWidth}x${e.clientHeight}`).join(' ');
    let drawnAt = '';
    const mark = () => {
      paint(layer, sections(), words);
      drawnAt = sizes();
    };
    const kept = new Set<Element>();
    const run = () => {
      for (let s = document.activeElement?.closest(SETTING); s && el.contains(s); s = s.parentElement?.closest(SETTING)) kept.add(s);
      const counts: Record<string, number> = {};
      for (const section of sections()) {
        const pane = section.querySelector('[data-pane]');
        if (!pane) continue;
        for (const out of pane.querySelectorAll(`[${OUT}]`)) out.removeAttribute(OUT);
        const found = foundIn(pieceOf(pane), words, section.dataset.label ?? '', kept);
        for (const out of found.hidden) out.setAttribute(OUT, '');
        for (const s of found.shown) kept.add(s);
        section.toggleAttribute(OUT, found.count === 0 && !found.alerting);
        counts[section.dataset.section ?? ''] = found.count;
      }
      mark();
      counted.current(counts);
    };
    run();
    // Only what's laid out changing, not what the search sets: the settings it hides, which it
    // doesn't hear, and its marks.
    const watch = new MutationObserver((changes) => {
      if (changes.some((c) => !layer.contains(c.target))) run();
    });
    watch.observe(el, { childList: true, subtree: true, characterData: true });
    // The marks move with what moves the text without a change to it, a narrower window or a box
    // that grows as it's typed in, and not again for what they were just drawn over: a pass costs
    // WebKit some 50 ms when a letter marks 1,500 places.
    const moved = new ResizeObserver(() => {
      if (sizes() !== drawnAt) mark();
    });
    for (const e of [el, ...sections()]) moved.observe(e);
    return () => {
      watch.disconnect();
      moved.disconnect();
      layer.remove();
    };
  }, [root, query, counted]);
}

interface SettingsFindFieldProps {
  inputRef: RefObject<HTMLInputElement>;
  value: string;
  onChange: (value: string) => void;
  /** Has the keyboard as Settings opens. */
  autoFocus: boolean;
}

/** The search field over Settings' sections. ⌘F anywhere in Settings brings the keyboard to it. */
export function SettingsFindField({ inputRef, value, onChange, autoFocus }: SettingsFindFieldProps) {
  // Escape clears what's typed wherever the keyboard is in Settings: the search is opened over it,
  // and a menu opened in what it found closes first, as it opened after (useDismiss.ts).
  useEscape(() => {
    onChange('');
    inputRef.current?.focus();
  }, value !== '');
  useEffect(() => {
    // Nothing behind Settings takes keys while it's up (mainKeys.ts, `dialogUp`).
    const onKey = (e: KeyboardEvent) => {
      if (!held(e, 'Cmd') || e.key.toLowerCase() !== 'f') return;
      e.preventDefault();
      inputRef.current?.focus();
      inputRef.current?.select();
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [inputRef]);
  return (
    <div className="ca-settings-find" role="search">
      <label className="ca-settings-find-field" title={keysIn('Search settings (⌘F)')}>
        <Icon name="search" size={12} />
        <input
          ref={inputRef}
          type="text"
          value={value}
          placeholder="Search settings"
          aria-label="Search settings"
          autoFocus={autoFocus}
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
          onChange={(e) => onChange(e.target.value)}
        />
        {value && <Button variant="ghost" size="sm" icon="x" className="ca-settings-find-clear" aria-label="Clear search" title="Clear (esc)" onMouseDown={(e) => e.preventDefault()} onClick={() => onChange('')} />}
      </label>
    </div>
  );
}
