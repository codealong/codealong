// What was said in sessions, looked for as you type: this Mac's in the engine (search.rs), and
// with `devices`, each linked device's on that device (peers.rs `search`), once typing pauses.
// The quick panel and the main window's search both find a session by it as well as by what
// describes it. Until the search for the words typed now answers, the last one's hits whose lines
// still have every word stay (`stillSaid`), so a session found by what was said doesn't drop out
// of the list with each key and come back when the answer does.

import { useEffect, useMemo, useState } from 'react';
import { searchDevices, searchText } from './api';
import { deviceHits, queryWords, stillSaid, TEXT_SEARCH_MIN } from './quick';
import type { TextFound, TextHit } from './types';

/** How long typing has to pause before what's typed is looked for in what sessions said. */
const TEXT_AFTER_MS = 150;

export interface TextSearch {
  /** The words looked for, as one string; null while too little is typed to look. */
  words: string | null;
  /** The hits by session key, other devices' keyed as their sessions are here (`@<short>:`). */
  said: ReadonlyMap<string, TextHit>;
  /** This Mac's answer for the words typed now, once it's in. */
  found: TextFound | null;
  /** This Mac or another device is still reading what its sessions said for the first time. */
  building: boolean;
}

const NONE: ReadonlyMap<string, TextHit> = new Map();

export function useTextSearch(query: string, { ready, devices }: { ready: boolean; devices: boolean }): TextSearch {
  const words = ready && query.replace(/\s+/g, '').length >= TEXT_SEARCH_MIN ? queryWords(query).join(' ') : null;
  const [here, setHere] = useState<{ for: string; found: TextFound } | null>(null);
  const [there, setThere] = useState<{ for: string; hits: TextHit[]; building: boolean } | null>(null);
  useEffect(() => {
    if (!words) return;
    let live = true;
    const id = window.setTimeout(() => {
      searchText(words.split(' '))
        .then((found) => live && setHere({ for: words, found }))
        // What matches by title still shows, and the search is asked again as typing goes on.
        .catch(() => undefined);
      if (devices)
        searchDevices(words.split(' '))
          .then((found) => live && setThere({ for: words, ...deviceHits(found) }))
          // As above: their sessions are still found by what describes them.
          .catch(() => undefined);
    }, TEXT_AFTER_MS);
    return () => {
      live = false;
      window.clearTimeout(id);
    };
  }, [words, devices]);
  const found = words && here?.for === words ? here.found : null;
  const said = useMemo(() => {
    if (!words) return NONE;
    const typed = queryWords(words);
    // Each source's answer for these words, else what's left of its last one.
    const mine = found ? found.hits : here ? stillSaid(here.found.hits, typed) : [];
    const theirs = !devices || !there ? [] : there.for === words ? there.hits : stillSaid(there.hits, typed);
    return new Map([...mine, ...theirs].map((h) => [h.key, h] as const));
  }, [words, found, here, there, devices]);
  const building = !!found?.building || (devices && !!words && there?.for === words && there.building);
  return { words, said, found, building };
}
