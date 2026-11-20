// @vitest-environment jsdom
import { beforeEach, describe, expect, it } from 'vitest';
import { act, mountHook, type Mounted } from './testing/dom';
import { useListFind, type ListFindInput } from './useListFind';

// The list's find in the main window (useListFind.ts): when it shows, when it takes the keyboard,
// and the panel it takes the whole window back from.

let did: string[] = [];
beforeEach(() => {
  did = [];
});

const has = (over: Partial<ListFindInput> = {}): ListFindInput => ({
  query: '',
  view: 'sessions',
  loading: false,
  setPanelMax: (max) => void did.push(`session panel max ${max}`),
  setPrPanelMax: (max) => void did.push(`PR panel max ${max}`),
  ...over,
});
type Find = Mounted<ReturnType<typeof useListFind>, ListFindInput>;

/** Lays the page out as the window does: the board, and the find's field once the find shows. */
function page(w: Find, laidOut = true): { board: HTMLElement; field: HTMLInputElement } {
  const board = document.body.appendChild(document.createElement('main'));
  // jsdom lays nothing out: a board that's drawn has a box, and one a panel covers has none.
  board.getClientRects = () => (laidOut ? [new DOMRect(0, 0, 800, 600)] : []) as unknown as DOMRectList;
  const field = board.appendChild(document.createElement('input'));
  field.value = 'importer';
  (w.current.boardRef as { current: HTMLElement | null }).current = board;
  (w.current.searchRef as { current: HTMLInputElement | null }).current = field;
  return { board, field };
}

describe('the list’s find', () => {
  it('shows while it’s open, and as long as it has words', () => {
    const w = mountHook(useListFind, has());
    expect(w.current.findShown).toBe(false);
    act(() => w.current.openFind());
    expect(w.current.findShown).toBe(true);
    act(() => w.current.setFindOpen(false));
    expect(w.current.findShown).toBe(false);
    w.rerender(has({ query: 'importer' }));
    expect(w.current.findShown).toBe(true);
  });

  it('takes the keyboard as it’s asked for, with its words ready to type over, and again on each ask', () => {
    const w = mountHook(useListFind, has({ query: 'importer' }));
    const { field } = page(w);
    act(() => w.current.openFind());
    expect(document.activeElement).toBe(field);
    expect([field.selectionStart, field.selectionEnd]).toEqual([0, 'importer'.length]);
    // The keyboard went elsewhere while the find stayed open: ⌥⌘F brings it back.
    field.blur();
    field.setSelectionRange(3, 3);
    act(() => w.current.openFind());
    expect(document.activeElement).toBe(field);
    expect([field.selectionStart, field.selectionEnd]).toEqual([0, 'importer'.length]);
  });

  it('doesn’t take the keyboard while it only shows for its words', () => {
    const w = mountHook(useListFind, has({ query: 'importer' }));
    const { field } = page(w);
    w.rerender(has({ query: 'importers' }));
    expect(document.activeElement).not.toBe(field);
  });

  it('takes the keyboard again once the board has been read, when it was asked for before', () => {
    const w = mountHook(useListFind, has({ loading: true }));
    act(() => w.current.openFind());
    // Nothing is laid out until the board is read: its field comes with the list's strip.
    const { field } = page(w);
    expect(document.activeElement).not.toBe(field);
    w.rerender(has({ loading: false }));
    expect(document.activeElement).toBe(field);
  });

  it('gives the list its place back from a panel that takes the whole window: the session’s, or the PR’s in PRs', () => {
    const sessions = mountHook(useListFind, has());
    page(sessions, false);
    act(() => sessions.current.openFind());
    const prs = mountHook(useListFind, has({ view: 'prs' }));
    page(prs, false);
    act(() => prs.current.openFind());
    expect(did).toEqual(['session panel max false', 'PR panel max false']);
  });

  it('leaves the panels as they are while the list is laid out, or before there’s a board at all', () => {
    const w = mountHook(useListFind, has());
    act(() => w.current.openFind());
    page(w);
    act(() => w.current.openFind());
    expect(did).toEqual([]);
  });
});
