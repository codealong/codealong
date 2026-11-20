// @vitest-environment jsdom
import { describe, expect, it } from 'vitest';
import { act, mountHook } from './testing/dom';
import { useRevealFind, type RevealFindInput } from './useRevealFind';

// What the find bar in a session's panel opens on in the main window (useRevealFind.ts).

const KESTREL = 'claude:northwind:kestrel';
const ATLAS = 'claude:northwind:atlas';
const none = () => undefined;
const has = (over: Partial<RevealFindInput> = {}): RevealFindInput => ({ selectedSessionKey: KESTREL, query: '', saidOf: none, ...over });
/** The words the quick panel found a session by, as the window's `reveal` keeps them. */
const found = { key: KESTREL, ask: { words: 'importer', n: 7 } };

describe('what a session’s find bar opens on', () => {
  it('is nothing for a session nothing found by its words', () => {
    expect(mountHook(useRevealFind, has()).current.findIn(KESTREL)).toBeNull();
  });

  it('is the words the quick panel found the session by, for that session alone', () => {
    const w = mountHook(useRevealFind, has());
    act(() => w.current.setRevealFind(found));
    expect(w.current.findIn(KESTREL)).toEqual({ words: 'importer', n: 7 });
    expect(w.current.findIn(ATLAS)).toBeNull();
  });

  it('lets those words go once another session is selected, so the bar isn’t handed them again', () => {
    const w = mountHook(useRevealFind, has());
    act(() => w.current.setRevealFind(found));
    w.rerender(has({ selectedSessionKey: ATLAS }));
    w.rerender(has());
    expect(w.current.findIn(KESTREL)).toBeNull();
  });

  it('lets them go once a search is typed here', () => {
    const w = mountHook(useRevealFind, has());
    act(() => w.current.setRevealFind(found));
    w.rerender(has({ query: 'docs' }));
    w.rerender(has());
    expect(w.current.findIn(KESTREL)).toBeNull();
  });

  it('is what this window’s search found the session by in what was said, over the quick panel’s words', () => {
    const saidOf = (key: string) => (key === KESTREL ? { text: 'Then tidy the importer tests' } : undefined);
    const w = mountHook(useRevealFind, has({ query: 'importer tests', saidOf }));
    expect(w.current.findIn(KESTREL)).toEqual({ words: 'importer tests', n: 0 });
    // The longest of the words the line has, when it doesn't have them together.
    w.rerender(has({ query: 'tests importer', saidOf }));
    expect(w.current.findIn(KESTREL)).toEqual({ words: 'importer', n: 0 });
    expect(w.current.findIn(ATLAS)).toBeNull();
  });

  it('gives the panel the same way to ask while nothing it goes by has changed', () => {
    const input = has();
    const w = mountHook(useRevealFind, input);
    const before = w.current.findIn;
    w.rerender({ ...input });
    expect(w.current.findIn).toBe(before);
    w.rerender({ ...input, query: 'docs' });
    expect(w.current.findIn).not.toBe(before);
  });
});
