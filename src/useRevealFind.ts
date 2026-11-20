// What the find bar in a session's panel opens on in the main window, wherever the panel shows it
// (App.tsx): for a session this window's search found by what was said in it, the words it found
// there; else the words the quick panel found it by (`Reveal.find`), until another session is
// selected or a search is typed here, which would otherwise hand the bar that ask again once it's
// cleared. Each reveal is a new ask, so showing the session again opens a bar that was closed.

import { useCallback, useEffect, useState } from 'react';
import { queryWords } from './quick';
import { findFor } from './threadFind';
import type { FindAsk } from './ui/ThreadFind';

/** What the window has for it. */
export interface RevealFindInput {
  /** The session selected, by its key. */
  selectedSessionKey: string | undefined;
  /** The list's find, and the line each session was found by in what was said. */
  query: string;
  saidOf: (key: string) => { text: string } | undefined;
}

export function useRevealFind({ selectedSessionKey, query, saidOf }: RevealFindInput) {
  const [revealFind, setRevealFind] = useState<{ key: string; ask: FindAsk } | null>(null);
  useEffect(() => {
    if (revealFind && (query || selectedSessionKey !== revealFind.key)) setRevealFind(null);
  }, [revealFind, selectedSessionKey, query]);
  const findIn = useCallback(
    (key: string): FindAsk | null => {
      const said = saidOf(key);
      if (said) return { words: findFor(said.text, queryWords(query)), n: 0 };
      return revealFind?.key === key ? revealFind.ask : null;
    },
    [saidOf, query, revealFind],
  );
  return { findIn, setRevealFind };
}
