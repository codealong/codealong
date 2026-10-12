// Improve Codealong: a session that improves Codealong itself, from the button beside New
// session, its shortcut (`[shortcuts] improve`) or the phone. The quick panel opens on
// Codealong's own project and worktree (the board's `improve`, from the engine's improve.rs),
// with a picture of every Codealong window as it was (`QuickOpen.improve.shots`, kept as
// attachments by the engine), asks what to change, and starts the session with the pictures in
// its prompt. These are the words and the pure rules; Quick.tsx shows them.

import type { Attachment, Config, Improve, ImproveSource } from './types';

/** What the empty input asks. */
export const IMPROVE_PLACEHOLDER = 'Describe how to improve Codealong…';

/** The project made for Codealong's source when it's in none of yours. */
export const CODEALONG_PROJECT = 'Codealong';

/** The button's tooltip, with its shortcut (the shortcut's label, or none when it's off). */
export function improveTitle(shortcut: string | null | undefined): string {
  const keys = shortcut ? ` (${shortcut})` : '';
  return `Improve Codealong: describe what to change, with pictures of its windows as they are now${keys}`;
}

/**
 * The prompt a session that improves Codealong starts on: what you wrote, then the pictures of
 * the windows, each named for its window, so the agent sees what you saw. Without pictures,
 * what you wrote alone.
 */
export function improvePrompt(text: string, shots: Attachment[]): string {
  const words = text.trim();
  if (shots.length === 0) return words;
  const lead = 'Codealong’s windows as they were when this was asked:';
  const lines = shots.map((s) => `![${s.name.replace(/[[\]\n]/g, '')}](${s.path})`).join('\n');
  return words ? `${words}\n\n${lead}\n${lines}` : `${lead}\n${lines}`;
}

/**
 * config.toml with a project for Codealong's source: a new `[[projects]]` named Codealong with
 * its repo (`github.com/owner/name`), or its folder when it has no remote; one already named
 * Codealong gets the repo or folder added instead.
 */
export function withCodealongProject(cfg: Config, source: ImproveSource): Config {
  const repos = source.repo ? [source.repo] : [];
  const folders = source.repo ? [] : [source.root];
  const have = cfg.projects.find((p) => p.name === CODEALONG_PROJECT);
  if (!have) return { ...cfg, projects: [...cfg.projects, { name: CODEALONG_PROJECT, repos, folders }] };
  const merged = {
    ...have,
    repos: [...have.repos, ...repos.filter((r) => !have.repos.includes(r))],
    folders: [...have.folders, ...folders.filter((f) => !have.folders.includes(f))],
  };
  return { ...cfg, projects: cfg.projects.map((p) => (p === have ? merged : p)) };
}

/** What the panel says when Codealong's source isn't in one of your projects: the words, and
 *  whether a project can be made for it (its source was found). Nothing while the board is still
 *  being read, or when the source is in a project. */
export function improveWords(improve: Improve | undefined, loading: boolean): { text: string; add: boolean } | null {
  if (loading || improve?.project) return null;
  if (improve?.source) return { text: `Codealong’s source is in ${improve.source.root}, in none of your projects. A project for it puts its worktrees on the board, where this session starts.`, add: true };
  return { text: 'Codealong’s source isn’t in a folder Codealong searches. Clone github.com/codealong/codealong into one (Settings › General lists them), and a session can improve it from here.', add: false };
}
