// A session's model, as a chip beside Send in its box (SessionReply.tsx) that opens its account's
// models above the box: what a session that isn't running goes on with (ResumeBar.tsx), with the
// effort under them, or what a running one switches to (SessionModel.tsx). The lines are the
// start menu's for that account (StartControls.tsx `agentLine`); what picking one does is the
// caller's. The place a session that isn't running goes on in is a chip of the same kind beside
// it (`BoxMenu`).

import { useRef, useState, type CSSProperties, type ReactNode } from 'react';
import type { AgentOption, ModelOption } from '../start';
import type { StartAccount } from '../types';
import { scaleOf } from '../zoom';
import { Picker, type PickerSection } from './Picker';
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
  const section = { key: account.name, options: lines.map((o) => agentLine(o, account, models, value)) };
  return <BoxMenu kind="model" sections={[section]} value={value} label={label} detail={detail} title={title} disabled={disabled} placeholder="Model" foot={foot} onStep={onStep} onPick={onPick} onDone={onDone} />;
}

/** A box menu's classes, and its chip's, by what it picks. */
const BOX_MENU = { model: ['ca-modelmenu', 'ca-modelmenu-chip'], place: ['ca-placemenu', 'ca-placemenu-chip'] } as const;

interface BoxMenuProps {
  kind: keyof typeof BOX_MENU;
  sections: PickerSection[];
  value: string | null;
  /** Before the chip's words. */
  icon?: string;
  label: ReactNode;
  detail?: string | null;
  title: string;
  disabled?: boolean;
  placeholder: string;
  foot?: ReactNode;
  onStep?: (delta: -1 | 1) => void;
  onPick: (id: string) => void;
  onDone: () => void;
  /** Opens under the box rather than over it: a box near the top of what scrolls (Settings),
   *  where the room above is cut off. */
  down?: boolean;
}

/** A quiet chip beside Send that opens its menu above the box, against the box's right edge, or
 *  under it (`down`). */
export function BoxMenu({ kind, sections, value, icon, label, detail, title, disabled, placeholder, foot, onStep, onPick, onDone, down }: BoxMenuProps) {
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
    setRoom(top == null || down ? null : Math.max(MENU_MIN, top - MENU_MARGIN));
    setOpen(true);
  };
  const close = () => {
    setOpen(false);
    onDone();
  };
  return (
    <div ref={chip} className={BOX_MENU[kind][0]} style={room == null ? undefined : ({ '--ca-menu-room': `${room}px` } as CSSProperties)}>
      <button type="button" data-picker-trigger className={BOX_MENU[kind][1]} title={title} disabled={disabled} aria-haspopup="dialog" aria-expanded={open} onClick={() => (open ? close() : show())}>
        {icon && <Icon name={icon} size={14} />}
        <span>{label}</span>
        {detail && <span className="ca-modelmenu-detail">{detail}</span>}
        <Icon name="chevron-down" size={12} />
      </button>
      {open && (
        <Picker
          className={down ? 'is-down' : 'is-up'}
          sections={sections}
          value={value}
          placeholder={placeholder}
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
