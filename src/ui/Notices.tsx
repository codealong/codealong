import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { count } from '../format';
import { noticeAction, noticeId, noticeLabel, untilPhrase } from '../notices';
import type { Notice } from '../types';
import { Button, Icon } from './primitives';

interface NoticesProps {
  /** The notices to show (`shownNotices`). */
  notices: Notice[];
  now: number;
  onDismiss: (n: Notice) => void;
  /** Opens what a notice offers (`noticeAction`). */
  onOpen: (opens: 'settings-file') => void;
}

/** What isn't working in the background, as one chip: the notice in a few words (how many,
 *  when there are several), a click for the whole of it, and a button that dismisses it. The
 *  same wherever a window shows them. */
export function Notices({ notices, now, onDismiss, onOpen }: NoticesProps) {
  const chip = useRef<HTMLSpanElement>(null);
  const [open, setOpen] = useState(false);
  const [, place] = useState(0);
  const none = notices.length === 0;
  // The card goes with its notices, and on a click or a key anywhere else; it follows its chip
  // as the window is sized.
  useEffect(() => {
    if (none) setOpen(false);
  }, [none]);
  useEffect(() => {
    if (!open) return;
    const away = (e: Event) => {
      const at = e.target as Element | null;
      if (!at?.closest?.('.ca-notice-card') && !at?.closest?.('.ca-notice')) setOpen(false);
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false);
    };
    const again = () => place((n) => n + 1);
    window.addEventListener('pointerdown', away, true);
    window.addEventListener('keydown', key);
    window.addEventListener('resize', again);
    return () => {
      window.removeEventListener('pointerdown', away, true);
      window.removeEventListener('keydown', key);
      window.removeEventListener('resize', again);
    };
  }, [open]);
  if (none) return null;
  const several = notices.length > 1;
  return (
    <span ref={chip} className={open ? 'ca-notice is-open' : 'ca-notice'} role="status">
      <button type="button" className="ca-notice-open" aria-expanded={open} onClick={() => setOpen(!open)}>
        <Icon name="alert" size={13} />
        <span>{several ? count(notices.length, 'problem') : noticeLabel(notices[0], now)}</span>
      </button>
      <button type="button" className="ca-notice-x" aria-label={several ? 'Dismiss all' : `Dismiss: ${notices[0].title}`} title={several ? 'Dismiss all' : 'Dismiss'} onClick={() => notices.forEach(onDismiss)}>
        <Icon name="x" size={12} />
      </button>
      {open && chip.current && (
        <NoticeCard
          notices={notices}
          now={now}
          anchor={chip.current}
          onDismiss={onDismiss}
          onOpen={(opens) => {
            setOpen(false);
            onOpen(opens);
          }}
        />
      )}
    </span>
  );
}

function NoticeCard({ notices, now, anchor, onDismiss, onOpen }: { notices: Notice[]; now: number; anchor: HTMLElement; onDismiss: (n: Notice) => void; onOpen: (opens: 'settings-file') => void }) {
  const ref = useRef<HTMLDivElement>(null);
  // Under the chip, its right edge on the chip's, kept inside the window.
  useLayoutEffect(() => {
    const card = ref.current;
    if (!card) return;
    const r = anchor.getBoundingClientRect();
    card.style.left = `${Math.max(8, Math.min(r.right - card.offsetWidth, window.innerWidth - card.offsetWidth - 8))}px`;
    card.style.top = `${r.bottom + 6}px`;
  });
  return createPortal(
    <div ref={ref} className="ca-notice-card" role="dialog" aria-label="What isn't working">
      {notices.map((n) => {
        const until = untilPhrase(n, now);
        const action = noticeAction(n);
        return (
          <section key={noticeId(n)} className="ca-notice-item">
            <div className="ca-notice-title">{n.title}</div>
            <div className="ca-notice-text">{n.text.replace(/`/g, '')}</div>
            {until && <div className="ca-notice-until">{until}</div>}
            <div className="ca-notice-foot">
              {action && (
                <Button size="sm" variant="primary" onClick={() => onOpen(action.opens)}>
                  {action.label}
                </Button>
              )}
              <Button size="sm" onClick={() => onDismiss(n)}>
                Dismiss
              </Button>
            </div>
          </section>
        );
      })}
    </div>,
    document.body,
  );
}
