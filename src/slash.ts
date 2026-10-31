// Codealong's own slash commands, typed at the start of a session's box: they act on the session
// from here instead of going to it. Only these names are Codealong's; any other `/…` (Claude
// Code's /compact, say) goes to the session as typed, which runs it where it can. That takes
// `/btw` and `/side` away from the harnesses' own, on purpose: typed into a session from here,
// theirs would answer in a terminal you aren't looking at. In a side conversation's box, where
// what's typed goes to the copy, they run too, and those with words for it there are offered
// (`aside`): `/fork` has a copy of the session take on what was worked out on the side, which
// neither harness's own `/fork` does from a side conversation (a headless copy has none).

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
  /** What it takes and does typed in a side conversation's box, where it's offered only with
   *  these. */
  aside?: { args: string; about: string };
}

export const COMMANDS: SlashCommand[] = [
  {
    name: 'fork',
    args: '[prompt]',
    about: 'a copy of it goes on in the background, starting on the prompt if you give one',
    // The copy is told what was said in the side conversation shown (side.rs `fork_prompt`).
    aside: { args: 'what to do', about: 'a copy of the session does it in the background, told what was said here' },
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
    // The copy runs beside the session, on its machine (side.rs), wherever it's asked from.
    takes: (s) => s.harness === 'claude' || s.harness === 'codex',
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

/** The commands what's typed so far could be becoming, for what the box says of them: a `/` and the
 *  start of a name, or a whole name and what follows it. In a side conversation's box (`aside`),
 *  only those with words for it there. */
export function slashMatches(text: string, s: Session, aside = false): SlashCommand[] {
  const offered = (c: SlashCommand) => !aside || !!c.aside;
  const whole = slashCommand(text, s);
  if (whole) return offered(whole.command) ? [whole.command] : [];
  const m = /^\/([a-z-]*)$/.exec(text.trimStart());
  return m ? COMMANDS.filter((c) => [c.name, ...(c.aliases ?? [])].some((n) => n.startsWith(m[1])) && c.takes(s) && offered(c)) : [];
}

/** What the box says a command takes and does: in a side conversation's box (`aside`), what it
 *  does there. */
export function commandWords(c: SlashCommand, aside: boolean): { args: string; about: string } {
  return aside && c.aside ? c.aside : { args: c.args, about: c.about };
}
