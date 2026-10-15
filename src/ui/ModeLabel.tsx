// The mode a session runs in (`Session.mode`), as its harness names it: Claude Code's Manual,
// Plan or Bypass, Codex's Approve for me or Full Access, OpenCode's agent, Cursor's auto-run.
// It's said in the reply box's foot beside its account, lowercase, as Claude Code says its own
// mode by its box (SessionReply.tsx), and in the header's Details for a session with no box;
// what the mode means is its tooltip. A session that runs elsewhere has its mode changed there,
// so Codealong only says it. One Codealong runs is switched here (`ModeMenu`): its agent offers
// the modes (`Session.modes`), and the chip opens them above the box, as Shift+Tab in the box
// steps to the next, as it does in Claude Code's terminal.

import type { ModeKind, SessionMode } from '../types';
import { BoxMenu } from './ModelMenu';
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

const iconOf = (mode: SessionMode) => MODE_ICON[mode.kind] ?? MODE_ICON.other;

export function ModeLabel({ mode, small = false }: { mode: SessionMode; small?: boolean }) {
  return (
    <span className={cx('ca-mode', small && 'is-small')} title={`Its mode: ${mode.label}. ${mode.detail}`}>
      <Icon name={iconOf(mode)} size={small ? 11 : 12} />
      <span className="ca-mode-name">{mode.label}</span>
    </span>
  );
}

/** The mode after the one it's in, in its agent's order and round again, as Shift+Tab steps
 *  through Claude Code's; the first when it's in none of them. */
export function nextMode(modes: SessionMode[], mode: SessionMode | undefined): SessionMode | null {
  const switchable = modes.filter((m) => m.id);
  if (switchable.length === 0) return null;
  const at = switchable.findIndex((m) => m.id === mode?.id);
  return switchable[(at + 1) % switchable.length];
}

/** The mode a session Codealong runs is in, as a chip in its box's foot that opens the modes its
 *  agent offers it (`modes`); picking one switches it (send.rs `switch_mode`). */
export function ModeMenu({ mode, modes, disabled, onPick, onDone }: { mode: SessionMode | undefined; modes: SessionMode[]; disabled: boolean; onPick: (id: string) => void; onDone: () => void }) {
  const options = modes
    .filter((m) => m.id)
    // Checked as the model menu's lines are (StartControls.tsx `agentLine`).
    .map((m) => ({ id: m.id!, text: `${m.label} ${m.detail}`, label: m.label, detail: m.detail, lead: <span className="ca-picker-check">{m.id === mode?.id ? <Icon name="check" size={14} /> : null}</span> }));
  const next = nextMode(modes, mode);
  return (
    <BoxMenu
      kind="mode"
      sections={[{ key: 'modes', options }]}
      value={mode?.id ?? null}
      icon={mode ? iconOf(mode) : 'shield'}
      iconSize={11}
      label={mode?.label ?? 'Mode'}
      title={`${mode ? `Its mode: ${mode.label}. ${mode.detail} ` : ''}Switch it${next ? ` (⇧⇥: ${next.label})` : ''}`}
      disabled={disabled}
      placeholder="Mode"
      onPick={(id) => {
        if (id !== mode?.id) onPick(id);
      }}
      onDone={onDone}
    />
  );
}
