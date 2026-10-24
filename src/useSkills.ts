// The skills the engine lists (skills.rs), read for a window when a slash is typed and kept a
// little while: a session's for its reply box, read on the device it runs on, and a project's
// repos' for the quick panel.

import { useEffect, useState } from 'react';
import { listSkills, sessionSkills } from './api';
import { useLatest } from './latest';
import type { SkillsListed } from './types';

/** How long a list is used before it's read again: a skill added a moment ago should show. */
const FRESH_MS = 20_000;

const kept = new Map<string, { at: number; listed: SkillsListed }>();

/** What the engine lists for these folders and accounts, once `wanted`; the last list for them
 *  meanwhile, and null before any. Read again when it's a while old and wanted again. */
export function useSkills(folders: string[], accounts: string[], wanted: boolean): SkillsListed | null {
  const key = `${folders.join('\n')}\n\n${accounts.join('\n')}`;
  return useKept(folders.length === 0 && accounts.length === 0 ? null : key, wanted, () => listSkills(folders, accounts));
}

/** The skills a session's box offers, as `useSkills` keeps them: its folder's and its account's,
 *  as the device it runs on reads them, this one or another. */
export function useSessionSkills(sessionKey: string, wanted: boolean): SkillsListed | null {
  // A folder's path never starts so.
  return useKept(`session:${sessionKey}`, wanted, () => sessionSkills(sessionKey));
}

/** What `read` lists, kept as `key`: read once `wanted`, again when it's a while old. */
function useKept(key: string | null, wanted: boolean, read: () => Promise<SkillsListed>): SkillsListed | null {
  const [, redraw] = useState(0);
  // What `key` stands for, as the latest render reads it.
  const reads = useLatest(read);
  useEffect(() => {
    if (!wanted || key === null) return;
    const have = kept.get(key);
    if (have && Date.now() - have.at < FRESH_MS) return;
    let live = true;
    reads
      .current()
      .then((listed) => {
        // The preview's stub answers null for what it doesn't know.
        kept.set(key, { at: Date.now(), listed: listed ?? { folders: [], accounts: [] } });
        if (live) redraw((n) => n + 1);
      })
      // Without the list the box still sends what's typed, as it did before there was one; a
      // skill's name then goes as typed. So does a session's box on a device from before it
      // could be asked.
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [key, wanted, reads]);
  return key === null ? null : (kept.get(key)?.listed ?? null);
}
