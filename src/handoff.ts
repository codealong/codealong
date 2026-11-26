// Pure rules for moving or handing off a session to another account or provider: which accounts
// are offered (same-harness native moves first, then other providers), whether a choice uses the
// lossless native fast path (Claude Code → Claude Code) or the generic handoff engine, and the
// plain-English copy the menu and confirmation dialog show.

import type { MoveOptions } from './api';
import { HARNESS_WORD, MOVES_ACCOUNTS } from './harness';
import type { AccountInfo, Harness, Lane, Session, StartPlace } from './types';

/** What a move or edit-from-here resolves to for a window that follows it (`SessionCan.moving`):
 *  - `{ kind: 'key', key }`: the copy's key is known immediately;
 *  - `{ kind: 'origin' }`: the move succeeded asynchronously (e.g. a cross-provider terminal/app
 *    launch) and the copy will appear on the board with `origin.key === fromKey`;
 *  - `null`: it moved in-place as itself, or the move failed. */
export type MoveFollow = { kind: 'key'; key: string } | { kind: 'origin' } | null;

/** Keys of sessions already on `lanes` whose origin is a fork of `fromKey`, snapshotted before a
 *  move so async follow matching ignores older forks or kept handoffs. */
export function forkOriginKeys(lanes: readonly Lane[], fromKey: string): ReadonlySet<string> {
  const known = new Set<string>();
  for (const lane of lanes) {
    if (lane.device) continue;
    for (const session of lane.sessions) {
      if (session.origin?.key === fromKey && session.origin.how === 'fork') known.add(session.key);
    }
  }
  return known;
}

/** Finds the session `fromKey` moved to on `lanes`: by exact `targetKey` when known, else the
 *  newest non-archived fork whose `origin.key === fromKey` that isn't in `knownKeys`. */
export function findMovedCopy(
  lanes: readonly Lane[],
  fromKey: string,
  targetKey: string | null,
  knownKeys?: ReadonlySet<string>,
): { lane: Lane; session: Session } | null {
  if (targetKey) {
    for (const lane of lanes) {
      const session = lane.sessions.find((s) => s.key === targetKey && !s.visitor);
      if (session) return { lane, session };
    }
    return null;
  }
  let best: { lane: Lane; session: Session; at: number } | null = null;
  for (const lane of lanes) {
    if (lane.device) continue;
    for (const session of lane.sessions) {
      if (session.key === fromKey || session.visitor || session.archived) continue;
      if (knownKeys?.has(session.key)) continue;
      if (session.origin?.key === fromKey && session.origin.how === 'fork') {
        const at = session.startedAt ?? session.lastActivity;
        if (!best || at > best.at) best = { lane, session, at };
      }
    }
  }
  return best ? { lane: best.lane, session: best.session } : null;
}

/** One account a session can move or hand off to. */
export interface MoveTarget {
  account: string;
  harness: Harness;
  /** True when moving with default settings uses the harness's lossless native transcript copy. */
  native: boolean;
}

/** The signed-in, enabled accounts `s` can move or hand off to: same-harness accounts first (so
 *  lossless moves lead), then other providers in config order. */
export function moveTargetsFor(s: Pick<Session, 'account' | 'harness' | 'resumeCommand'>, accounts: readonly AccountInfo[]): MoveTarget[] {
  const nativeSame = MOVES_ACCOUNTS.includes(s.harness) && !!s.resumeCommand;
  const eligible = accounts.filter((a) => a.name !== s.account && a.signedIn && !a.disabled);
  const same = eligible.filter((a) => a.harness === s.harness).map((a): MoveTarget => ({ account: a.name, harness: a.harness, native: nativeSame }));
  const other = eligible.filter((a) => a.harness !== s.harness).map((a): MoveTarget => ({ account: a.name, harness: a.harness, native: false }));
  return [...same, ...other];
}

/** Whether a move to `target` with `opts` takes the lossless native fast path (handoff.rs
 *  `uses_native_move`) rather than building a generic handoff packet. */
export function isNativeMove(target: MoveTarget, opts?: MoveOptions): boolean {
  if (!target.native || opts?.keep) return false;
  if (opts?.model || opts?.effort || opts?.place) return false;
  if (opts?.prompt?.trim()) return false;
  return true;
}

/** Label for one item in the `Move to` submenu: includes the provider name when cross-provider,
 *  and notes `(has room)` on the suggested account. */
export function moveSubmenuLabel(target: MoveTarget, sourceHarness: Harness, suggested: string | null): string {
  const cross = target.harness !== sourceHarness;
  const base = cross ? `${target.account} · ${HARNESS_WORD[target.harness]}` : target.account;
  return target.account === suggested ? `${base} (has room)` : base;
}

/** Where a generic handoff starts, as a phrase (`in Codealong`, `in a new Ghostty tab`…). */
export function handoffPlaceWords(place: StartPlace | null | undefined, terminal: string): string {
  switch (place) {
    case 'codealong':
      return 'in Codealong';
    case 'background':
      return 'in the background';
    case 'claude-app':
      return 'in the Claude app';
    case 'codex-app':
      return 'in the Codex app';
    case 'terminal':
    default:
      return `in a new ${terminal} tab`;
  }
}

/** Dialog title for `MoveDialog`. */
export function moveDialogTitle(target: MoveTarget | undefined, sourceHarness: Harness, keep: boolean): string {
  if (keep) return 'Hand off to another account';
  if (target && target.harness !== sourceHarness) return `Move to ${HARNESS_WORD[target.harness]}`;
  return 'Move to another account';
}

/** Concise explanation of how moving across accounts works, for the confirmation dialog's lede. */
export function moveDialogWords(
  s: Pick<Session, 'title' | 'status' | 'attachCommand' | 'harness'>,
  target: MoveTarget,
  terminal: string,
  goesOn: boolean,
  opts?: MoveOptions,
  modelLabel?: string | null,
): string {
  const keep = !!opts?.keep;
  const first = !keep && s.status === 'working' ? ' Its turn is stopped first.' : '';
  const then = goesOn ? ', told to carry on where it stopped' : '';
  if (isNativeMove(target, opts)) {
    const where = s.attachCommand != null ? 'as a background job' : `in a new ${terminal} tab`;
    return `A copy of “${s.title}” goes on under ${target.account} ${where}${then} with its full conversation carried over, and this one is closed and archived.${first}`;
  }
  const provider = HARNESS_WORD[target.harness];
  const onModel = modelLabel ? ` on ${modelLabel}` : '';
  const fate = keep ? '; this session stays open.' : ', and this one is closed and archived.';
  return `Starts a new ${provider} session under ${target.account}${onModel} with a handoff brief and a link to the full transcript on disk${then}${fate}${first}`;
}
