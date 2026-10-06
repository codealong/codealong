// A session's model, as a chip beside Send in its box (SessionReply.tsx) that opens its account's
// models above the box: what a session that isn't running goes on with (ResumeBar.tsx), with the
// effort under them, or what a running one switches to (SessionModel.tsx). The lines are the
// start menu's for that account (StartControls.tsx `agentLine`); what picking one does is the
// caller's.

import { useRef, useState, type CSSProperties, type ReactNode } from 'react';
import type { AgentOption, ModelOption } from '../start';
import type { StartAccount } from '../types';
import { scaleOf } from '../zoom';
import { Picker } from './Picker';
import { Icon } from './primitives';
import { agentLine } from './StartControls';

/** The menu is never shorter than this, and keeps this far from the window's top. */
const MENU_MIN = 160;
const MENU_MARGIN = 12;

export interface ModelMenuProps {
  account: StartAccount;
  /** Its lines: the account's own model and its models, or some of them. */
  lines: AgentOption[];
  models: ModelOption[];
  /** The line checked, if any. */
  value: string | null;
  /** What the chip says: the model, and after it, quieter, the effort. */
  label: ReactNode;
  detail?: string | null;
  title: string;
  disabled?: boolean;
  /** The effort row under the lines, and ← → while the filter is empty. */
  foot?: ReactNode;
  onStep?: (delta: -1 | 1) => void;
  onPick: (id: string) => void;
  /** The menu closed, picked or not: the box takes the keyboard back. */
  onDone: () => void;
}

export function ModelMenu({ account, lines, models, value, label, detail, title, disabled, foot, onStep, onPick, onDone }: ModelMenuProps) {
  const [open, setOpen] = useState(false);
  // The room above the box, as the menu opens: a window sized to what it shows (the peek) has no
  // more than its conversation's height to open the menu into.
  const [room, setRoom] = useState<number | null>(null);
  const chip = useRef<HTMLDivElement>(null);
  const show = () => {
    const field = chip.current?.closest<HTMLElement>('.ca-composer-field');
    // On the screen's scale; the menu's height is in the card's own pixels, which a zoom makes
    // bigger (zoom.ts `scaleOf`).
    const top = field ? field.getBoundingClientRect().top / scaleOf(field) : null;
    setRoom(top == null ? null : Math.max(MENU_MIN, top - MENU_MARGIN));
    setOpen(true);
  };
  const close = () => {
    setOpen(false);
    onDone();
  };
  const section = { key: account.name, options: lines.map((o) => agentLine(o, account, models, value)) };
  return (
    <div ref={chip} className="ca-modelmenu" style={room == null ? undefined : ({ '--ca-menu-room': `${room}px` } as CSSProperties)}>
      <button type="button" data-picker-trigger className="ca-modelmenu-chip" title={title} disabled={disabled} aria-haspopup="dialog" aria-expanded={open} onClick={() => (open ? close() : show())}>
        <span>{label}</span>
        {detail && <span className="ca-modelmenu-detail">{detail}</span>}
        <Icon name="chevron-down" size={12} />
      </button>
      {open && (
        <Picker
          className="is-up"
          sections={[section]}
          value={value}
          placeholder="Model"
          onPick={(id) => {
            onPick(id);
            close();
          }}
          onClose={close}
          foot={foot}
          onStep={onStep}
        />
      )}
    </div>
  );
}
