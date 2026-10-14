// The mode a session runs in (`Session.mode`), as its harness names it: Claude Code's Manual,
// Plan or Bypass, Codex's Approve for me or Full Access, OpenCode's agent, Cursor's auto-run.
// It's said under the reply box beside its account, lowercase, where Claude Code says its own
// mode under its box (SessionReply.tsx), and in the header's Details for a session with no box;
// what the mode means is its tooltip. Codealong only says it: the mode is changed where the
// session runs.

import type { ModeKind, SessionMode } from '../types';
import { cx, Icon } from './primitives';

/** An icon for how much it does without asking. */
const MODE_ICON: Record<ModeKind, string> = {
  plan: 'map',
  'read-only': 'eye',
  asks: 'user-check',
  edits: 'pencil',
  auto: 'robot',
  skips: 'bolt',
  other: 'shield',
};

export function ModeLabel({ mode, small = false }: { mode: SessionMode; small?: boolean }) {
  return (
    <span className={cx('ca-mode', small && 'is-small')} title={`Its mode: ${mode.label}. ${mode.detail}`}>
      <Icon name={MODE_ICON[mode.kind] ?? MODE_ICON.other} size={small ? 11 : 12} />
      <span className="ca-mode-name">{mode.label}</span>
    </span>
  );
}
