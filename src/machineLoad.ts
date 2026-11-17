// How busy a machine is, as it reads itself (the engine's load.rs): its words for the windows,
// and the other machine the quick panel offers when the one a start goes to is strained. A
// strained machine starts a session as slowly as it does everything else, so a calm one that has
// the same repo is offered, on one key, never switched to by itself: the worktree you picked, its
// changes and its terminal are where they are.

import { shortOf } from './devices';
import { repoKey } from './quick';
import type { DeviceView, Lane, Load, LoadLevel } from './types';

/** Takes the machine the quick panel offers (`calmerMachine`), with the start's repo there. */
export const CALMER_CHORD = 'Shift+Cmd+D';

export const LOAD_WORDS: Record<LoadLevel, string> = { calm: 'Calm', busy: 'Busy', strained: 'Strained' };

/** A load's numbers, for a tooltip: "load 27 per CPU of 10, 40% of memory free". */
export function loadNumbers(l: Load): string {
  const parts: string[] = [];
  if (l.perCpu !== null) parts.push(`load ${l.perCpu} per CPU of ${l.cpus}`);
  if (l.memoryFree !== null) parts.push(`${l.memoryFree}% of memory free`);
  if (l.memoryShort) parts.push('memory getting short');
  return parts.join(', ');
}

/** A machine's load in a sentence: "Mac mini is strained: load 27 per CPU of 10, 40% of memory
 *  free." */
export function loadSentence(l: Load, name: string): string {
  const numbers = loadNumbers(l);
  return `${name} is ${l.level}${numbers ? `: ${numbers}` : ''}.`;
}

/** A machine the quick panel offers to start on instead. */
export interface Calmer {
  /** Its short id; null for this machine. */
  on: string | null;
  name: string;
  load: Load;
  /** Where the start goes there: a worktree of the same repo, or its chats for a chat. */
  lane: Lane;
}

/** A machine a start could go to: this one (`on` null) or a linked one. */
interface Candidate {
  on: string | null;
  name: string;
  load: Load | null | undefined;
  lanes: readonly Lane[];
}

/** The GitHub repos a lane's checkout is of: its slug, and the one GitHub has now when it was
 *  renamed, lowercased. */
function slugsOf(l: Pick<Lane, 'repoSlug' | 'repoSlugNow'>): string[] {
  return [l.repoSlug, l.repoSlugNow].filter((x): x is string => !!x).map((x) => x.toLowerCase());
}

/** Whether `l` is a checkout of the same repo as `of`: the same GitHub repo when `of` has one,
 *  whatever each machine calls it, else a folder of the same name in the same project. */
function sameRepo(l: Lane, of: Pick<Lane, 'project' | 'repo' | 'repoSlug' | 'repoSlugNow'>): boolean {
  const slugs = slugsOf(of);
  if (slugs.length > 0) return slugsOf(l).some((x) => slugs.includes(x));
  return slugsOf(l).length === 0 && repoKey(l) === repoKey(of);
}

/**
 * Another machine to start on while the one a start goes to (`to.on`, null for this one) is
 * strained: one that's calm, has the start's repo (its main checkout there when it has one) or
 * chats for a chat, and can be started on from here (connected, taking remote control). The least
 * loaded first. None while the start's machine isn't strained, or has no repo picked, or no other
 * machine is calm: a busy one would only trade one wait for another.
 */
export function calmerMachine(
  to: { on: string | null; lane: Pick<Lane, 'project' | 'repo' | 'repoSlug' | 'repoSlugNow' | 'startsChats'> | null },
  here: { name: string; load: Load | null | undefined; lanes: readonly Lane[] },
  macs: readonly DeviceView[],
  macLanes: readonly Lane[],
): Calmer | null {
  const lane = to.lane;
  if (!lane) return null;
  const candidates: Candidate[] = [
    { on: null, name: here.name, load: here.load, lanes: here.lanes },
    ...macs.filter((d) => d.online && d.remoteControl).map((d) => ({ on: d.short, name: d.name, load: d.load, lanes: macLanes.filter((l) => shortOf(l.id) === d.short) })),
  ];
  const from = to.on === null ? candidates[0] : (candidates.find((c) => c.on === to.on) ?? { load: macs.find((d) => d.short === to.on)?.load });
  if (from.load?.level !== 'strained') return null;
  const found: Calmer[] = [];
  for (const c of candidates) {
    if (c.on === to.on || c.load?.level !== 'calm') continue;
    const repo = lane.startsChats ? [] : c.lanes.filter((l) => !l.startsChats && sameRepo(l, lane));
    const there = lane.startsChats ? c.lanes.find((l) => l.startsChats) : (repo.find((l) => l.origin === 'main') ?? repo[0]);
    if (there) found.push({ on: c.on, name: c.name, load: c.load, lane: there });
  }
  // Windows says nothing of its CPUs: taken as neither the calmest nor the busiest of the calm.
  const perCpu = (c: Calmer) => c.load.perCpu ?? 0.5;
  found.sort((a, b) => perCpu(a) - perCpu(b));
  return found[0] ?? null;
}
