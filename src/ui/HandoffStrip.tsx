// Inline confirmation bar docked at the top of a session's reply box (SessionReply.tsx) when
// you switch the account chip to another account or provider, or click Move… on a header banner.
// Keeps the full conversation visible above while letting you adjust model/effort/place in the
// chip, optionally type a next-step note in the reply box, toggle Keep open, and press ⏎ (or click
// Move / Hand off) to confirm — or Escape / ✕ to cancel without touching your draft.

import type { MoveOptions } from '../api';
import { handoffSummary, isNativeMove, moveDialogWords, type MoveTarget, type StagedHandoff } from '../handoff';
import type { Session, StartPlace } from '../types';
import { CheckBox, cx, Icon } from './primitives';

interface HandoffStripProps {
  session: Session;
  target: MoveTarget;
  terminal: string;
  goesOn: boolean;
  staged: StagedHandoff;
  effectivePlace: StartPlace | null;
  takesModel: boolean;
  prompt: string;
  onToggleKeep: (keep: boolean) => void;
  onCancel: () => void;
}

/** Compact inline confirmation strip for a staged move or cross-provider handoff. */
export function HandoffStrip({ session, target, terminal, goesOn, staged, effectivePlace, takesModel, prompt, onToggleKeep, onCancel }: HandoffStripProps) {
  const rawOpts: MoveOptions = {
    keep: staged.keep || undefined,
    place: staged.place ?? undefined,
    model: takesModel && staged.model ? staged.model : undefined,
    effort: takesModel && staged.effort ? staged.effort : undefined,
    prompt: prompt.trim() || undefined,
  };
  const displayOpts: MoveOptions | undefined = isNativeMove(target, rawOpts) ? undefined : { ...rawOpts, place: effectivePlace ?? undefined };
  const summary = handoffSummary(session, target, goesOn, displayOpts);
  const fullLede = moveDialogWords(session, target, terminal, goesOn, displayOpts);

  return (
    <div className={cx('ca-handoff-bar', staged.keep && 'is-keep')} role="status">
      <Icon name={staged.keep ? 'git-fork' : 'arrow-up-right'} size={13} className="ca-handoff-icon" />
      <span className="ca-handoff-text" title={fullLede}>
        {summary}
      </span>
      <span className="ca-handoff-acts">
        <label className="ca-check ca-handoff-keep" title="Starts a linked handoff copy without closing or archiving this session">
          <CheckBox checked={staged.keep} onChange={onToggleKeep} />
          <span>Keep open</span>
        </label>
        <button type="button" className="ca-handoff-cancel" aria-label="Cancel move" title="Cancel (Esc)" onClick={onCancel}>
          <Icon name="x" size={13} />
        </button>
      </span>
    </div>
  );
}
