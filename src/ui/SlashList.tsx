// The list a reply box shows while a slash and the start of a name are typed: Codealong's own
// commands (slash.ts), then the session's skills (skills.ts). ↑ ↓ choose, ⇥ or ⏎ finishes the
// name, and the box keeps the keyboard throughout: the list is drawn, not focused.

import { useEffect, useRef } from 'react';
import { cx } from './primitives';

export interface SlashOption {
  /** As typed after the slash. */
  name: string;
  /** What may follow it. */
  hint?: string;
  about: string;
  /** Codealong's own, which acts on the session from here instead of going to it. */
  own?: boolean;
}

export function SlashList({ options, active, onPick, onHover }: { options: SlashOption[]; active: number; onPick: (o: SlashOption) => void; onHover: (i: number) => void }) {
  const list = useRef<HTMLDivElement>(null);
  useEffect(() => {
    list.current?.querySelector('.is-active')?.scrollIntoView({ block: 'nearest' });
  }, [active]);
  return (
    <div ref={list} className="ca-slash" role="listbox" aria-label="Commands and skills">
      {options.map((o, i) => (
        <div
          key={`${o.own ? 'own' : 'skill'}:${o.name}`}
          role="option"
          aria-selected={i === active}
          className={cx('ca-slash-row', i === active && 'is-active')}
          title={o.about}
          // The box keeps the cursor.
          onMouseDown={(e) => e.preventDefault()}
          onMouseMove={() => i !== active && onHover(i)}
          onClick={() => onPick(o)}
        >
          <span className="ca-slash-name">
            /{o.name}
            {o.hint && <span className="ca-slash-hint"> {o.hint}</span>}
          </span>
          <span className="ca-slash-about">{o.own ? `Codealong: ${o.about}` : o.about}</span>
        </div>
      ))}
    </div>
  );
}
