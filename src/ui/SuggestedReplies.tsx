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
// starts (board::suggestions_hold). For questions a session asks, what's suggested is choices of
// their own, which its question box shows picked (SessionReply.tsx `Questions`), with the same
// mark and dismiss (`SuggestionMark`, `DismissSuggestions`) in its top row.

import { forwardRef, useState, type KeyboardEvent, type MouseEvent } from 'react';
import { showContextMenu } from '../contextMenu';
import { chordKeys, chordSigns, SUGGESTION_CHORDS, suggestionChord } from '../menus';
import type { Key } from '../queue';
import type { Suggestions } from '../types';
import { Button, cx, Icon, Kbd } from './primitives';
import { bare, cmd, held, keysIn, typed } from '../platform';

/** How a pick goes: sent as it is, sent and then on to the next that waits on you, or put in the
 *  box to change first. */
export type PickHow = 'send' | 'next' | 'edit';

/** What a send refuses while the session waits on you: the call it wants to make, or the
 *  questions it asks. */
export type Denies = 'call' | 'question';

interface Props {
  suggestions: Suggestions;
  onPick: (text: string, how: PickHow) => void;
  onDismiss: () => void;
  /** Why they can't be sent now, when they can't: the session waits on something that can't be
   *  answered from here. They can still be put in the box. */
  blocked?: string;
  /** What sending one does when it isn't only sent: it denies the call the session waits on, or
   *  declines the questions it asks, and tells it this instead. */
  denies?: Denies;
  /** There's a next session waiting on you to go on to. */
  next?: boolean;
  /** Gives the box the keyboard back. */
  onLeave: () => void;
  /** Their keys work from anywhere in the window, not only from the box and the list (the notch,
   *  the waiting window), so they show whatever has the keyboard. */
  keyed?: boolean;
}

/** The way a click or a key with these modifiers picks a reply. */
function how(e: { metaKey: boolean; ctrlKey: boolean; altKey: boolean }, next: boolean): PickHow {
  if (e.altKey) return 'edit';
  return cmd(e) && next ? 'next' : 'send';
}

/** The reply a key sends, when it's one of their keys (`SUGGESTION_CHORDS`). */
export function chordedReply(e: Key, replies: string[]): string | null {
  const index = suggestionChord(e);
  return index === null ? null : (replies[index] ?? null);
}

export const SuggestedReplies = forwardRef<HTMLDivElement, Props>(function SuggestedReplies({ suggestions: g, onPick, onDismiss, blocked, denies, next = false, onLeave, keyed = false }, ref) {
  const sends = denies === 'question' ? 'Decline the question it asks, and send this instead' : denies ? 'Deny the call it waits on, and send this instead' : 'Send this as your reply';
  const title = blocked ?? keysIn(`${sends} · ⌥-click to change it first${next ? ' · ⌘-click sends and goes on to the next' : ''}`);
  const pick = (text: string, h: PickHow) => {
    if (blocked && h !== 'edit') return;
    onPick(text, h);
  };
  const menu = (e: MouseEvent, text: string, chord: string | undefined) => {
    e.preventDefault();
    void showContextMenu([
      { label: denies === 'question' ? 'Decline and send this' : denies ? 'Deny and send this' : 'Send', run: () => pick(text, 'send'), enabled: !blocked, accelerator: chord },
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
    // Its number alone, by the key's place: some layouts type digits with ⇧.
    const number = /^Digit([1-9])$/.exec(e.code);
    const chosen = number && typed(e) ? g.replies[Number(number[1]) - 1] : undefined;
    if (chosen !== undefined) {
      pick(chosen, 'send');
    } else if ((e.key === 'ArrowUp' || e.key === 'ArrowDown') && bare(e)) {
      const to = at + (e.key === 'ArrowUp' ? -1 : 1);
      if (to >= all.length) onLeave();
      else all[Math.max(0, to)]?.focus();
    } else if (e.key === 'Escape') {
      onLeave();
    } else if (e.key === 'Enter' && (bare(e) || held(e, 'Cmd') || held(e, 'Alt') || held(e, 'Alt', 'Cmd'))) {
      pick(text, how(e, next));
    } else {
      return;
    }
    e.preventDefault();
    e.stopPropagation();
  };
  const note = useNoteOpen(g);
  return (
    <div ref={ref} className={cx('ca-suggest', keyed && 'is-keyed')} role="group" aria-label="Suggested replies">
      <SuggestionMark suggestions={g} note={note} />
      <DismissSuggestions onDismiss={onDismiss} />
      <div className="ca-suggest-replies">
        {g.replies.map((text, i) => {
          const chord: string | undefined = SUGGESTION_CHORDS[i];
          return (
            <button
              key={text}
              type="button"
              className="ca-suggest-reply"
              title={chord ? `${title} · ${chordSigns(chord)}` : title}
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
      <SuggestionNote suggestions={g} note={note} />
    </div>
  );
});

/** The sparkle that marks what another session suggested: at the start of suggested replies' card,
 *  and in a question box's top row with the choices picked for it. Who suggested them and their
 *  note are under the pointer: the card's tint and the sparkle already say they're suggestions,
 *  and the note, written for you to weigh, read as clutter over every reply. With a note it's a
 *  button, which shows the note under them (`useNoteOpen`, `SuggestionNote`), so the keyboard and
 *  a screen reader get to it too. */
export function SuggestionMark({ suggestions: g, note }: { suggestions: Suggestions; note?: NoteOpen }) {
  const by = g.byTitle ? `Suggested by ${g.byTitle}` : g.replies.length ? 'Suggested replies' : 'Suggested';
  const who = g.by ? `${by} (${g.by})` : by;
  if (!g.note || !note) {
    return (
      <span className="ca-suggest-mark" title={who} role="img" aria-label={who}>
        <Icon name="sparkles" size={13} />
      </span>
    );
  }
  return (
    <button type="button" className={cx('ca-suggest-mark', 'has-note', note.open && 'is-open')} title={`${who}\n\n${g.note}`} aria-label={`${who}: ${note.open ? 'hide' : 'show'} its note`} aria-expanded={note.open} onClick={note.toggle}>
      <Icon name="sparkles" size={13} />
    </button>
  );
}

/** Whether the note of the suggestions shown is open, for their mark and the note under them. A
 *  new set of suggestions starts closed. */
export interface NoteOpen {
  open: boolean;
  toggle: () => void;
}

export function useNoteOpen(g: Suggestions | null | undefined): NoteOpen {
  const [openAt, setOpenAt] = useState<number | null>(null);
  const at = g?.at ?? null;
  return { open: at !== null && openAt === at, toggle: () => setOpenAt((was) => (was === at ? null : at)) };
}

/** The note the suggestions came with, once their mark opened it. */
export function SuggestionNote({ suggestions: g, note }: { suggestions: Suggestions | null | undefined; note: NoteOpen }) {
  if (!g?.note || !note.open) return null;
  return <p className="ca-suggest-note">{g.note}</p>;
}

/** Takes the suggestions away, here at once and on the session's Mac after. */
export function DismissSuggestions({ onDismiss }: { onDismiss: () => void }) {
  return <Button variant="ghost" size="sm" icon="x" className="ca-suggest-dismiss" aria-label="Dismiss suggestions" title="Dismiss: take these suggestions away" onClick={onDismiss} />;
}

/** Choices another session picked for questions it asks: their mark and their dismiss together, in
 *  the question box's top row, tinted as suggested replies' card is. */
export function SuggestedPicks({ suggestions, note, onDismiss }: { suggestions: Suggestions; note: NoteOpen; onDismiss: () => void }) {
  return (
    <span className="ca-suggest-picks" role="group" aria-label="Suggested choices">
      <SuggestionMark suggestions={suggestions} note={note} />
      <DismissSuggestions onDismiss={onDismiss} />
    </span>
  );
}
