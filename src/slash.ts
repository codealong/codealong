// Codealong's own slash commands, typed at the start of a session's box: they act on the session
// from here instead of going to it. Only these names are Codealong's; any other `/…` (Claude
// Code's /compact, say) goes to the session as typed, which runs it where it can.

import type { Session } from './types';

export interface SlashCommand {
  name: string;
  /** What may follow the name, as the hint shows it. */
  args: string;
  /** What it does, in a few words, as the hint shows it. */
  about: string;
  /** Whether the session's harness can do it. */
  takes: (s: Session) => boolean;
}

export const COMMANDS: SlashCommand[] = [
  {
    name: 'fork',
    args: '[prompt]',
    about: 'a copy of it goes on in the background, starting on the prompt if you give one',
    // Claude Code forks into a background job, OpenCode in its service, Codex in Codealong.
    takes: (s) => s.harness === 'claude' || s.harness === 'codex' || s.harness === 'opencode',
  },
];

/** The command `text` is, with what follows its name (its lines kept); null when it isn't one of
 *  Codealong's commands the session takes. */
export function slashCommand(text: string, s: Session): { command: SlashCommand; rest: string } | null {
  const m = /^\/([a-z-]+)(?:\s([\s\S]*))?$/.exec(text.trim());
  const command = m && COMMANDS.find((c) => c.name === m[1] && c.takes(s));
  return command ? { command, rest: (m[2] ?? '').trim() } : null;
}

/** The commands what's typed so far could be becoming, for the hint under the box: a `/` and the
 *  start of a name, or a whole name and what follows it. */
export function slashMatches(text: string, s: Session): SlashCommand[] {
  const whole = slashCommand(text, s);
  if (whole) return [whole.command];
  const m = /^\/([a-z-]*)$/.exec(text.trimStart());
  return m ? COMMANDS.filter((c) => c.name.startsWith(m[1]) && c.takes(s)) : [];
}
