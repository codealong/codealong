// Codealong's own slash commands, typed at the start of a session's box: they act on the session
// from here instead of going to it. Only these names are Codealong's; any other `/…` (Claude
// Code's /compact, say) goes to the session as typed, which runs it where it can. That takes
// `/btw` and `/side` away from the harnesses' own, on purpose: typed into a session from here,
// theirs would answer in a terminal you aren't looking at.

import { shortOf } from './devices';
import type { Session } from './types';

export interface SlashCommand {
  name: string;
  /** Other names it goes by: what the harnesses call their own. */
  aliases?: string[];
  /** What may follow the name, as the hint shows it. */
  args: string;
  /** What it does, in a few words, as the hint shows it. */
  about: string;
  /** The box's send button shows this while the command is typed, so ⏎ reads as running it. */
  icon: string;
  /** Whether the session's harness can do it. */
  takes: (s: Session) => boolean;
}

export const COMMANDS: SlashCommand[] = [
  {
    name: 'fork',
    args: '[prompt]',
    about: 'a copy of it goes on in the background, starting on the prompt if you give one',
    icon: 'git-fork',
    // Claude Code forks into a background job, OpenCode in its service, Codex in Codealong.
    takes: (s) => s.harness === 'claude' || s.harness === 'codex' || s.harness === 'opencode',
  },
  {
    name: 'side',
    // Claude Code's own is /btw, Codex's /side.
    aliases: ['btw'],
    args: '[question]',
    about: 'a throwaway copy answers here; the session carries on, unaware. Alone, it shows the earlier ones',
    icon: 'help',
    // The copy runs beside the session, on its Mac (side.rs).
    takes: (s) => (s.harness === 'claude' || s.harness === 'codex') && !shortOf(s.key),
  },
];

const named = (name: string) => (c: SlashCommand) => c.name === name || !!c.aliases?.includes(name);

/** The command `text` is, with what follows its name (its lines kept); null when it isn't one of
 *  Codealong's commands the session takes. */
export function slashCommand(text: string, s: Session): { command: SlashCommand; rest: string } | null {
  const m = /^\/([a-z-]+)(?:\s([\s\S]*))?$/.exec(text.trim());
  const command = m && COMMANDS.find((c) => named(m[1])(c) && c.takes(s));
  return command ? { command, rest: (m[2] ?? '').trim() } : null;
}

/** The commands what's typed so far could be becoming, for the hint under the box: a `/` and the
 *  start of a name, or a whole name and what follows it. */
export function slashMatches(text: string, s: Session): SlashCommand[] {
  const whole = slashCommand(text, s);
  if (whole) return [whole.command];
  const m = /^\/([a-z-]*)$/.exec(text.trimStart());
  return m ? COMMANDS.filter((c) => [c.name, ...(c.aliases ?? [])].some((n) => n.startsWith(m[1])) && c.takes(s)) : [];
}
