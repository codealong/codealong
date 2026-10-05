// The skills the engine lists (skills.rs), read for a window when a slash is typed and kept a
// little while: a session's for its reply box, a project's repos' for the quick panel.

import { useEffect, useState } from 'react';
import { listSkills } from './api';
import { shortOf } from './devices';
import type { SkillsListed } from './types';

/** How long a list is used before it's read again: a skill added a moment ago should show. */
const FRESH_MS = 20_000;

const kept = new Map<string, { at: number; listed: SkillsListed }>();

/** What the engine lists for these folders and accounts, once `wanted`; the last list for them
 *  meanwhile, and null before any. Read again when it's a while old and wanted again. */
export function useSkills(folders: string[], accounts: string[], wanted: boolean): SkillsListed | null {
  const key = `${folders.join('\n')}\n\n${accounts.join('\n')}`;
  const [, redraw] = useState(0);
  useEffect(() => {
    // The effect goes by `key`, which stands for the folders and accounts.
    const [f, a] = key.split('\n\n').map((part) => (part ? part.split('\n') : []));
    if (!wanted || (f.length === 0 && a.length === 0)) return;
    const have = kept.get(key);
    if (have && Date.now() - have.at < FRESH_MS) return;
    let live = true;
    listSkills(f, a)
      .then((listed) => {
        // The preview's stub answers null for what it doesn't know.
        kept.set(key, { at: Date.now(), listed: listed ?? { folders: [], accounts: [] } });
        if (live) redraw((n) => n + 1);
      })
      // Without the list the box still sends what's typed, as it did before there was one; a
      // skill's name then goes as typed.
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [key, wanted]);
  return kept.get(key)?.listed ?? null;
}

/** The folder and account to list a session's skills for: none for one on another device,
 *  whose folders aren't this Mac's. */
export function sessionSkillsFor(s: { key: string; cwd: string; account: string }): { folders: string[]; accounts: string[] } {
  return shortOf(s.key) || !s.cwd ? { folders: [], accounts: [] } : { folders: [s.cwd], accounts: [s.account] };
}
