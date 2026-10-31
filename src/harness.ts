// The harnesses Codealong reads, by name: how the UI says each one, and the app a session
// opens in when it isn't in the terminal.

import type { Harness, Session, Usage } from './types';

export const HARNESSES: Harness[] = ['claude', 'codex', 'opencode', 'antigravity', 'cursor'];

/** The harness's name as the UI says it. */
export const HARNESS_WORD: Record<Harness, string> = { claude: 'Claude Code', codex: 'Codex', opencode: 'OpenCode', antigravity: 'Antigravity', cursor: 'Cursor' };

/** The app a harness's sessions open in when they don't live in the terminal. */
export const HARNESS_APP: Record<Harness, string> = { claude: 'Claude app', codex: 'Codex', opencode: 'OpenCode', antigravity: 'Antigravity', cursor: 'Cursor' };

/** Where a session opens: the terminal by name (the one it runs in, else config.toml's), or the
 *  harness's app. */
export function sessionPlace(s: Session, terminal: string): string {
  if (s.hosted) return 'Codealong';
  return s.surface === 'cli' ? (s.terminal ?? terminal) : HARNESS_APP[s.harness];
}

/** The harness a session key names: `<harness>:<account>:<id>`, or `@<device>:` and that for a
 *  session on another device. */
export function harnessOfKey(key: string): Harness | null {
  const own = key.startsWith('@') ? key.slice(key.indexOf(':') + 1) : key;
  const h = own.slice(0, own.indexOf(':')) as Harness;
  return HARNESSES.includes(h) ? h : null;
}

/** Whether Codealong can name a session where its harness keeps names (harness.rs `rename`):
 *  Claude Code, Codex and OpenCode can, though Codex not an archived thread. */
export function renames(s: { key: string; archived?: boolean }): boolean {
  const h = harnessOfKey(s.key);
  return h === 'claude' || h === 'opencode' || (h === 'codex' && !s.archived);
}

/** The harnesses whose sessions move to another of their accounts (harness.rs `move_to`,
 *  `moves_accounts`): Claude Code, which resumes another account's transcript by its path. */
export const MOVES_ACCOUNTS: readonly Harness[] = ['claude'];

/** The harnesses that hold a conversation of their own, which answers questions about a PR in its
 *  card's box (harness.rs `fresh_side`): Claude Code, headless, and Codex, on an app-server. */
export const FRESH_SIDES: readonly Harness[] = ['claude', 'codex'];

/** The harnesses whose accounts draw session emblems (harness.rs `draws`): Codex, with its image
 *  tool. */
export const DRAWS: readonly Harness[] = ['codex'];

/** The harnesses whose sessions fork from before a prompt, your edit of it in its place (harness.rs
 *  `fork_from`): Claude Code and Codex through the copy Codealong runs, OpenCode in its service. */
export const EDITS: readonly Harness[] = ['claude', 'codex', 'opencode'];

/** The harnesses whose running turn takes in a message they hold once the step it's on ends,
 *  where the others hold it until the turn ends (send.rs `Delivery::busy`): Claude Code and
 *  OpenCode. */
export const TAKES_BETWEEN_STEPS: readonly Harness[] = ['claude', 'opencode'];

/** What a message the session's harness holds waits for, as the end of a sentence. */
export function heldUntil(harness: Harness | null): string {
  return harness && TAKES_BETWEEN_STEPS.includes(harness) ? 'its current step ends' : 'this turn ends';
}

/** What Send now on a message its harness holds does, where that isn't steering it into the turn
 *  (harness.rs `steer_queued`): Claude Code's own send-now, which stops the step it's on. */
const SENDS_HELD_NOW: Partial<Record<Harness, string>> = {
  claude: 'Send now: Claude Code stops the step it’s on, a running command too, and goes on with what it holds, as its own send-now does',
};

/** What Send now on a message the session's harness holds does: its tooltip, which is all the
 *  icon has to name it, as Edit's and Remove's are. */
export function sendsHeldNow(harness: Harness | null): string {
  return (harness && SENDS_HELD_NOW[harness]) || `Send now: into the turn it’s taking, rather than once ${heldUntil(harness)}`;
}

/** What taking a message out of what its harness holds does to the rest, where it does anything
 *  (harness.rs `remove_queued`): Claude Code gives back everything it holds at once. */
const TAKES_ALL_OUT: Partial<Record<Harness, string>> = {
  claude: 'Claude Code gives back everything it holds at once, so the others go back in after it, and anything typed in its terminal’s box goes with them',
};

const andTheRest = (harness: Harness | null, said: string) => (harness && TAKES_ALL_OUT[harness] ? `${said}. ${TAKES_ALL_OUT[harness]}` : said);

/** What Remove on a message the session's harness holds does, for its tooltip. */
export function removesHeld(harness: Harness | null): string {
  return andTheRest(harness, 'Remove, so it’s never sent');
}

/** What Edit on a message the session's harness holds does, for its tooltip. */
export function editsHeld(harness: Harness | null): string {
  return andTheRest(harness, 'Edit: it comes out of the queue into the box below, so it can’t go while you change it, and you send it from there');
}

/** The harnesses whose sessions move to another folder as themselves (harness.rs
 *  `move_folder`): Claude Code, which goes on with a session wherever `--resume` runs. */
export const MOVES_FOLDERS: readonly Harness[] = ['claude'];

/** Where a harness keeps a name given here, in a few words for the rename box. */
export const NAME_KEPT: Record<Harness, string> = {
  claude: 'Claude Code keeps it with the session, for its terminal and /resume.',
  codex: 'Codex keeps it with the thread, for the Codex app.',
  opencode: 'OpenCode keeps it with the session, for its terminal.',
  antigravity: '',
  cursor: '',
};

/** The longest name a session takes (harness.rs `NAME_MAX`). */
const NAME_MAX = 200;

/** A name as typed, made one line as the engine makes it (harness.rs `session_name`): control,
 *  line-break, direction and zero-width characters become spaces, then runs of spaces one; the
 *  ends are trimmed and it's cut at 200 characters. Empty when nothing's left. */
export function sessionName(raw: string): string {
  const spaced = raw.replace(/[\u{0}-\u{1f}\u{7f}-\u{9f}\u{61c}\u{200b}-\u{200f}\u{2028}-\u{202e}\u{2060}-\u{206f}\u{feff}]/gu, ' ');
  return Array.from(spaced.split(/\s+/).filter(Boolean).join(' ')).slice(0, NAME_MAX).join('').trimEnd();
}

/** A name as the board shows it (board.rs): backticks as quotes, cut at 90 characters. */
export function nameShown(name: string): string {
  const t = name.replace(/`/g, "'");
  const chars = Array.from(t);
  return chars.length <= 90 ? t : `${chars.slice(0, 89).join('').trimEnd()}…`;
}

/** Why an account shows no meters, when its harness has none to show. */
export function usageNote(u: Usage | undefined): string {
  if (!u) return 'No usage yet';
  if (u.needsSignIn) return 'Not signed in';
  if (u.loading) return 'Reading…';
  if (u.harness === 'codex') return 'No turns yet';
  if (u.noLimits) return 'No limits to show';
  return 'Usage unavailable';
}
