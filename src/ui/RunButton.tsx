// Running a command from a session's conversation: a button beside a shell block's Copy that runs
// its commands in a new tab of your terminal, as you, in the folder the session works in or the
// one the command ran in, says for a moment that it did, and has the session told what they
// printed once they end (actions.rs `run_command`), as `!` in the session's box does: an agent
// writes a command for you to run because it wants what it prints, whatever its harness. The tab
// closes by itself a while after they end (`[apps] close_command_tabs_after`) unless you take a
// shell there. A block with no language is a shell block when each of its lines starts with a
// command this Mac has or a script by its path from where it runs (`isCommandBlock`), as agents
// often write one for you to run, or when it's led by `!`, as Claude Code's agents write one.

import { createContext, useEffect, useState } from 'react';
import { shellCommands } from '../api';
import { tildePath } from '../format';
import type { Lane, Session } from '../types';
import { cx, Icon } from './primitives';
import { errorText } from '../report';

/** Where a session's commands run: its folder on this Mac, and the terminal that opens there. A
 *  session's thread gives it to its code blocks; a session on another device has none, and its
 *  blocks no Run. `run` runs a command for the session in `folder`, the session told what it
 *  printed once it ends (api.ts `runCommand`): a `block`'s commands without their comments, or
 *  else one led by `!` as the session's box runs it. */
export interface RunPlace {
  folder: string;
  terminal: string;
  run: (command: string, folder: string, block: boolean) => Promise<string>;
}

export const RunContext = createContext<RunPlace | null>(null);

/** The folder a session's commands run in: the worktree it's shown in, or its own folder when
 *  that's inside the worktree (a subfolder, or a chat's own folder in the lane of an app's chats). */
export function runFolder(lane: Pick<Lane, 'path'>, s: Pick<Session, 'cwd'>): string {
  const inside = s.cwd === lane.path || s.cwd.startsWith(`${lane.path.replace(/\/$/, '')}/`);
  return s.cwd && inside ? s.cwd : lane.path;
}

/** Whether what's in a session's box is a command led by `!`, which runs in a tab of yours for
 *  the session to be told what it printed (actions.rs `run_command`). Not text that opens with an
 *  image, whose `!` is the image's (`![shot](images/…)`). */
export function bangCommand(text: string): boolean {
  const t = text.trimStart();
  return t.startsWith('!') && !t.startsWith('![') && t.slice(1).trim().length > 0;
}

/** A word that sets a variable for the command after it: `NODE_ENV=test npm test`. */
const ASSIGNMENT = /^[A-Za-z_][A-Za-z0-9_]*=\S*\s+/;
/** A command's name, ended by a space, the line's end or what joins it to the next command. */
const NAME = /^([A-Za-z0-9_][A-Za-z0-9._+-]*)(?=$|[\s;&|<>])/;
/** A script named by its path from the folder it runs in, with what it's given after it:
 *  `../scripts/deploy.mjs plan --note "…"`, as agents write one for you to run. Alone, a path is
 *  as often a file named as a script run (as `find` prints them), and one led by `/` or `~` as
 *  often output that names a file, so neither counts. */
const SCRIPT = /^\.\.?\/[\w.+@/-]*[\w.+-](?=\s+\S|\s*[;&|<>])/;
/** A line the next one carries on: an escaped newline, or a pipe or `&&` left open. */
const CARRIES_ON = /(\\|\||&&)$/;

/** Whether a block that doesn't say it's shell is commands all the same: each of its lines,
 *  past comments and blank ones and those a line before carries on into, starts with a command
 *  in `known` (runcode.rs `commands`) or a script given arguments (`SCRIPT`). Output, a path or
 *  a config file doesn't, nor a command shown with what it printed. */
export function isCommandBlock(text: string, known: ReadonlySet<string>): boolean {
  let commands = 0;
  let carried = false;
  for (const line of text.split('\n')) {
    const t = line.trim();
    if (carried) {
      carried = CARRIES_ON.test(t);
      continue;
    }
    if (!t || t.startsWith('#')) continue;
    let rest = t;
    while (ASSIGNMENT.test(rest)) rest = rest.replace(ASSIGNMENT, '');
    const name = NAME.exec(rest)?.[1];
    if (!(name && known.has(name)) && !SCRIPT.test(rest)) return false;
    commands++;
    carried = CARRIES_ON.test(t);
  }
  return commands > 0;
}

// One ask for every block in a window, again once the answer is a minute old: a program the
// agent had you install shows up for the blocks after it.
let asked: { at: number; known: Promise<ReadonlySet<string>> } | null = null;

/** What a new terminal's shell takes as a command's first word, once the engine has said, when
 *  `wanted`. */
export function useShellCommands(wanted: boolean): ReadonlySet<string> | null {
  const [known, setKnown] = useState<ReadonlySet<string> | null>(null);
  useEffect(() => {
    if (!wanted) return;
    if (!asked || Date.now() - asked.at > 60_000) {
      // A failed ask leaves such blocks with Copy alone, as they were before Run guessed at them.
      asked = { at: Date.now(), known: shellCommands().then((names) => new Set(names), () => new Set<string>()) };
    }
    let live = true;
    void asked.known.then((k) => {
      if (live) setKnown(k);
    });
    return () => {
      live = false;
    };
  }, [wanted]);
  return known;
}

/** Run for a block, in `place`'s folder: the session's, or the one its command ran in. `block`
 *  as `RunPlace.run` takes it. */
export function RunButton({ text, block, place, onFailed }: { text: string; block: boolean; place: RunPlace; onFailed: (why: string | null) => void }) {
  const [state, setState] = useState<'going' | 'done' | null>(null);
  useEffect(() => {
    if (state !== 'done') return;
    const id = window.setTimeout(() => setState(null), 1500);
    return () => window.clearTimeout(id);
  }, [state]);
  const title = `Run in a new ${place.terminal} tab in ${tildePath(place.folder)}, as you: the session is told what it prints once it ends`;
  const doneTitle = `Running in ${place.terminal}: the session is told what it prints once it ends`;
  return (
    <button
      type="button"
      className={cx('ca-copybtn', state && `is-${state}`)}
      title={state === 'done' ? doneTitle : title}
      aria-label={title}
      disabled={state === 'going'}
      onClick={(e) => {
        e.stopPropagation();
        setState('going');
        onFailed(null);
        place.run(text, place.folder, block).then(
          () => setState('done'),
          (err: unknown) => {
            setState(null);
            onFailed(errorText(err));
          },
        );
      }}
    >
      <Icon name={state === 'done' ? 'check' : 'player-play'} size={13} stroke={state === 'done' ? 2.25 : 1.75} />
      {state && <span>{state === 'going' ? 'Starting…' : 'Running'}</span>}
    </button>
  );
}
