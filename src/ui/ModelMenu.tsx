// A quiet chip in a box's foot that opens a menu above the box (`BoxMenu`): a session's account,
// model and effort (SessionAgent.tsx), an account with its model and effort wherever one is picked
// (AgentMenu.tsx), an automation's agent (AgentPromptBox.tsx), and the mode of a session Codealong
// runs, at the foot's other end (ModeLabel.tsx `ModeMenu`). What picking a line does is the
// caller's.

import { useRef, useState, type CSSProperties, type ReactNode } from 'react';
import { scaleOf } from '../zoom';
import { Picker, type PickerSection } from './Picker';
import { Icon } from './primitives';

/** The menu is never shorter than this, and keeps this far from the window's top. */
const MENU_MIN = 160;
const MENU_MARGIN = 12;

/** What a box menu has under its lines: a row of its own, or one made with what closes the menu,
 *  for a pick there that's done with it (a running session's effort, SessionAgent.tsx). */
export type MenuFoot = ReactNode | ((close: () => void) => ReactNode);

/** A box menu's classes, and its chip's, by what it picks. */
const BOX_MENU = { model: ['ca-modelmenu', 'ca-modelmenu-chip'], mode: ['ca-modemenu', 'ca-modemenu-chip'] } as const;

interface BoxMenuProps {
  kind: keyof typeof BOX_MENU;
  sections: PickerSection[];
  value: string | null;
  /** Before the chip's words, at its size. */
  icon?: string;
  iconSize?: number;
  label: ReactNode;
  detail?: string | null;
  title: string;
  disabled?: boolean;
  placeholder: string;
  foot?: MenuFoot;
  onStep?: (delta: -1 | 1) => void;
  onPick: (id: string) => void;
  onDone: () => void;
  /** Opens under the box rather than over it: a box near the top of what scrolls (Settings),
   *  where the room above is cut off. */
  down?: boolean;
}

/** A quiet chip beside Send that opens its menu above the box, against the box's right edge, or
 *  under it (`down`). */
export function BoxMenu({ kind, sections, value, icon, iconSize = 14, label, detail, title, disabled, placeholder, foot, onStep, onPick, onDone, down }: BoxMenuProps) {
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
        {icon && <Icon name={icon} size={iconSize} />}
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
          foot={typeof foot === 'function' ? foot(close) : foot}
          onStep={onStep}
        />
      )}
    </div>
  );
}
