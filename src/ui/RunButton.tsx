// Running a command from a session's conversation: a button beside a shell block's Copy that types
// its commands into a new terminal (runcode.rs), in the folder the session works in or the one the
// command ran in, and says for a moment that it did.

import { createContext, useEffect, useState } from 'react';
import { runInTerminal } from '../api';
import { tildePath } from '../format';
import type { Lane, Session } from '../types';
import { Icon } from './primitives';

/** Where a session's commands run: its folder on this Mac, and the terminal that opens there. A
 *  session's thread gives it to its code blocks; a session on another device has none, and its
 *  blocks no Run. */
export interface RunPlace {
  folder: string;
  terminal: string;
}

export const RunContext = createContext<RunPlace | null>(null);

/** The folder a session's commands run in: the worktree it's shown in, or its own folder when
 *  that's inside the worktree (a subfolder, or a chat's own folder in the lane of an app's chats). */
export function runFolder(lane: Pick<Lane, 'path'>, s: Pick<Session, 'cwd'>): string {
  const inside = s.cwd === lane.path || s.cwd.startsWith(`${lane.path.replace(/\/$/, '')}/`);
  return s.cwd && inside ? s.cwd : lane.path;
}

export function RunButton({ text, folder, terminal, onFailed }: { text: string; folder: string; terminal: string; onFailed: (why: string | null) => void }) {
  const [state, setState] = useState<'going' | 'done' | null>(null);
  useEffect(() => {
    if (state !== 'done') return;
    const id = window.setTimeout(() => setState(null), 1500);
    return () => window.clearTimeout(id);
  }, [state]);
  const title = `Run in ${terminal}, in ${tildePath(folder)}`;
  return (
    <button
      type="button"
      className={['ca-copybtn', state && `is-${state}`].filter(Boolean).join(' ')}
      title={state === 'done' ? `Opened in ${terminal}` : title}
      aria-label={title}
      disabled={state === 'going'}
      onClick={(e) => {
        e.stopPropagation();
        setState('going');
        onFailed(null);
        runInTerminal(folder, text).then(
          () => setState('done'),
          (err: unknown) => {
            setState(null);
            onFailed(String(err).replace(/^Error: /, ''));
          },
        );
      }}
    >
      <Icon name={state === 'done' ? 'check' : 'player-play'} size={13} stroke={state === 'done' ? 2.25 : 1.75} />
      {state && <span>{state === 'going' ? 'Opening…' : 'Opened'}</span>}
    </button>
  );
}
