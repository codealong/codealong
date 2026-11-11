// Your meetings (src-tauri/src/meetings.rs): the ones in the day ahead, as your calendar has
// them (`ahead`), and the ones Codealong took, the latest first (`kept`: the folders among your
// meetings that say which meeting they are), the one picked with its notes and its transcript
// as its files have them. A dialog in the main window, opened from its bar. A meeting that's
// going on or about to start can be taken from here, before or after its offer; the folders are
// the record of the rest, so this lists what's there now.

import { useEffect, useState } from 'react';
import { answerMeeting, keptMeetings, meetingFiles, meetingsAhead, revealFile } from '../api';
import { heardLine } from '../listening';
import { meetingTime, meetingWith } from '../meetings';
import { useLatest } from '../latest';
import { errorText } from '../report';
import type { KeptMeeting, MeetingAhead, MeetingFiles } from '../types';
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

export function MeetingsDialog({ onClose, onShowSession }: { onClose: () => void; onShowSession: (key: string) => boolean }) {
  const [kept, setKept] = useState<KeptMeeting[] | null>(null);
  const [ahead, setAhead] = useState<MeetingAhead[]>([]);
  const [failed, setFailed] = useState<string | null>(null);
  const [picked, setPicked] = useState<string | null>(null);
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
    keptMeetings().then(
      (all) => {
        if (over) return;
        setKept(all);
        setPicked((p) => p ?? all[0]?.folder ?? null);
      },
      (e: unknown) => !over && setFailed(errorText(e)),
    );
    // What's ahead, and what can be done with each, as the engine has it now and every few
    // seconds while the dialog is open: a meeting comes to be takeable, or is taken elsewhere.
    // None while meetings aren't offered, or from an engine that doesn't list them.
    const look = () =>
      void meetingsAhead().then(
        (all) => {
          if (over) return;
          setAhead(all);
          setNow(Date.now());
        },
        () => {},
      );
    look();
    const again = window.setInterval(look, AHEAD_EVERY_MS);
    return () => {
      over = true;
      window.clearInterval(again);
    };
  }, []);
  useEffect(() => {
    if (!picked) return;
    let over = false;
    meetingFiles(picked).then(
      (got) => !over && setFiles({ of: picked, files: got }),
      (e: unknown) => !over && setFailed(errorText(e)),
    );
    return () => {
      over = true;
    };
  }, [picked]);
  const shown = kept?.find((m) => m.folder === picked) ?? null;
  const read = files && files.of === picked ? files.files : null;
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
      lede="The meetings in the day ahead, and the ones whose notes Codealong took, the latest first. Each of those is a folder of plain files, yours to read, move or share."
      onClose={onClose}
      busy={taking !== null}
      className="ca-meetingsdlg"
      aside={failed ? <span className="ca-meetings-failed">{failed}</span> : undefined}
      foot={
        <Button variant="primary" size="sm" onClick={onClose} disabled={taking !== null}>
          Done
        </Button>
      }
    >
      <div className="ca-dialog-body">
        {none ? (
          <p className="ca-meetings-none">
            No meetings yet. With Offer to take a meeting’s notes as it starts on (Settings › Experimental), Codealong asks as one in your calendar is about to start, the day’s meetings are listed here, and the ones you take are kept here.
          </p>
        ) : (
          <div className="ca-meetings">
            <div className="ca-meetings-list">
              {ahead.length > 0 && (
                <>
                  <h3 className="ca-meetings-group">Coming up</h3>
                  <ul aria-label="Coming up">
                    {ahead.map((m) => (
                      <li key={m.id} className="ca-meetings-ahead">
                        <span className="ca-meetings-title">{m.title.trim() || 'A meeting'}</span>
                        <span className="ca-meetings-when">{meetingTime(m, now)}</span>
                        <span className="ca-meetings-who">{[meetingWith(m), m.heard ? 'being heard now' : m.answered && !m.takeable ? '' : m.answered ? 'you passed on it' : ''].filter(Boolean).join(' · ')}</span>
                        {m.takeable && (
                          <Button size="sm" variant={m.answered ? 'secondary' : 'primary'} disabled={taking !== null} title="Start a session that listens to it and keeps its notes" onClick={() => take(m)}>
                            {taking === m.id ? 'Starting…' : 'Take notes'}
                          </Button>
                        )}
                      </li>
                    ))}
                  </ul>
                </>
              )}
              {(kept ?? []).length > 0 && (
                <>
                  <h3 className="ca-meetings-group">Taken</h3>
                  <ul aria-label="Taken">
                    {(kept ?? []).map((m) => (
                      <li key={m.folder}>
                        <button type="button" className={cx('ca-meetings-row', m.folder === picked && 'is-on')} aria-current={m.folder === picked || undefined} onClick={() => setPicked(m.folder)}>
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
            {shown ? (
              <section className="ca-meetings-shown" aria-label={shown.title}>
                <header className="ca-meetings-head">
                  <Seg label="What to read" look="text" options={PARTS} value={part} onPick={setPart} />
                  <span className="ca-meetings-acts">
                    <Button variant="ghost" size="sm" icon="message" title="Show the session that took its notes" onClick={show}>
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
              <p className="ca-meetings-none">None taken yet: the notes and transcript of the ones you take show here.</p>
            )}
          </div>
        )}
      </div>
    </Dialog>
  );
}
