// What Codealong asks an agent to do for it: the sessions it offers to start with words of its
// own, to set something up on this machine. Each is an `AgentTask`, its prompt and where it
// starts, and every button that offers one opens the quick panel with it filled in (api.ts
// `askAgent`), in the worktree it names or a chat of its own, so you see what it asks and pick
// the agent that does it. The one asked from inside the panel (Switch to Codealong Dev?) starts
// there as a Return would. A prompt says what Codealong knows and leaves the steps to the docs it
// points at, so they're written in one place and follow what they describe.
//
// - Get the iPhone app, from Settings › Devices: `iphoneTask`.
// - Switch to Codealong Dev, from Improve Codealong in the installed app: `devSetupTask`.
// - Install an account's command line that isn't on this machine, from Settings › Accounts:
//   `installCliTask`.
// - Sync this machine's projects with another's, from that machine in Settings › Devices:
//   `syncProjectsTask`.
// - Update Codealong Dev to origin/main, from the sidebar once origin/main has commits the build
//   hasn't (updates.rs `dev_check`): `devUpdateTask`.

import { IPHONE_REPO } from './devices';
import { HARNESS_WORD } from './harness';
import { CODEALONG_REPO } from './improve';
import type { DevBehind, Harness, ProjectThere } from './types';

export interface AgentTask {
  /** What it asks. */
  prompt: string;
  /** The worktree it starts in, and its project; a chat of its own, in no project, when null. */
  laneId: string | null;
  project?: string | null;
}

/** A task for a chat of its own. */
function chat(prompt: string): AgentTask {
  return { prompt, laneId: null };
}

/** Installing Codealong for iPhone on your phone with you and linking it with this machine. The
 *  steps themselves are the iOS README's, which the agent reads there. */
export function iphoneTask(): AgentTask {
  return chat(
    [
      `Install Codealong for iPhone on my iPhone and link it with this Mac. It isn’t on the App Store yet: it’s built with Xcode and installed over a cable, as the “Get it on your iPhone” section of its README says (https://${IPHONE_REPO}).`,
      `Work in a clone of ${IPHONE_REPO} that’s on this Mac already (\`codealong worktrees\` lists the repos Codealong knows), pulled up to date. With none, clone it into one of the folders Codealong scans (\`scan\` in the config.toml \`codealong status\` names), so it shows in Codealong. Codealong’s Settings › Agents installs the \`codealong\` command when it isn’t there.`,
      'Install what the README says it needs and isn’t here, asking me before anything that wants my password. Walk me through what only I can do first (connecting the phone, Trust, Developer Mode, my Apple ID in Xcode) a step at a time, and wait for me to say each is done. Then run `scripts/install-device.sh`; the first build takes a while.',
      'Once it’s installed, walk me through trusting the developer on the phone, which iOS asks for only then, so the app opens. Then link it: run `codealong devices code` and have me enter the code in the app under Macs. Then tell me how to let the phone reply and approve from there, that my other Macs link the same way, and when the app needs installing again.',
    ].join('\n\n'),
  );
}

/**
 * Setting this machine up with Codealong Dev, a build of Codealong's source, in place of the
 * installed app: in the source's checkout here (`source`, its worktree `laneId`), or in a chat of
 * its own that clones it into a folder Codealong looks for repos in (`scan`), so it comes onto
 * the board. The repo's own notes say how to build it.
 */
export function devSetupTask(source: string | null, scan: string[], laneId: string | null): AgentTask {
  const where = source
    ? `Codealong’s source is already in ${source}: build from it, bringing it up to date with GitHub first where that loses nothing.`
    : scan.length === 1
      ? `Clone ${CODEALONG_REPO} into ${scan[0]}, the folder Codealong looks for repos in, so it comes onto Codealong’s board.`
      : scan.length > 1
        ? `Clone ${CODEALONG_REPO} into one of the folders Codealong looks for repos in (${scan.join(', ')}), so it comes onto Codealong’s board.`
        : `Clone ${CODEALONG_REPO} into the folder where I keep my code (ask me if you can’t tell), and add that folder to \`scan\` in Codealong’s config.toml so Codealong finds it.`;
  const prompt = [
    'Set this machine up to run Codealong Dev, a development version of Codealong built from its source, in place of the installed Codealong, so I can change Codealong and run the changes.',
    where,
    'Its docs/development.md says what building it needs and how. Install what this machine is missing for it, and tell me when something needs me, such as a password or a dialog to click. Then run scripts/relaunch.sh in the checkout: it builds Codealong Dev, puts it in Applications and opens it, and the installed Codealong quits as it opens. The two share settings and todos.',
    'Once Codealong Dev is running, tell me so, and that opening Codealong from Applications goes back to the installed one. From then on, Improve Codealong in Codealong Dev starts its sessions in this checkout, and scripts/relaunch.sh runs what they change.',
  ].join('\n\n');
  return { prompt, laneId };
}

/** `n` commits, in words. */
function commits(n: number): string {
  return n === 1 ? '1 commit' : `${n} commits`;
}

/** Updating Codealong Dev: what origin/main has that the running build hasn't, pulled into the
 *  source's checkout it was looked at in (the session starts in its worktree, or in a chat of its
 *  own for a source in none of your projects) and built by relaunch.sh, which reopens the app. A
 *  checkout holding work of its own stops it rather than lose it. */
export function devUpdateTask(b: DevBehind): AgentTask {
  const short = b.built.slice(0, 8);
  const prompt = [
    `Update Codealong Dev to the latest origin/main and relaunch it. It was built from ${short}, and origin/main has ${commits(b.commits)} since. Codealong’s source is in ${b.source}.`,
    'Fetch, then bring the checkout to origin/main only where that loses nothing: if it has uncommitted changes, or commits origin/main doesn’t have, stop and tell me what they are rather than stash, reset or merge them. Then run scripts/relaunch.sh in it: it builds Codealong Dev, waits its turn behind other builds, and quits and reopens the running one once it’s built.',
    `Tell me what its last line said, and in a few lines what’s new since ${short} (\`git log --oneline ${short}..origin/main\`).`,
  ].join('\n\n');
  return { prompt, laneId: b.laneId };
}

/** An account whose command line isn't on this machine (the engine's `Adapter::missing_cli`),
 *  as Settings › Accounts has it. */
export interface MissingCli {
  name: string;
  harness: Harness;
  /** The command that runs it, as Codealong does. */
  command: string;
  /** The program that isn't here. */
  cli: string;
}

/** Installing an account's command line, which Codealong starts its sessions with and which
 *  isn't on this machine. The agent runs on another account, or on this one where it starts
 *  without it (the Claude app, Codealong's own sessions); how to install it is the harness's
 *  own docs'. */
export function installCliTask(m: MissingCli): AgentTask {
  const word = HARNESS_WORD[m.harness];
  return chat(
    [
      `Install ${m.cli}, ${word}’s command line, on this machine. Codealong runs my ${word} account ${m.name} with \`${m.command}\`, and ${m.cli} isn’t found here: not on the PATH my shell gives a terminal, nor where its installer puts it.`,
      `Install it the way ${word}’s own docs say, asking me before anything that wants my password. Then check that a new terminal finds it (\`command -v ${m.cli}\`) and that it runs. If it needs me to sign in, tell me: Codealong’s Settings › Accounts has Sign in for ${m.name}.`,
      `Once it runs, tell me so: Codealong can start ${m.name}’s sessions with it again, and Settings › Accounts stops saying it’s missing the next time it reads the accounts.`,
    ].join('\n\n'),
  );
}

/** Making this machine's projects match another's (`machine`, by its name), so the same work is
 *  one project on both: what that machine names its projects is in the prompt, as its worktrees
 *  came (peers.rs `projects_there`), and the agent compares it with config.toml and the checkouts
 *  here, says what it would change, and changes only what you pick. */
export function syncProjectsTask(machine: string, projects: ProjectThere[]): AgentTask {
  const listed = projects.map((p) => `- ${p.name}: ${p.repos.length > 0 ? p.repos.join(', ') : 'no repos yet'}`).join('\n');
  return chat(
    [
      `Make this machine’s Codealong projects match my ${machine}’s, so the same work is in the same project on both machines, under the same name.`,
      `On ${machine}, Codealong has these projects, each repo as config.toml names one, or its folder on ${machine} when it has no GitHub remote:\n${listed}`,
      'Codealong’s projects are the `[[projects]]` tables in its config.toml (`codealong status` names the file): each has a `name`, `repos` (`github.com/owner/name`, or `github.com/owner/*` for every repo of an owner) and `folders`. `codealong worktrees --all` lists every checkout Codealong found on this machine and the project each is in, `Elsewhere` when it’s in none.',
      `Compare the two, and before changing anything tell me what you’d change here: a project ${machine} has that this machine has under another name, or not at all; repos this machine has in another project, or in none, that ${machine} has in one; and repos ${machine} has that aren’t cloned here, with where you’d clone each (one of the folders in \`scan\`, so Codealong finds it). Keep what’s only on this machine, and don’t rename or remove a project without asking.`,
      `Once I say which to do, change only the \`[[projects]]\` tables, leaving the rest of config.toml as it is, run \`codealong config check\`, clone what I picked, and tell me how the projects stand. If something here should be on ${machine} too, say so: Sync projects there does the same the other way.`,
    ].join('\n\n'),
  );
}
