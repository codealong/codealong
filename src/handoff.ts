// Pure rules for moving or handing off a session to another account or provider: which accounts
// are offered (same-harness native moves first, then other providers), whether a choice uses the
// lossless native fast path (Claude Code → Claude Code) or the generic handoff engine, and the
// plain-English copy the menu and inline reply-box confirmation strip show.

import type { MoveOptions } from './api';
import { HARNESS_WORD, MOVES_ACCOUNTS } from './harness';
import type { AccountInfo, Harness, Session, StartPlace } from './types';

/** One account a session can move or hand off to. */
export interface MoveTarget {
  account: string;
  harness: Harness;
  /** True when moving with default settings uses the harness's lossless native transcript copy. */
  native: boolean;
}

/** A move or handoff staged inside the session's reply box, awaiting confirmation. */
export interface StagedHandoff {
  account: string;
  model: string | null;
  effort: string | null;
  place: StartPlace | null;
  keep: boolean;
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

/** Full plain-English description of what a move or handoff does, for tooltips and status notes. */
export function moveDialogWords(
  s: Pick<Session, 'title' | 'status' | 'attachCommand' | 'harness'>,
  target: MoveTarget,
  terminal: string,
  goesOn: boolean,
  opts?: MoveOptions,
): string {
  const keep = !!opts?.keep;
  const first = !keep && s.status === 'working' ? ' Its turn is stopped first.' : '';
  const then = goesOn ? ', told to carry on where it stopped' : '';
  if (isNativeMove(target, opts)) {
    const where = s.attachCommand != null ? 'as a background job' : `in a new ${terminal} tab`;
    return `A copy of “${s.title}” goes on under ${target.account} ${where}${then}, and this one is closed and archived.${first}`;
  }
  const where = handoffPlaceWords(opts?.place, terminal);
  const provider = HARNESS_WORD[target.harness];
  const directive = opts?.prompt?.trim() ? ' with your next-step note' : then;
  const fate = keep ? '; this session stays open.' : ', and this one is closed and archived.';
  return `A new ${provider} session starts under ${target.account} ${where} with a handoff brief and a link to the full transcript${directive}${fate}${first}`;
}

/** Compact one-line summary for the inline confirmation bar above the conversation input. */
export function handoffSummary(
  s: Pick<Session, 'harness'>,
  target: MoveTarget,
  goesOn: boolean,
  opts?: MoveOptions,
): string {
  const keep = !!opts?.keep;
  const cross = target.harness !== s.harness;
  const who = cross ? `${target.account} (${HARNESS_WORD[target.harness]})` : target.account;
  const carry = !keep && goesOn && !opts?.prompt?.trim() ? ' · carries on' : '';
  if (keep) return `Hand off to ${who} · keeps this open`;
  if (isNativeMove(target, opts)) return `Move to ${who}${carry} · closes this`;
  return `Move to ${who}${carry} · closes this`;
}

/** Placeholder for the reply box while a move or handoff is staged. */
export function handoffPlaceholder(target: MoveTarget, keep: boolean): string {
  return keep ? `Add a note for ${target.account}, or press ⏎ to hand off…` : `Add a note for ${target.account}, or press ⏎ to move…`;
}

/** Primary confirm button text for a staged move or handoff. */
export function handoffActionLabel(account: string, keep: boolean): string {
  return keep ? `Hand off to ${account}` : `Move to ${account}`;
}

/** Builds the `MoveOptions` sent to `move_session` from a staged handoff, returning `undefined`
 *  when the lossless native same-harness fast path applies. */
export function handoffSubmitOptions(
  target: MoveTarget,
  staged: StagedHandoff,
  prompt: string,
  takesModelHere: boolean,
): MoveOptions | undefined {
  const trimmed = prompt.trim();
  const raw: MoveOptions = {
    keep: staged.keep || undefined,
    place: staged.place ?? undefined,
    model: takesModelHere && staged.model ? staged.model : undefined,
    effort: takesModelHere && staged.effort ? staged.effort : undefined,
    prompt: trimmed || undefined,
  };
  return isNativeMove(target, raw) ? undefined : raw;
}
