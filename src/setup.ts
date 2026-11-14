// The setup wizard's choices (ui/SetupDialog.tsx), as pure functions: what it starts ticked, what
// each row says, and what config.toml gets once you're done. The engine's side, what it offers
// and when it shows by itself, is src-tauri/src/setup.rs.

import type { AccountCfg, Config, EmblemsCfg, Profile, SetupAccount, SetupFolder, SetupView } from './types';

export type SetupStep = 'accounts' | 'folders' | 'apps' | 'emblems' | 'ready';
export const SETUP_STEPS: SetupStep[] = ['accounts', 'folders', 'apps', 'emblems', 'ready'];

/** What's picked so far. Accounts by their folder, folders as config.toml writes them. */
export interface SetupPicks {
  accounts: string[];
  folders: string[];
  terminal: string;
  editor: string;
  /** Put Codealong in the accounts kept (the plugin, or the skill) and the command on PATH. */
  agents: boolean;
  atLogin: boolean;
  /** Session emblems, as Settings › General sets them. */
  emblems: EmblemsCfg;
}

/** An account starts ticked when config.toml lists it, or it's in use or signed in: the ones a
 *  first run lists. */
export function accountPicked(a: SetupAccount): boolean {
  return a.listed || a.state !== 'idle';
}

/** The folders start ticked when config.toml searches them and they're there, or your agents
 *  worked in them; with none of those, every one with repos in it. */
export function foldersPicked(folders: SetupFolder[]): string[] {
  const kept = folders.filter((f) => (f.scanned && f.exists) || f.worked > 0);
  return (kept.length > 0 ? kept : folders.filter((f) => f.exists && (f.repos ?? 0) > 0)).map((f) => f.path);
}

/** One of `profiles`: `id` when it's among them, else the first, else `id` as it is. */
function installedOr(id: string, profiles: Profile[]): string {
  return profiles.some((p) => p.id === id) || profiles.length === 0 ? id : profiles[0].id;
}

/** What the wizard starts with. */
export function firstPicks(view: SetupView, cfg: Config, terminals: Profile[], editors: Profile[]): SetupPicks {
  return {
    accounts: view.accounts.filter(accountPicked).map((a) => a.home),
    folders: foldersPicked(view.folders),
    terminal: installedOr(cfg.apps.terminal, terminals),
    editor: installedOr(cfg.apps.editor, editors),
    agents: true,
    atLogin: false,
    // As config.toml has them, which a first run's has off (config.rs `EmblemsCfg`): both kinds
    // spend, Jev's requests or an account's usage, so they're on only once you pick one.
    emblems: cfg.emblems ?? { kind: 'off', account: '', prompt: '' },
  };
}

/** The accounts you kept that can draw session emblems (setup.rs `Account::draws`), by the name
 *  config.toml gets for each: the ones the wizard's Drawn is offered for. */
export function emblemDrawers(view: SetupView, picked: string[]): string[] {
  return view.accounts.filter((a) => a.draws && picked.includes(a.home)).map((a) => a.name);
}

/** The emblems config.toml gets. Drawn ones go to an account that can draw: the one picked while
 *  it's among `drawers`, else the first of them, which the engine's own pick of the first whose
 *  harness draws (config.rs `drawing_account`) wouldn't be when that one is signed out. */
export function emblemsAfter(emblems: EmblemsCfg, drawers: string[]): EmblemsCfg {
  if (emblems.kind !== 'drawn' || drawers.length === 0 || drawers.includes(emblems.account)) return emblems;
  return { ...emblems, account: drawers[0] };
}

/** The accounts config.toml gets: the ones it lists that you kept, as it has them, then the
 *  others you picked, in the wizard's order. */
export function accountsAfter(view: SetupView, listed: AccountCfg[], picked: string[]): AccountCfg[] {
  const kept = listed.filter((a) => picked.includes(a.home));
  const added = view.accounts
    .filter((a) => !a.listed && picked.includes(a.home))
    .map((a): AccountCfg => ({ name: a.name, harness: a.harness, home: a.home, command: a.command, start: null, acp: null, monthly: null, per_token: false, disabled: false, usage_cap: null }));
  return [...kept, ...added];
}

/** config.toml's settings with what was picked: the accounts (`accountsAfter`), the folders in
 *  the wizard's order, the apps and the emblems (`emblemsAfter`, drawn by one of `drawers`). One
 *  save writes them all: saving the settings writes the accounts too (config.rs `with_config`), so
 *  a save of the accounts alone would be undone. */
export function configAfter(cfg: Config, picks: SetupPicks, accounts: AccountCfg[], drawers: string[]): Config {
  return { ...cfg, accounts, scan: picks.folders, apps: { ...cfg.apps, terminal: picks.terminal, editor: picks.editor }, emblems: emblemsAfter(picks.emblems, drawers) };
}

/** What a folder's row says of it. */
export function folderNote(f: SetupFolder): string {
  if (!f.exists) return 'Not there yet';
  if (f.repos === null) return 'Not looked in yet';
  if (f.repos === 0) return 'No repos in it yet';
  const repos = f.repos === 1 ? '1 repo' : `${f.repos} repos`;
  if (f.worked === 0) return repos;
  const which = f.worked < f.repos ? String(f.worked) : f.repos === 1 ? 'it' : f.repos === 2 ? 'both' : 'all of them';
  return `${repos}; your agents worked in ${which} lately`;
}

/** A folder as config.toml writes it, from what was typed: `~` for your home folder kept, a
 *  trailing slash dropped. Null when there's nothing. */
export function typedFolder(text: string): string | null {
  const t = text.trim().replace(/\/+$/, '');
  return t === '' ? null : t;
}
