// Improve Codealong: a session that improves Codealong itself, from the button beside New
// session, its shortcut (`[shortcuts] improve`) or the phone. The quick panel opens on
// Codealong's own project and worktree (the board's `improve`, from the engine's improve.rs),
// with a picture of every Codealong window as it was (`QuickOpen.improve.shots`, kept as
// attachments by the engine), asks what to change, and starts the session with the pictures in
// its prompt. These are the words and the pure rules; Quick.tsx shows them.

import type { Attachment, Config, Improve, ImproveSource, Lane } from './types';

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

/** Where Codealong's source is published, for a clone. */
export const CODEALONG_REPO = 'https://github.com/codealong/codealong';

/**
 * The prompt for a session that sets this machine up with Codealong Dev, a build of Codealong's
 * source, in place of the installed app: where the source is, or the folder to clone it into
 * (`scan`, the folders Codealong looks for repos in, so it comes onto the board), then the
 * repo's own notes for building it, so the steps stay the repo's.
 */
export function devSetupPrompt(source: string | null, scan: string[]): string {
  const where = source
    ? `Codealong’s source is already in ${source}: build from it, bringing it up to date with GitHub first where that loses nothing.`
    : scan.length === 1
      ? `Clone ${CODEALONG_REPO} into ${scan[0]}, the folder Codealong looks for repos in, so it comes onto Codealong’s board.`
      : scan.length > 1
        ? `Clone ${CODEALONG_REPO} into one of the folders Codealong looks for repos in (${scan.join(', ')}), so it comes onto Codealong’s board.`
        : `Clone ${CODEALONG_REPO} into the folder where I keep my code (ask me if you can’t tell), and add that folder to \`scan\` in Codealong’s config.toml so Codealong finds it.`;
  return [
    'Set this machine up to run Codealong Dev, a development version of Codealong built from its source, in place of the installed Codealong, so I can change Codealong and run the changes.',
    where,
    'Its docs/development.md says what building it needs and how. Install what this machine is missing for it, and tell me when something needs me, such as a password or a dialog to click. Then run scripts/relaunch.sh in the checkout: it builds Codealong Dev, puts it in Applications and opens it, and the installed Codealong quits as it opens. The two share settings and todos.',
    'Once Codealong Dev is running, tell me so, and that opening Codealong from Applications goes back to the installed one. From then on, Improve Codealong in Codealong Dev starts its sessions in this checkout, and scripts/relaunch.sh runs what they change.',
  ].join('\n\n');
}

/** Codealong's source among your other Macs' worktrees (the devices' lanes, each id `@<short>:…`),
 *  for the panel to go to when this Mac has none: a main checkout first, a shown one before a
 *  hidden one, a connected Mac's first. The pictures go there with the prompt (actions.rs
 *  `carry_images`). */
export function remoteImproveLane(lanes: Lane[]): Lane | null {
  const sources = lanes.filter((l) => l.codealong && !l.startsChats);
  const pick = (from: Lane[]) => from.find((l) => l.origin === 'main' && !l.hidden) ?? from.find((l) => !l.hidden) ?? from[0] ?? null;
  return pick(sources.filter((l) => l.device?.online)) ?? pick(sources);
}

/** What the panel says when Codealong's source isn't on this Mac's board in one of your projects:
 *  the words, and whether a project can be made for it (its source was found here). Nothing while
 *  the board is still being read, or when the source is in a project. `remote` is where another of
 *  your Macs has it, when one does. */
export function improveWords(improve: Improve | undefined, loading: boolean, remote: Lane | null = null): { text: string; add: boolean } | null {
  if (loading || improve?.project) return null;
  if (improve?.source) return { text: `Codealong’s source is in ${improve.source.root}, in none of your projects. A project for it puts its worktrees on the board, where this session starts.`, add: true };
  if (remote) {
    const mac = remote.device?.name ?? 'another Mac';
    return { text: `Codealong’s source is on ${mac}, not on this Mac, so the session starts there and the pictures go with it. To start here, clone github.com/codealong/codealong into a folder Codealong searches (Settings › General lists them).`, add: false };
  }
  return { text: 'Codealong’s source isn’t in a folder Codealong searches. Clone github.com/codealong/codealong into one (Settings › General lists them), and a session can improve it from here.', add: false };
}
