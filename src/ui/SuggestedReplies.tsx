// Replies another session suggested for this one (`codealong session suggest`, suggestions.rs): a
// supervisor that reads the sessions waiting on you writes what you might answer each, and they
// show above the session's box, in every window that has it. A click sends one as it is, as your
// own message, the way ⏎ sends what's typed; ⌘-click sends it and goes on to the next session that
// waits on you, as ⌘⏎ does, where there's one; ⌥-click puts it in the box to change first. The box
// is still yours: type anything instead. ↑ from the empty box goes into the list, ↑ ↓ move through
// it, ⏎ sends (⌘⏎, ⌥⏎ as the clicks), its number alone sends that one, and ↓ past the last or
// Escape goes back to the box. ⌘ and its number (menus.ts `SUGGESTION_CHORDS`) sends one from the
// box too, and from anywhere in a window that works through what waits on you; the box listens
// for them (SessionReply.tsx), and each reply shows its key at its end while it works there, its
// arrow otherwise. A reply's right-click menu has the same, and Dismiss takes them all away on the
// session's Mac. They go by themselves once something is sent to the session or a turn of its
// starts (board::suggestions_hold).

import { forwardRef, type KeyboardEvent, type MouseEvent } from 'react';
import { showContextMenu } from '../contextMenu';
import { chordKeys, SUGGESTION_CHORDS, suggestionChord } from '../menus';
import type { Key } from '../queue';
import type { Suggestions } from '../types';
import { cx, Icon, Kbd } from './primitives';

/** How a pick goes: sent as it is, sent and then on to the next that waits on you, or put in the
 *  box to change first. */
export type PickHow = 'send' | 'next' | 'edit';

interface Props {
  suggestions: Suggestions;
  onPick: (text: string, how: PickHow) => void;
  onDismiss: () => void;
  /** Why they can't be sent now, when they can't: the session waits on something that can't be
   *  answered from here. They can still be put in the box. */
  blocked?: string;
  /** What sending one does when it isn't only sent: it denies the call the session waits on and
   *  tells it this instead. */
  denies?: boolean;
  /** There's a next session waiting on you to go on to. */
  next?: boolean;
  /** Gives the box the keyboard back. */
  onLeave: () => void;
  /** Their keys work from anywhere in the window, not only from the box and the list (the notch,
   *  the waiting window), so they show whatever has the keyboard. */
  keyed?: boolean;
}

/** The way a click or a key with these modifiers picks a reply. */
function how(e: { metaKey: boolean; altKey: boolean }, next: boolean): PickHow {
  if (e.altKey) return 'edit';
  return e.metaKey && next ? 'next' : 'send';
}

/** The reply a key sends, when it's one of their keys (`SUGGESTION_CHORDS`). */
export function chordedReply(e: Key, replies: string[]): string | null {
  const index = suggestionChord(e);
  return index === null ? null : (replies[index] ?? null);
}

export const SuggestedReplies = forwardRef<HTMLDivElement, Props>(function SuggestedReplies({ suggestions: g, onPick, onDismiss, blocked, denies, next = false, onLeave, keyed = false }, ref) {
  const sends = denies ? 'Deny the call it waits on, and send this instead' : 'Send this as your reply';
  const title = blocked ?? `${sends} · ⌥-click to change it first${next ? ' · ⌘-click sends and goes on to the next' : ''}`;
  const pick = (text: string, h: PickHow) => {
    if (blocked && h !== 'edit') return;
    onPick(text, h);
  };
  const menu = (e: MouseEvent, text: string, chord: string | undefined) => {
    e.preventDefault();
    void showContextMenu([
      { label: denies ? 'Deny and send this' : 'Send', run: () => pick(text, 'send'), enabled: !blocked, accelerator: chord },
      ...(next ? [{ label: 'Send and go on to the next', run: () => pick(text, 'next'), enabled: !blocked, accelerator: chord && `Cmd+${chord}` }] : []),
      { label: 'Change before sending', run: () => pick(text, 'edit') },
      'separator',
      { label: 'Dismiss suggestions', run: onDismiss },
    ]);
  };
  // The list's keys are its own: the window's would move its list, open a row or close the peek.
  const keys = (e: KeyboardEvent<HTMLButtonElement>, text: string) => {
    if (e.nativeEvent.isComposing) return;
    const all = [...(e.currentTarget.parentElement?.querySelectorAll<HTMLButtonElement>('.ca-suggest-reply') ?? [])];
    const at = all.indexOf(e.currentTarget);
    const plain = !e.shiftKey && !e.ctrlKey;
    // Its number alone, by the key's place: some layouts type digits with ⇧.
    const number = /^Digit([1-9])$/.exec(e.code);
    const chosen = number && !e.metaKey && !e.ctrlKey && !e.altKey ? g.replies[Number(number[1]) - 1] : undefined;
    if (chosen !== undefined) {
      pick(chosen, 'send');
    } else if ((e.key === 'ArrowUp' || e.key === 'ArrowDown') && plain && !e.metaKey && !e.altKey) {
      const to = at + (e.key === 'ArrowUp' ? -1 : 1);
      if (to >= all.length) onLeave();
      else all[Math.max(0, to)]?.focus();
    } else if (e.key === 'Escape') {
      onLeave();
    } else if (e.key === 'Enter' && plain) {
      pick(text, how(e, next));
    } else {
      return;
    }
    e.preventDefault();
    e.stopPropagation();
  };
  const by = g.byTitle ? `Suggested by ${g.byTitle}` : 'Suggested replies';
  return (
    <div ref={ref} className={cx('ca-suggest', keyed && 'is-keyed')} role="group" aria-label="Suggested replies">
      <div className="ca-suggest-head">
        <Icon name="sparkles" size={13} />
        <span className="ca-suggest-by" title={g.by ? `${by} (${g.by})` : by}>
          {by}
        </span>
        <button type="button" className="ca-linkbtn" title="Take these suggestions away" onClick={onDismiss}>
          Dismiss
        </button>
      </div>
      {g.note && <p className="ca-suggest-note">{g.note}</p>}
      <div className="ca-suggest-replies">
        {g.replies.map((text, i) => {
          const chord: string | undefined = SUGGESTION_CHORDS[i];
          return (
            <button
              key={text}
              type="button"
              className="ca-suggest-reply"
              title={chord ? `${title} · ${chordKeys(chord).join('')}` : title}
              aria-disabled={!!blocked}
              aria-keyshortcuts={chord?.replace('Cmd', 'Meta')}
              onClick={(e) => pick(text, how(e, next))}
              onKeyDown={(e) => keys(e, text)}
              onContextMenu={(e) => menu(e, text, chord)}
            >
              <span className="ca-suggest-text">{text}</span>
              {/* Its key while it works (SuggestedReplies.css), its arrow otherwise. */}
              {chord && <Kbd keys={chordKeys(chord)} />}
              <Icon name="arrow-up" size={13} />
            </button>
          );
        })}
      </div>
    </div>
  );
});
