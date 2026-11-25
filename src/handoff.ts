// Pure rules for moving or handing off a session to another account or provider: which accounts
// are offered (same-harness native moves first, then other providers), whether a choice uses the
// lossless native fast path (Claude Code → Claude Code) or the generic handoff engine, and the
// plain-English copy the menu and dialog show.

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

/** What the move or handoff does, for the dialog's lede and tooltips. */
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
