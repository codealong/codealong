// Quick confirmation modal when moving or handing off a session to another account or provider:
// picking any model under another account in the account selector (SessionAgent.tsx), or clicking
// Move… in a header banner (SessionHeader.tsx), opens `MoveDialog` to briefly explain that the
// session is moving across accounts and how that works (lossless native copy vs. handoff brief +
// full transcript file), with a Keep original open toggle and ⏎ to confirm.

import { useState } from 'react';
import type { MoveOptions } from '../api';
import { moveDialogTitle, moveDialogWords } from '../handoff';
import { HARNESS_WORD } from '../harness';
import type { AccountMoves } from '../menus';
import type { Session } from '../types';
import { Button, CheckBox, Dialog, Seg } from './primitives';

interface MoveDialogProps {
  session: Session;
  moves: AccountMoves;
  /** The account picked from the account selector, or null when opened from Move… in a header banner. */
  to: string | null;
  /** Model picked under `to` in the account selector, if any. */
  model?: string | null;
  /** Human-readable name of `model`, for the confirmation copy. */
  modelLabel?: string | null;
  terminal: string;
  onClose: () => void;
}

/** Brief confirmation before moving or handing off a session to another account. */
export function MoveDialog({ session: s, moves, to: asked, model: askedModel = null, modelLabel: askedModelLabel = null, terminal, onClose }: MoveDialogProps) {
  const first = asked && moves.to.includes(asked) ? asked : (moves.suggested ?? moves.to[0]);
  const [to, setTo] = useState(first);
  const [keep, setKeep] = useState(false);

  const target = moves.targets.find((t) => t.account === to) ?? moves.targets[0];
  // Model applies only to the account it was picked under.
  const model = to === asked ? askedModel : null;
  const modelLabel = to === asked ? askedModelLabel : null;
  const opts: MoveOptions | undefined = keep || model ? { keep: keep || undefined, model: model || undefined } : undefined;

  const move = () => {
    moves.run(to, opts);
    onClose();
  };

  const title = moveDialogTitle(target, s.harness, keep);
  const lede = target ? moveDialogWords(s, target, terminal, moves.goesOn, opts, modelLabel) : '';
  // When opened from Move… (no account pre-picked), let the user pick among eligible accounts.
  const showPicker = !asked && moves.targets.length > 1;

  return (
    <Dialog
      className="ca-movedlg"
      title={title}
      lede={lede}
      onClose={onClose}
      onSubmit={move}
      aside={
        <label className="ca-check ca-movedlg-keep" title="Starts a linked handoff copy without closing or archiving this session">
          <CheckBox checked={keep} onChange={setKeep} />
          <span>Keep original open</span>
        </label>
      }
      foot={
        <>
          <Button variant="ghost" size="sm" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" size="sm" type="submit" autoFocus>
            {keep ? `Hand off to ${to}` : `Move to ${to}`}
          </Button>
        </>
      }
    >
      {showPicker && (
        <div className="ca-movedlg-to">
          <span>To</span>
          <Seg
            label="Move to"
            options={moves.targets.map((t) => ({
              id: t.account,
              label: t.account,
              note:
                t.account === moves.suggested
                  ? t.harness !== s.harness
                    ? `${HARNESS_WORD[t.harness]} · has room`
                    : 'has room'
                  : t.harness !== s.harness
                    ? HARNESS_WORD[t.harness]
                    : undefined,
            }))}
            value={to}
            onPick={setTo}
          />
        </div>
      )}
    </Dialog>
  );
}
