// Account-level usage limit for a session whose own last turn didn't hit a limit: warns before you
// send a new turn when its account is already at a limit this session's model spends. Mirrors
// src-tauri/src/exhausted.rs (`check_with_model`) for active refusals on other sessions of the
// same account, and also checks Usage meters at 100% when extra usage isn't usable to carry turns
// past the plan's limits (`Usage.extraUsable`).

import { resetsPhrase } from './format';
import { limitName } from './lanes';
import { untilWords } from './snooze';
import { metersFor } from './start';
import type { Lane, Meter, Session, Usage } from './types';

/** How old a refusal may be and still count, matching start.rs / exhausted.rs `REFUSED_FOR`. */
const REFUSED_FOR = 31 * 24 * 3_600_000;
/** Minimum time until reset before offering Snooze until it lifts, matching snooze.ts `WORTH`. */
const SNOOZE_WORTH = 5 * 60_000;
/** Below this percent on a fresher usage read, an earlier refusal for that meter has cleared. */
const CLEARED_UNDER = 90;
/** Families with their own weekly limits on Claude plans, matching start.ts / start.rs `FAMILIES`. */
const FAMILIES = ['fable', 'opus', 'sonnet', 'haiku'];

/** An active account-level usage limit that will block a new turn on this session. */
export interface AccountLimit {
  account: string;
  /** Meter key (`session`, `week`, `model:Opus`), or null for an unnamed usage limit. */
  meter: string | null;
  /** When the limit lifts (epoch ms), or null when unknown. */
  resetsAt: number | null;
  /** Extra usage is off or refused too (e.g. out of credits / spend cap reached). */
  spendCapped: boolean;
  /** True when backed by an actual refusal on another session; false when read from a 100% meter. */
  refused: boolean;
}

function limitModel(meter: string | null | undefined): string | null {
  if (!meter?.startsWith('model:')) return null;
  return meter.slice('model:'.length) || null;
}

function familiesOf(limit: string): string[] {
  return limit
    .split(' & ')
    .map((f) => f.trim().toLowerCase())
    .filter(Boolean);
}

/** Whether `model` spends a limit scoped to `limit` (start.rs `spends`). */
function spendsLimit(model: string | null | undefined, limit: string | null, meters: readonly Meter[]): boolean {
  if (!limit) return true;
  const m = model?.toLowerCase() ?? '';
  if (familiesOf(limit).some((f) => m.includes(f))) return true;
  const named = FAMILIES.some((f) => m.includes(f)) || meters.some((x) => x.model !== null && familiesOf(x.model).some((f) => m.includes(f)));
  return !named;
}

/**
 * Whether `session`'s account is currently at a usage limit that a new turn on `session` would
 * hit, scoped to `deviceId` (null/undefined for this Mac). Returns null when `session` already
 * stopped at its own limit (its stopped-at-limit banner handles it), is working, or is archived.
 */
export function accountLimitFor(
  session: Pick<Session, 'account' | 'model' | 'status' | 'archived' | 'limit'>,
  lanes: readonly Lane[],
  usage: readonly Usage[],
  now: number,
  deviceShort?: string | null,
): AccountLimit | null {
  if (session.limit || session.status === 'working' || session.archived) return null;

  const targetDevice = deviceShort ?? null;
  const acctUsage = usage.find((u) => u.account === session.account);
  const meters = acctUsage?.meters ?? [];
  const sameAccount = lanes
    .filter((l) => (l.device?.short ?? null) === targetDevice)
    .flatMap((l) => l.sessions)
    .filter((s) => s.account === session.account);

  // 1. Active refusal on another session of the same account (exhausted.rs `check_with_model` +
  //    board/overlays.rs `room_back` fresher-usage clearing).
  let bestRefusal: { at: number; out: AccountLimit } | null = null;
  for (const other of sameAccount) {
    const lim = other.limit;
    if (!lim || lim.resetsAt == null || lim.resetsAt <= now) continue;
    if (now - lim.at >= REFUSED_FOR) continue;
    const lm = limitModel(lim.meter);
    if (!spendsLimit(session.model, lm, meters)) continue;

    // A later non-working turn without a limit that spends the same limit proves the refusal is over.
    const clearedByTurn = sameAccount.some(
      (o) => !o.limit && o.status !== 'working' && o.lastActivity > lim.at && (!lm || (o.model != null && spendsLimit(o.model, lm, meters))),
    );
    if (clearedByTurn) continue;

    // A fresher usage read since the refusal clears it when extra usage came back on (after a spend
    // cap) or when the named meter dropped back below near-full (e.g. a reset credit was spent).
    if (acctUsage && acctUsage.asOf != null && acctUsage.asOf > lim.at && !acctUsage.needsSignIn) {
      if (lim.spendCapped && acctUsage.extraUsable === true) continue;
      if (lim.meter != null) {
        const read = acctUsage.meters.find((m) => m.key === lim.meter);
        if (read && read.percent < CLEARED_UNDER) continue;
      }
    }

    const out: AccountLimit = {
      account: session.account,
      meter: lim.meter,
      resetsAt: lim.resetsAt,
      spendCapped: !!lim.spendCapped,
      refused: true,
    };
    const replace =
      !bestRefusal ||
      (out.resetsAt ?? 0) > (bestRefusal.out.resetsAt ?? 0) ||
      (out.resetsAt === bestRefusal.out.resetsAt && lim.at > bestRefusal.at);
    if (replace) bestRefusal = { at: lim.at, out };
  }
  if (bestRefusal) return bestRefusal.out;

  // 2. Plan meter at 100% for a limit this session's model spends, unless extra usage can carry
  //    turns past the plan's limits (`Usage.extraUsable === true`).
  if (!acctUsage || acctUsage.needsSignIn || acctUsage.loading || acctUsage.noLimits) return null;
  if (acctUsage.extraUsable === true) return null;

  const full = metersFor(acctUsage, session.model ?? null).filter((m) => m.percent >= 100 && (m.resetsAt == null || m.resetsAt > now));
  if (full.length === 0) return null;

  // Pick the binding limit: prefer one with a future reset time, and the latest reset among those.
  let bestMeter = full[0];
  for (const m of full.slice(1)) {
    if (m.resetsAt != null && (bestMeter.resetsAt == null || m.resetsAt > bestMeter.resetsAt)) {
      bestMeter = m;
    }
  }
  return {
    account: session.account,
    meter: bestMeter.model ? `model:${bestMeter.model}` : bestMeter.key,
    resetsAt: bestMeter.resetsAt,
    spendCapped: acctUsage.extraUsable === false,
    refused: false,
  };
}

/** Short banner phrase for an account-level usage limit: `claude-work at weekly limit · lifts in 9h`. */
export function accountLimitPhrase(limit: AccountLimit, now: number): string {
  const base = `${limit.account} at ${limitName(limit.meter)}`;
  if (limit.resetsAt == null || limit.resetsAt <= now) return base;
  return `${base} · lifts ${resetsPhrase(limit.resetsAt, now)}`;
}

/** Tooltip explanation for an account-level usage limit. */
export function accountLimitWords(limit: AccountLimit, now: number, moves = true): string {
  const spent = limit.spendCapped ? ', with no extra usage to carry it' : '';
  const when = limit.resetsAt != null && limit.resetsAt > now ? ` It lifts ${resetsPhrase(limit.resetsAt, now)}.` : '';
  const next = moves ? ' Starting a new turn will hit it unless you move to another account.' : ' Starting a new turn will hit it.';
  return `${limit.account} is at its ${limitName(limit.meter)}${spent}.${when}${next}`;
}

/** Whether the banner should offer `Snooze until it lifts` for this account limit. */
export function accountLimitSnoozeable(limit: AccountLimit, now: number): boolean {
  return limit.resetsAt != null && limit.resetsAt - now >= SNOOZE_WORTH;
}

/** Tooltip for the `Snooze until it lifts` button on the account-limit banner. */
export function accountLimitSnoozeTitle(resetsAt: number, now: number): string {
  return `It leaves what waits on you until ${untilWords(resetsAt, now)}, when the limit lifts`;
}
