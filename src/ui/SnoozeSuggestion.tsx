// A snooze offered for a session whose last reply says its work goes on at a later time
// (snooze.ts `suggestedSnooze`): one line above its box, in every window that shows a session
// with one, that snoozes it until then with a click. It's the first line of the session's Snooze
// menu, run from there (`suggestedIn`), so the click does what the menu and its chord do
// (`SUGGESTED_CHORD`, ⌃⌘8): a window working through what waits on you goes on to the next. Who
// read the reply that way is under the pointer: Jev (insights.rs `waits_question`), or the agent
// that tagged it (`codealong session tag --waits-until`). Nothing snoozes by itself, and the line
// goes with the reply it read: once the session is sent something, a turn of its starts, it's
// snoozed, or the time comes. A snooze until a usage limit lifts is its banner's button instead
// (SessionHeader.tsx), beside what the banner says of the limit.

import type { MenuGroup } from '../contextMenu';
import { chordKeys, chordSigns } from '../menus';
import { SUGGESTED_CHORD, suggestedIn, suggestedSnooze, untilWords } from '../snooze';
import type { Session } from '../types';
import { Icon, Kbd } from './primitives';

interface Props {
  session: Session;
  /** The session's Snooze menu (menus.ts `sessionActions`), which has the suggested time. */
  snooze: MenuGroup | undefined;
  now: number;
}

export function SnoozeSuggestion({ session, snooze, now }: Props) {
  const suggested = suggestedSnooze(session, now);
  const action = suggestedIn(snooze);
  if (suggested?.why !== 'reply' || !action) return null;
  const when = untilWords(suggested.until, now);
  return (
    <div className="ca-suggest ca-snoozesuggest" role="group" aria-label="Suggested snooze">
      <button
        type="button"
        className="ca-suggest-reply"
        title={`Snooze it until ${when}: it leaves what waits on you until then (${chordSigns(SUGGESTED_CHORD)}). ${suggested.by}`}
        aria-keyshortcuts={SUGGESTED_CHORD.replace('Cmd', 'Meta')}
        onClick={() => action.run()}
      >
        <Icon name="zzz" size={13} />
        <span className="ca-suggest-text">
          Snooze until {when}
          <span className="ca-snoozesuggest-why">as its reply says</span>
        </span>
        <Kbd keys={chordKeys(SUGGESTED_CHORD)} />
      </button>
    </div>
  );
}
