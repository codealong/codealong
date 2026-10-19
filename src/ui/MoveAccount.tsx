// Moving a session to another account of its harness, asked first: the account in its box's foot
// is a chip that opens its harness's accounts above the box (`AccountMenu`, SessionReply.tsx), and
// picking another asks whether to move it there (`MoveDialog`), as Move… does in the box a usage
// limit or a cut-off turn puts under its header (SessionHeader.tsx). SessionPanel.tsx holds the
// dialog, so every window showing a session asks the same. The move is the menu's Move to
// (menus.ts `moveAccounts`): a copy goes on under the account from what it had written, and this
// one is closed and archived (actions.rs `move_session`).

import { useState } from 'react';
import type { AccountMoves } from '../menus';
import type { Session } from '../types';
import { BoxMenu } from './ModelMenu';
import { AccountLabel, Button, Dialog, Icon, Seg } from './primitives';

/** What the move does, for the dialog's lede and a button's tooltip. */
export function moveWords(s: Pick<Session, 'title' | 'status' | 'attachCommand'>, to: string, terminal: string, goesOn: boolean): string {
  const where = s.attachCommand != null ? 'as a background job' : `in a new ${terminal} tab`;
  const first = s.status === 'working' ? ' Its turn is stopped first.' : '';
  const then = goesOn ? ', told to carry on where it stopped' : '';
  return `A copy of “${s.title}” goes on under ${to} ${where}${then}, and this one is closed and archived.${first}`;
}

/** The session's account, as a chip in its box's foot that opens the accounts it can move to,
 *  its own checked; picking another asks first (`onPick`, which opens `MoveDialog`). */
export function AccountMenu({ session: s, moves, onPick, onDone }: { session: Session; moves: AccountMoves; onPick: (account: string) => void; onDone: () => void }) {
  const line = (name: string, detail: string | undefined) => ({
    id: name,
    text: name,
    label: name,
    detail,
    lead: <span className="ca-picker-check">{name === s.account ? <Icon name="check" size={14} /> : null}</span>,
  });
  const options = [line(s.account, 'Its account'), ...moves.to.map((a) => line(a, a === moves.suggested ? 'Has room' : undefined))];
  return (
    <BoxMenu
      kind="account"
      sections={[{ key: 'accounts', options }]}
      value={s.account}
      label={<AccountLabel session={s} small />}
      title={`Its account: ${s.account}. Move it to another`}
      placeholder="Account"
      foot={<span className="ca-picker-foot-note">Another account moves it there, once you say so</span>}
      onPick={(id) => {
        if (id !== s.account) onPick(id);
      }}
      onDone={onDone}
    />
  );
}

interface MoveDialogProps {
  session: Session;
  moves: AccountMoves;
  /** The account picked to move it to; the one with room, or the first, without one. */
  to: string | null;
  terminal: string;
  onClose: () => void;
}

/** Asks before moving a session to another account, and which, where there's more than one. */
export function MoveDialog({ session: s, moves, to: asked, terminal, onClose }: MoveDialogProps) {
  const first = asked && moves.to.includes(asked) ? asked : (moves.suggested ?? moves.to[0]);
  const [to, setTo] = useState(first);
  const move = () => {
    moves.run(to);
    onClose();
  };
  return (
    <Dialog
      className="ca-movedlg"
      title="Move to another account"
      lede={moveWords(s, to, terminal, moves.goesOn)}
      onClose={onClose}
      onSubmit={move}
      foot={
        <>
          <Button variant="ghost" size="sm" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" size="sm" type="submit" autoFocus>
            Move to {to}
          </Button>
        </>
      }
    >
      {moves.to.length > 1 && (
        <div className="ca-movedlg-to">
          <span>To</span>
          <Seg label="Move to" options={moves.to.map((a) => ({ id: a, label: a, note: a === moves.suggested ? 'has room' : undefined }))} value={to} onPick={setTo} />
        </div>
      )}
    </Dialog>
  );
}
