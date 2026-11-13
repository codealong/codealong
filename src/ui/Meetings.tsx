// Your meetings (src-tauri/src/meetings.rs): the ones in the day ahead, as your calendar has
// them (`ahead`), and the ones Codealong heard, the latest first (`yours`: the folders that say
// which meeting they are, a taken meeting's and one a session was asked to listen to alike).
// A dialog in the main window, opened from its bar. The one picked shows on the right: one
// ahead with when it is, who's in it and its call, and the listening to start for it, whatever
// its time (Take notes as its offer has it, or Listen… to pick where its session starts and
// what it makes of what it hears); one heard with its notes and its transcript as its files have
// them. The folders are the record of those, so this lists what's there now.

import { useEffect, useState } from 'react';
import { answerMeeting, keptMeetings, meetingFiles, meetingsAhead, openUrl, revealFile } from '../api';
import { heardLine } from '../listening';
import { firstPick, meetingStands, meetingTime, meetingWith, startWords, type MeetingPick } from '../meetings';
import { useLatest } from '../latest';
import { errorText } from '../report';
import type { KeptMeeting, MeetingAhead, MeetingFiles } from '../types';
import { CopyButton } from './CopyButton';
import { HeardLines } from './Listening';
import { Markdown } from './Markdown';
import { Button, cx, Dialog, Seg } from './primitives';

/** When a meeting was, as a list says it: the day and the time it started. */
function when(start: number): string {
  return new Date(start).toLocaleString([], { weekday: 'short', month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' });
}

/** How often what's ahead is read again while the dialog is open, and how long a session just
 *  started for a meeting has to reach this window's board before the dialog closes without it. */
const AHEAD_EVERY_MS = 10_000;
const SHOWS_WITHIN_MS = 8000;

type Part = 'notes' | 'transcript';
const PARTS: { id: Part; label: string }[] = [
  { id: 'notes', label: 'Notes' },
  { id: 'transcript', label: 'Transcript' },
];

/** `onListen`: Listen now in the quick panel, for the meeting ahead given or one you name. */
export function MeetingsDialog({ onClose, onShowSession, onListen }: { onClose: () => void; onShowSession: (key: string) => boolean; onListen: (meeting?: string) => void }) {
  const [kept, setKept] = useState<KeptMeeting[] | null>(null);
  const [ahead, setAhead] = useState<MeetingAhead[]>([]);
  const [failed, setFailed] = useState<string | null>(null);
  const [picked, setPicked] = useState<MeetingPick | null>(null);
  const [files, setFiles] = useState<{ of: string; files: MeetingFiles } | null>(null);
  const [part, setPart] = useState<Part>('notes');
  // The meeting being taken from here: its session is starting.
  const [taking, setTaking] = useState<string | null>(null);
  const [now, setNow] = useState(() => Date.now());
  // Read after the render that asked: a take answers many seconds on, when the board the
  // window had then no longer says where the new session is.
  const showSession = useLatest(onShowSession);
  const close = useLatest(onClose);
  useEffect(() => {
    let over = false;
    // What's ahead, and what can be done with each, as the engine has it now and every few
    // seconds while the dialog is open: a meeting comes to be takeable, or is taken elsewhere.
    // None while meetings aren't offered, or from an engine that doesn't list them.
    const look = () =>
      meetingsAhead().then(
        (all) => {
          if (!over) {
            setAhead(all);
            setNow(Date.now());
          }
          return all;
        },
        () => [] as MeetingAhead[],
      );
    // The first look with your meetings, so the dialog opens on the one going on when there is
    // one, and picks once: a meeting that comes on later doesn't take the place of the one read.
    void Promise.all([
      keptMeetings().catch((e: unknown) => {
        if (!over) setFailed(errorText(e));
        return null;
      }),
      look(),
    ]).then(([all, soon]) => {
      if (over) return;
      if (all) setKept(all);
      setPicked((p) => p ?? firstPick(all ?? [], soon));
    });
    const again = window.setInterval(() => void look(), AHEAD_EVERY_MS);
    return () => {
      over = true;
      window.clearInterval(again);
    };
  }, []);
  const folder = picked && 'kept' in picked ? picked.kept : null;
  useEffect(() => {
    if (!folder) return;
    let over = false;
    meetingFiles(folder).then(
      (got) => !over && setFiles({ of: folder, files: got }),
      (e: unknown) => !over && setFailed(errorText(e)),
    );
    return () => {
      over = true;
    };
  }, [folder]);
  const shown = folder ? (kept?.find((m) => m.folder === folder) ?? null) : null;
  const soon = picked && 'ahead' in picked ? (ahead.find((m) => m.id === picked.ahead) ?? null) : null;
  const read = files && files.of === folder ? files.files : null;
  const text = read ? (part === 'notes' ? read.notes : read.transcript) : null;
  const show = () => {
    // Its session, where the board still has it: the dialog gives way to it.
    if (shown && onShowSession(shown.session)) onClose();
    else setFailed('Its session is no longer on the board. Its notes and transcript are here.');
  };
  const take = (m: MeetingAhead) => {
    setFailed(null);
    setTaking(m.id);
    answerMeeting(m.id, true).then(
      (listening) => {
        // The session that takes its notes: the dialog gives way to it once the window's board
        // has it, which follows the engine's by a moment, and gives way anyway if it never does.
        const key = listening?.session;
        const until = Date.now() + SHOWS_WITHIN_MS;
        const show = () => {
          if (!key || showSession.current(key) || Date.now() > until) close.current();
          else window.setTimeout(show, 250);
        };
        show();
      },
      (e: unknown) => {
        setTaking(null);
        setFailed(errorText(e));
      },
    );
  };
  const none = kept !== null && kept.length === 0 && ahead.length === 0;
  return (
    <Dialog
      title="Meetings"
      lede="The meetings in the day ahead, and the ones Codealong heard, the latest first. Each of those is a folder of plain files, yours to read, move or share."
      onClose={onClose}
      busy={taking !== null}
      className="ca-meetingsdlg"
      aside={failed ? <span className="ca-meetings-failed">{failed}</span> : undefined}
      foot={
        <>
          {/* A meeting that isn't in your calendar, or that you'd name yourself: Listen now asks
              what it is in the quick panel (meetings.rs `now`). */}
          <Button variant="ghost" size="sm" icon="microphone" title="Have Codealong hear a meeting from the moment you say, in your calendar or not, with a session started for it" onClick={() => onListen()} disabled={taking !== null}>
            Listen now…
          </Button>
          <Button variant="primary" size="sm" onClick={onClose} disabled={taking !== null}>
            Done
          </Button>
        </>
      }
    >
      <div className="ca-dialog-body">
        {none ? (
          <p className="ca-meetings-none">
            No meetings yet. Listen now has Codealong hear one from the moment you say, and it’s kept here; so is one a session listens to (Listen… in its menu). With Offer to take a meeting’s notes as it starts on (Settings › Experimental), Codealong asks as one in your calendar is about to start, and the day’s meetings are listed here too.
          </p>
        ) : (
          <div className="ca-meetings">
            <div className="ca-meetings-list">
              {ahead.length > 0 && (
                <>
                  <h3 className="ca-meetings-group">Coming up</h3>
                  <ul aria-label="Coming up">
                    {ahead.map((m) => (
                      <li key={m.id}>
                        <button type="button" className={cx('ca-meetings-row', soon?.id === m.id && 'is-on')} aria-current={soon?.id === m.id || undefined} onClick={() => setPicked({ ahead: m.id })}>
                          <span className="ca-meetings-title">{m.title.trim() || 'A meeting'}</span>
                          <span className="ca-meetings-when">{meetingTime(m, now)}</span>
                          <span className="ca-meetings-who">{[meetingWith(m), m.heard ? 'being heard now' : m.answered && !m.takeable ? '' : m.answered ? 'you passed on it' : ''].filter(Boolean).join(' · ')}</span>
                        </button>
                      </li>
                    ))}
                  </ul>
                </>
              )}
              {(kept ?? []).length > 0 && (
                <>
                  <h3 className="ca-meetings-group">Heard</h3>
                  <ul aria-label="Heard">
                    {(kept ?? []).map((m) => (
                      <li key={m.folder}>
                        <button type="button" className={cx('ca-meetings-row', m.folder === folder && 'is-on')} aria-current={m.folder === folder || undefined} onClick={() => setPicked({ kept: m.folder })}>
                          <span className="ca-meetings-title">{m.title.trim() || 'A meeting'}</span>
                          <span className="ca-meetings-when">{when(m.start)}</span>
                          <span className="ca-meetings-who">{[meetingWith(m), m.lines === 0 ? 'nothing heard' : m.notes ? '' : 'no notes'].filter(Boolean).join(' · ')}</span>
                        </button>
                      </li>
                    ))}
                  </ul>
                </>
              )}
            </div>
            {soon ? (
              <AheadShown
                m={soon}
                now={now}
                heardIn={soon.heard ? (kept?.find((k) => k.id === soon.id) ?? null) : null}
                taking={taking}
                onTake={() => take(soon)}
                onListen={() => onListen(soon.id)}
                onNotes={(f) => setPicked({ kept: f })}
                onFailed={setFailed}
              />
            ) : shown ? (
              <section className="ca-meetings-shown" aria-label={shown.title}>
                <header className="ca-meetings-head">
                  <Seg label="What to read" look="text" options={PARTS} value={part} onPick={setPart} />
                  <span className="ca-meetings-acts">
                    {/* What's shown, as its file has it: the notes' Markdown, or the transcript's
                        lines. A new one for each, so one doesn't say the other was copied. */}
                    <CopyButton
                      key={`${shown.folder}:${part}`}
                      label={part === 'notes' ? 'Copy notes' : 'Copy transcript'}
                      title={part === 'notes' ? 'Copy its notes, as their file has them' : 'Copy its transcript, a line for each thing said'}
                      text={text ?? ''}
                      disabled={!text}
                    />
                    <Button variant="ghost" size="sm" icon="message" title="Show the session that listened to it" onClick={show}>
                      Its session
                    </Button>
                    <Button variant="ghost" size="sm" icon="folder" title={shown.folder} onClick={() => void revealFile(`${shown.folder}/meeting.json`).catch((e: unknown) => setFailed(errorText(e)))}>
                      Show in Finder
                    </Button>
                  </span>
                </header>
                <div className="ca-meetings-body">
                  {read === null ? null : text === null ? (
                    <p className="ca-meetings-none">{part === 'notes' ? 'No notes were kept of it.' : 'Nothing was heard.'}</p>
                  ) : part === 'notes' ? (
                    <Markdown text={text} />
                  ) : (
                    <HeardLines lines={text.split('\n').filter((l) => l.trim() !== '').map(heardLine)} />
                  )}
                </div>
              </section>
            ) : (
              <p className="ca-meetings-none">{picked ? 'That meeting is over: once it’s heard, its notes and transcript show here.' : 'None heard yet: the notes and transcript of each meeting Codealong hears show here.'}</p>
            )}
          </div>
        )}
      </div>
    </Dialog>
  );
}

/** A meeting in the day ahead, picked: where it stands, when it is, who's in it and its call,
 *  and the listening to start for it, whatever its time (`startWords` says what that does):
 *  Take notes, as its offer takes it, or Listen… to pick where its session starts and what it
 *  makes of what it hears. One being heard has its notes so far among the ones heard
 *  (`heardIn`). */
function AheadShown({ m, now, heardIn, taking, onTake, onListen, onNotes, onFailed }: { m: MeetingAhead; now: number; heardIn: KeptMeeting | null; taking: string | null; onTake: () => void; onListen: () => void; onNotes: (folder: string) => void; onFailed: (why: string) => void }) {
  const title = m.title.trim() || 'A meeting';
  const words = startWords(m);
  const link = m.link;
  return (
    <section className="ca-meetings-shown" aria-label={title}>
      <header className="ca-meetings-head">
        <span className="ca-meetings-stands">{meetingStands(m, now)}</span>
        <span className="ca-meetings-acts">
          {link && (
            <Button variant="ghost" size="sm" icon="external" title={link} onClick={() => void openUrl(link).catch((e: unknown) => onFailed(errorText(e)))}>
              Join call
            </Button>
          )}
          {heardIn && (
            <Button variant="ghost" size="sm" icon="notes" title="Its notes and transcript so far" onClick={() => onNotes(heardIn.folder)}>
              Its notes
            </Button>
          )}
        </span>
      </header>
      <div className="ca-meetings-body">
        <h3 className="ca-meetings-name">{title}</h3>
        <dl className="ca-meetings-facts">
          <dt>When</dt>
          <dd>{meetingTime(m, now)}</dd>
          {m.calendar.trim() && (
            <>
              <dt>Calendar</dt>
              <dd>{m.calendar}</dd>
            </>
          )}
          {m.attendees.length > 0 && (
            <>
              <dt>With</dt>
              <dd>{m.attendees.join(', ')}</dd>
            </>
          )}
        </dl>
        {words && (
          <div className="ca-meetings-start">
            <p>{words}</p>
            <span className="ca-meetings-acts">
              <Button variant={m.takeable ? 'primary' : 'secondary'} size="sm" icon="microphone" disabled={taking !== null} title="Start a session that listens to it and keeps its notes, as a chat of its own" onClick={onTake}>
                {taking === m.id ? 'Starting…' : 'Take notes'}
              </Button>
              <Button variant="ghost" size="sm" disabled={taking !== null} title="Pick where its session starts, what it makes of what it hears and what it hears, in the quick panel" onClick={onListen}>
                Listen…
              </Button>
            </span>
          </div>
        )}
      </div>
    </section>
  );
}
