// The list's find in the main window (ui/ListBar.tsx `ListFind`, App.tsx): open from ⌥⌘F or /
// until the keyboard leaves it, and shown as long as it has words. Each ask gives it the keyboard
// once what it asks for is laid out, with its words ready to type over.

import { useCallback, useLayoutEffect, useRef, useState } from 'react';

/** What the window has for it. */
export interface ListFindInput {
  /** The find's words. */
  query: string;
  view: string;
  /** The board hasn't been read yet: the find takes the keyboard again once it has. */
  loading: boolean;
  /** Takes a session's panel in Sessions, or a PR's in PRs, back from the whole window. */
  setPanelMax: (max: boolean) => void;
  setPrPanelMax: (max: boolean) => void;
}

export function useListFind({ query, view, loading, setPanelMax, setPrPanelMax }: ListFindInput) {
  /** The board, which the find is over, and the find's own field. */
  const boardRef = useRef<HTMLElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const [findOpen, setFindOpen] = useState(false);
  // Each ⌥⌘F or /, which gives the find the keyboard once what it asks for is laid out.
  const [findAsk, setFindAsk] = useState(0);
  const findShown = findOpen || query !== '';
  const openFind = useCallback(() => {
    // The find is over the list: a panel taking the whole window (⇧⌘F) gives the list its place
    // back. Only Sessions' and PRs' panels do.
    if (boardRef.current?.getClientRects().length === 0) (view === 'prs' ? setPrPanelMax : setPanelMax)(false);
    setFindOpen(true);
    setFindAsk((n) => n + 1);
  }, [view, setPanelMax, setPrPanelMax]);
  // Asked for, or the board read once since: the keyboard goes to it, with its words ready to type over.
  useLayoutEffect(() => {
    if (!findOpen) return;
    searchRef.current?.focus();
    searchRef.current?.select();
  }, [findOpen, findAsk, loading]);
  return { boardRef, searchRef, findShown, openFind, setFindOpen };
}
