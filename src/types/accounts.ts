// An account and what's read of it: its plan usage and limits (src-tauri/peer/src/model.rs
// `Usage`), limit resets (resets.rs), training (training.rs), sign-in (signin.rs) and MCP servers
// (mcp.rs); and an account as Settings and the setup wizard list it (src-tauri/src/model.rs
// `AccountInfo`, detect.rs, setup.rs).

import type { AccountCfg } from './config';
import type { Harness } from './session';

export type Severity = 'normal' | 'warning' | 'critical';

/** One plan limit: the 5-hour session, the week, or a per-model week such as Fable's. */
export interface Meter {
  key: string;
  label: string;
  window: string;
  percent: number;
  resetsAt: number | null;
  severity: Severity;
  model: string | null;
}

export interface Usage {
  account: string;
  harness: Harness;
  plan: string | null;
  meters: Meter[];
  asOf: number | null;
  error: string | null;
  needsSignIn: boolean;
  loading: boolean;
  /** Its harness keeps no limits Codealong can read, and `error` says why. */
  noLimits?: boolean;
  /** Its `preferred` setting: Auto starts it over accounts without one while it has room.
   *  Missing from older Macs. */
  preferred?: boolean;
  /** The usage endpoint turned the last read away, and `error` says when it's tried again. */
  rateLimited?: boolean;
}

/** Where an account stands on its provider training on what it sends (training.rs). */
export interface Training {
  account: string;
  /** `unset`: nothing's chosen yet, and the provider asks the next time it's used. */
  state: 'on' | 'off' | 'unset' | 'unknown';
  /** Why, in words. */
  note: string;
  /** Codealong can turn training off itself. */
  canStop: boolean;
  /** The provider's page where it's changed by hand. */
  url: string | null;
  asOf: number | null;
  /** Why the latest read couldn't tell, when this is an older answer kept. */
  stale?: string | null;
}

/** The limit resets an account holds (resets.rs): each puts its plan's limits back to empty. */
export interface Resets {
  account: string;
  /** How many it holds; `credits` may list fewer. */
  available: number;
  /** The ones listed, the first to expire first. */
  credits: ResetCredit[];
  /** Codealong can use one itself. */
  canUse: boolean;
  /** Why there's nothing to show, in words. */
  note: string | null;
  asOf: number | null;
  /** Why the latest read failed, when this is an older answer kept. */
  stale?: string | null;
}

export interface ResetCredit {
  id: string;
  /** `Full reset`. */
  title: string | null;
  /** Why it was granted, in the provider's words. */
  description: string | null;
  grantedAt: number | null;
  /** When it can no longer be used; null when it doesn't expire. */
  expiresAt: number | null;
}

/** How using a reset went (resets.rs `use_one`). */
export interface ResetUsed {
  account: string;
  /** One was spent and the limits are back to empty. */
  reset: boolean;
  /** What the toast says. */
  message: string;
  /** What the account holds after. */
  after: Resets;
}

/** Who an account is signed in as and on what plan (signin.rs), for a boundary group's members. */
export interface SignIn {
  account: string;
  email: string | null;
  /** The provider's organization or workspace id. */
  organization: string | null;
  organizationName: string | null;
  /** The provider's name for the plan: team, enterprise, max, pro, business… */
  plan: string | null;
  /** The plan is under the provider's commercial terms. */
  commercial: boolean;
  /** The provider said so with the account's own sign-in; false when only its files did. */
  verified: boolean;
  asOf: number | null;
  /** Why it isn't verified, or why an older answer is kept. */
  note?: string | null;
}

/** One MCP server an account's agents can call (mcp.rs). */
export interface McpServer {
  /** As its harness names it: "Gmail", "figma". */
  name: string;
  /** Where it comes from, in words: "claude.ai connector", "figma plugin", "~/.cursor/mcp.json". */
  from: string;
  /** Its tools' names, where the harness lists them without starting it. */
  tools?: string[];
}

/** The MCP servers an account's agents have (mcp.rs): its provider's connectors, its settings'
 *  servers and its plugins'. */
export interface Mcp {
  account: string;
  servers: McpServer[];
  /** Why some or all of them couldn't be read. */
  note?: string | null;
  asOf: number | null;
  /** Why the latest read couldn't tell, when this is an older answer kept. */
  stale?: string | null;
}

export interface AccountInfo extends AccountCfg {
  /** Its harness records the pages its sessions make, so a Chrome profile for them does something. */
  makesPages: boolean;
  /** Its harness can give a new account what you made in this one's folder (its settings,
   *  instructions, skills…), so the New account form offers to copy from it. */
  copiesSettings: boolean;
  folderExists: boolean;
  signedIn: boolean;
  email: string | null;
  /** What to say of its sign-in in place of the email: the providers OpenCode is signed in to,
   *  or that it runs only free models. */
  signInNote: string | null;
  /** Its command line's program when it isn't on this machine (harness.rs `missing_cli`), so it
   *  starts nothing that needs it; missing when it's there. */
  missingCli?: string;
}

/** An account on this Mac that config.toml doesn't list (detect.rs). */
export interface FoundAccount extends AccountCfg {
  /** The file the command comes from, when it's your own wrapper or alias. */
  via: string | null;
  /** active: sessions in the last 30 days; signedIn: signed in but quiet; idle: installed or set
   * up in your shell, neither signed in nor used lately. */
  state: 'active' | 'signedIn' | 'idle';
  signedIn: boolean;
  folderExists: boolean;
  email: string | null;
  signInNote: string | null;
  lastUsed: number | null;
  /** Codealong has never read this folder, and nobody took it out or dismissed it. */
  new: boolean;
}

/** An account the setup wizard offers (setup.rs): one config.toml lists, or another on this Mac. */
export interface SetupAccount extends FoundAccount {
  listed: boolean;
  /** It can draw session emblems: its harness draws, it's signed in, and it isn't turned off. */
  draws: boolean;
}

/** A folder the setup wizard offers to search for repos (setup.rs). */
export interface SetupFolder {
  /** With `~` for your home folder, as config.toml's `scan` writes it. */
  path: string;
  exists: boolean;
  /** The repos in it, three levels deep; null when it couldn't be read in time. */
  repos: number | null;
  /** Of those, the ones your agents worked in lately. */
  worked: number;
  /** config.toml's `scan` lists it. */
  scanned: boolean;
}

/** What the setup wizard offers. */
export interface SetupView {
  accounts: SetupAccount[];
  folders: SetupFolder[];
}
