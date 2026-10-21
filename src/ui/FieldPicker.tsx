// One of many picked in a form (the schedule dialog's session and worktree): a field that shows
// the option picked, as the menu lists it, and opens the quick panel's menu (Picker.tsx) under
// it, to find another by typing. A native select can't filter, nor show a session's status and
// age.

import { useEffect, useRef, useState, type ReactNode } from 'react';
import { Picker, type PickerSection } from './Picker';
import { cx, Icon } from './primitives';

interface FieldPickerProps {
  /** Its name, for a screen reader and as the field's tooltip. */
  name: string;
  /** A word before what's picked, saying what it is: To a session, In a worktree. */
  prefix?: string;
  sections: PickerSection[];
  value: string;
  /** What the field says for what's picked, when it needs more than its line on the menu, which
   *  its section's heading puts in context (a worktree's project). */
  said?: { label: ReactNode; detail?: ReactNode };
  /** What the field says when nothing on the menu is picked. */
  none: ReactNode;
  placeholder: string;
  empty?: string;
  onPick: (id: string) => void;
}

export function FieldPicker({ name, prefix, sections, value, said, none, placeholder, empty, onPick }: FieldPickerProps) {
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const button = useRef<HTMLButtonElement>(null);
  const picked = sections.flatMap((s) => s.options).find((o) => o.id === value);
  const detail = said ? said.detail : picked?.detail;

  // The dialog's body scrolls: the menu is brought into it as it opens.
  useEffect(() => {
    if (open) box.current?.querySelector('.ca-picker')?.scrollIntoView({ block: 'nearest' });
  }, [open]);

  // Back to the field once the menu goes, unless the press that closed it focused something else.
  const close = () => {
    setOpen(false);
    requestAnimationFrame(() => {
      if (!document.activeElement || document.activeElement === document.body) button.current?.focus();
    });
  };

  return (
    <div ref={box} className="ca-fieldpick">
      <button ref={button} type="button" data-picker-trigger className={cx('ca-fieldpick-btn', !picked && 'is-none')} aria-label={name} title={name} aria-haspopup="dialog" aria-expanded={open} onClick={() => (open ? close() : setOpen(true))}>
        {prefix && <span className="ca-fieldpick-prefix">{prefix}</span>}
        {picked?.lead}
        <span className="ca-fieldpick-text">{picked ? (said?.label ?? picked.label) : none}</span>
        {detail && <span className="ca-fieldpick-detail">{detail}</span>}
        {picked?.trail}
        <Icon name="chevron-down" size={12} />
      </button>
      {open && (
        <Picker
          sections={sections}
          value={value}
          placeholder={placeholder}
          empty={empty}
          onPick={(id) => {
            onPick(id);
            close();
          }}
          onClose={close}
        />
      )}
    </div>
  );
}
