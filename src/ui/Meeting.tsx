// The meeting on offer (src/meetings.ts, src-tauri/src/meetings.rs): as one in your calendar is
// about to start, every window that has a place for it asks whether to take its notes. One
// component, drawn as a card under the notch's pills or as one line (ahead of the floating bar's
// pills, and in the main window beside its notices). Take notes starts a session for the
// meeting, which listens; Not this one puts the offer away for good.
//
// Before its offer, from `[voice] meeting_countdown` minutes before it starts, the notch and the
// floating bar count down to it (`MeetingSoon`), quietly: no sound, and nothing opens by itself.
// A click on the countdown shows its offer then, in the window it was clicked in, to take it
// early; the main window counts down on its Meetings button, whose dialog opens on it.

import { useState, type MouseEvent } from 'react';
import { answerMeeting } from '../api';
import { meetingCountdown, meetingWhen, meetingWith } from '../meetings';
import { errorText } from '../report';
import type { Meeting, MeetingOffer } from '../types';
import { Button, cx, Icon } from './primitives';
import { useNow } from './useNow';

/** Its name, when it starts and who's in it, as a tooltip says them. */
function meetingWords(m: Meeting, now: number): string {
  const who = meetingWith(m);
  return `${m.title.trim() || 'A meeting'}, ${meetingWhen(m, now)}${who ? `, ${who}` : ''}`;
}

/** The meeting counted down to (`Board.meetingSoon`): the minutes to go beside the notch's
 *  camera, or its name and when as a pill of the floating bar. A click shows its offer early
 *  (`onOpen`). */
export function MeetingSoon({ meeting, look, onOpen }: { meeting: Meeting; look: 'notch' | 'pill'; onOpen: () => void }) {
  const now = useNow(5000);
  return (
    <button
      type="button"
      className={cx('ca-meeting-soon', `is-${look}`)}
      title={`${meetingWords(meeting, now)}. Click to take its notes.`}
      onClick={(e) => {
        // Not a click on what it's drawn over (the notch, which a click opens on a session).
        e.stopPropagation();
        onOpen();
      }}
    >
      <Icon name="calendar" size={look === 'notch' ? 13 : 12} />
      {look === 'pill' && <span className="ca-meeting-title">{meeting.title.trim() || 'A meeting'}</span>}
      <span className="ca-meeting-when">{look === 'pill' ? meetingWhen(meeting, now) : meetingCountdown(meeting, now)}</span>
    </button>
  );
}

/** The minutes to go until the meeting counted down to, on its own: what the main window's
 *  Meetings button says beside its calendar. */
export function MeetingCountdown({ meeting }: { meeting: Meeting }) {
  const now = useNow(5000);
  return <>{meetingCountdown(meeting, now)}</>;
}

export function MeetingOffered({ offer, look }: { offer: MeetingOffer; look: 'card' | 'line' }) {
  // When it starts is said to the minute, so the clock is looked at more often than that.
  const now = useNow(5000);
  const [asked, setAsked] = useState(false);
  // What went wrong with taking it, said where it was asked: the offer stays, to try again.
  const [failed, setFailed] = useState<string | null>(null);
  // Its session is being started, as this window asked or another did.
  const taking = asked || !!offer.taking;
  const answer = (take: boolean) => (e: MouseEvent) => {
    // Not a click on what the offer is drawn over (the notch, which a click opens).
    e.stopPropagation();
    setFailed(null);
    if (take) setAsked(true);
    answerMeeting(offer.id, take)
      .catch((err: unknown) => setFailed(errorText(err)))
      .finally(() => setAsked(false));
    // Nothing to do with the session it started: the board shows it as it comes.
  };
  const title = offer.title.trim() || 'A meeting';
  const when = meetingWhen(offer, now);
  const who = meetingWith(offer);
  if (look === 'line') {
    return (
      <span className={cx('ca-meeting is-line', failed && 'is-failed')} role="status" title={failed ?? meetingWords(offer, now)}>
        <Icon name="calendar" size={13} />
        <span className="ca-meeting-title">{title}</span>
        <span className="ca-meeting-when">{failed ?? when}</span>
        <button type="button" className="ca-meeting-take" disabled={taking} title="Start a session that listens to it and keeps its notes" onClick={answer(true)}>
          {taking ? 'Starting…' : 'Take notes'}
        </button>
        <button type="button" className="ca-meeting-pass" disabled={taking} aria-label="Not this one" title="Not this one: it isn’t offered again" onClick={answer(false)}>
          <Icon name="x" size={12} />
        </button>
      </span>
    );
  }
  return (
    <div className="ca-meeting is-card" role="status">
      <span className="ca-meeting-head">
        <Icon name="calendar" size={13} />
        <span className="ca-meeting-title">{title}</span>
        <span className="ca-meeting-when">{when}</span>
      </span>
      {who && <span className="ca-meeting-who">{who}</span>}
      {failed && <span className="ca-meeting-failed">{failed}</span>}
      <span className="ca-meeting-acts">
        <Button size="sm" variant="primary" disabled={taking} title="Start a session that listens to it and keeps its notes" onClick={answer(true)}>
          {taking ? 'Starting…' : 'Take notes'}
        </Button>
        <Button size="sm" variant="ghost" disabled={taking} title="It isn’t offered again" onClick={answer(false)}>
          Not this one
        </Button>
      </span>
    </div>
  );
}
