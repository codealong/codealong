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
  return s.surface === 'cli' ? terminal : HARNESS_APP[s.harness];
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
