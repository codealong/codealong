// Your other devices as the windows get them: this one, the linked ones and what they hold, and
// the ones found or asking to link. Mirrors src-tauri/src/peers.rs. `[devices]` is in config.ts,
// and a lane's `DeviceTag` in lane.ts.

import type { Usage } from './accounts';
import type { Load } from './board';
import type { TextFound } from './conversation';
import type { Lane } from './lane';
import type { PrSession } from './pr';
import type { LabelColor } from './todos';

/** Where a machine stands for the automations your machines share, so one of them runs each: a
 *  backup runs one only when no first choice that can is around. */
export type Rank = 'first' | 'backup';

export type DeviceVia = 'tailscale' | 'iroh' | 'test';

/** Your other devices, as the engine hands them to the windows (`devices` event). */
export interface DevicesView {
  enabled: boolean;
  me: DevicesMe;
  /** Linked devices with boards of their own (Macs), in the order they were linked. */
  devices: DeviceView[];
  /** Linked phones and tablets: they only watch, so they have no worktrees and are in no view. */
  phones: DeviceView[];
  /** Your devices on your tailnet running Codealong that aren't linked yet. */
  found: FoundDevice[];
  /** Devices asking to link, waiting for your answer here. */
  asks: DeviceAsk[];
  /** Every linked device's worktrees; each lane id and session key starts `@<short>:`. */
  lanes: Lane[];
  /** The sessions on your other Macs that worked on each PR they list, by the PR's key in
   *  lowercase, each key and lane id starting `@<short>:` (peerfeed.rs `pr_sessions_for_windows`). */
  prSessions: Record<string, PrSession[]>;
}

export interface DevicesMe {
  key: string;
  short: string;
  name: string;
  tailscale: 'off' | 'missing' | 'stopped' | 'running';
  tailnet: string | null;
  address: string | null;
  /** "off", "starting", "online", "offline", or why it didn't start. */
  iroh: string;
  remoteControl: boolean;
  error: string | null;
}

export interface DeviceView {
  key: string;
  short: string;
  name: string;
  /** Its badge's color: the one you gave it, or the one its key picks. */
  color: LabelColor;
  online: boolean;
  via: DeviceVia | null;
  since: number | null;
  lastSeen: number | null;
  remoteControl: boolean;
  app: string;
  /** It has this device linked too. */
  linkedBack: boolean;
  loading: boolean;
  usage: Usage[];
  kind: 'mac' | 'phone' | 'tablet';
  /** Where it stands for the automations your machines share; none from one that shares none. */
  automations?: Rank | null;
  /** How busy it is while it's connected (load.rs there); null from a phone, and from a machine
   *  before 2026-11-17. */
  load?: Load | null;
  /** Its projects as it names them, from its worktrees as they came (peers.rs `projects_there`). */
  projects?: ProjectThere[];
}

/** A project as another machine names it, with its repos as config.toml names them
 *  (`github.com/owner/name`), or a checkout's folder there when it has no GitHub remote. */
export interface ProjectThere {
  name: string;
  repos: string[];
}

export interface FoundDevice {
  key: string;
  short: string;
  name: string;
  /** The six digits both devices show while linking. */
  code: string;
  /** You asked it to link; it's waiting for an answer there. */
  asking: boolean;
}

export interface DeviceAsk {
  key: string;
  short: string;
  name: string;
  code: string;
}

/** A linked device's answer to a search of what was said. */
export interface DeviceFound {
  device: string;
  name: string;
  found: TextFound | null;
  error: string | null;
}
