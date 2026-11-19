// Skills a session can be asked to run by name (skills.rs), and the `codealong` command, the
// skill and the plugin as each account has them installed (install.rs).

import type { Harness } from './session';

/** The `codealong` command link and the agent skill per account (install.rs). */
export interface AgentsStatus {
  command: { link: string | null; target: string; onPath: boolean };
  /** Whether the skill folder ships with this build at all. */
  skillFound: boolean;
  /** The Claude Code plugin's version in this build, when it ships with it. */
  pluginVersion: string | null;
  accounts: AccountSkill[];
  /** One line per failure of the last install; empty when only read. */
  errors: string[];
}

export interface AccountSkill {
  name: string;
  harness: Harness;
  /** The skill's folder for this account; null when the harness has none Codealong knows. */
  path: string | null;
  installed: boolean;
  current: boolean;
  /** Someone changed this copy, so updates leave it alone until it's replaced from Settings. */
  edited: boolean;
  /** For a Claude Code account, the plugin, which is what Codealong installs there; the skill
   *  fields then say whether a copy Codealong made before is still in its skills folder. */
  plugin: PluginState | null;
}

/** The Codealong plugin in a Claude Code account. */
export interface PluginState {
  /** The version installed; null when it isn't. */
  version: string | null;
  /** Installed but not turned on in Claude Code: its hooks and skill don't run. */
  disabled: boolean;
  /** Where its marketplace is when it isn't Codealong's own (a GitHub repo, say). */
  from: string | null;
}

/** A skill a session can be asked to run by name (skills.rs). */
export interface Skill {
  /** As it's typed after the slash. */
  name: string;
  description: string;
  /** What may follow its name. */
  hint?: string;
  /** Meant to be run by name; false for one its file keeps off the menu. */
  typed: boolean;
  /** The agent may load it by itself. */
  byModel: boolean;
  /** What goes in a prompt in place of `/name`, for each harness that reads it and can be told
   *  to run it by name. A harness that isn't here isn't offered the skill. */
  written: Partial<Record<Harness, string>>;
  /** How often it's been run, and when last, where a harness keeps count. */
  uses: number;
  lastUsed: number;
}

/** Each folder's skills (its own and those of the folders above it in its worktree), and each
 *  account's own, plugins' included. */
export interface SkillsListed {
  folders: { folder: string; skills: Skill[] }[];
  accounts: { account: string; skills: Skill[] }[];
}
