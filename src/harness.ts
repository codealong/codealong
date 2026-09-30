// The harnesses Codealong reads, by name: how the UI says each one, and the app a session
// opens in when it isn't in the terminal.

import type { Harness, Session, Usage } from './types';

export const HARNESSES: Harness[] = ['claude', 'codex', 'opencode', 'antigravity'];

/** The harness's name as the UI says it. */
export const HARNESS_WORD: Record<Harness, string> = { claude: 'Claude Code', codex: 'Codex', opencode: 'OpenCode', antigravity: 'Antigravity' };

/** The app a harness's sessions open in when they don't live in the terminal. */
export const HARNESS_APP: Record<Harness, string> = { claude: 'Claude app', codex: 'Codex', opencode: 'OpenCode', antigravity: 'Antigravity' };

/** Where a session opens: the terminal by name, or the harness's app. */
export function sessionPlace(s: Session, terminal: string): string {
  if (s.hosted) return 'Codealong';
  return s.surface === 'cli' ? terminal : HARNESS_APP[s.harness];
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

/** Where a harness keeps a name given here, in a few words for the rename box. */
export const NAME_KEPT: Record<Harness, string> = {
  claude: 'Claude Code keeps it with the session, for its terminal and /resume.',
  codex: 'Codex keeps it with the thread, for the Codex app.',
  opencode: 'OpenCode keeps it with the session, for its terminal.',
  antigravity: '',
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
  if (u.harness === 'opencode' || u.harness === 'antigravity') return 'No limits to show';
  return 'Usage unavailable';
}
