// Your meetings (src-tauri/src/meetings.rs `kept`): the folders among your meetings that say
// which meeting they are, the latest first, and the one picked with its notes and its
// transcript as its files have them. A dialog in the main window, opened from its bar. The
// folders are the record, so this lists what's there now: nothing is kept about them elsewhere.

import { useEffect, useState } from 'react';
import { keptMeetings, meetingFiles, revealFile } from '../api';
import { heardLine } from '../listening';
import { meetingWith } from '../meetings';
import { errorText } from '../report';
import type { KeptMeeting, MeetingFiles } from '../types';
import { HeardLines } from './Listening';
import { Markdown } from './Markdown';
import { Button, cx, Dialog, Seg } from './primitives';

/** When a meeting was, as a list says it: the day and the time it started. */
function when(start: number): string {
  return new Date(start).toLocaleString([], { weekday: 'short', month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' });
}

type Part = 'notes' | 'transcript';
const PARTS: { id: Part; label: string }[] = [
  { id: 'notes', label: 'Notes' },
  { id: 'transcript', label: 'Transcript' },
];

export function MeetingsDialog({ onClose, onShowSession }: { onClose: () => void; onShowSession: (key: string) => boolean }) {
  const [all, setAll] = useState<KeptMeeting[] | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  const [picked, setPicked] = useState<string | null>(null);
  const [files, setFiles] = useState<{ of: string; files: MeetingFiles } | null>(null);
  const [part, setPart] = useState<Part>('notes');
  useEffect(() => {
    let over = false;
    keptMeetings().then(
      (kept) => {
        if (over) return;
        setAll(kept);
        setPicked((p) => p ?? kept[0]?.folder ?? null);
      },
      (e: unknown) => !over && setFailed(errorText(e)),
    );
    return () => {
      over = true;
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
  const shown = all?.find((m) => m.folder === picked) ?? null;
  const read = files && files.of === picked ? files.files : null;
  const text = read ? (part === 'notes' ? read.notes : read.transcript) : null;
  const show = () => {
    // Its session, where the board still has it: the dialog gives way to it.
    if (shown && onShowSession(shown.session)) onClose();
    else setFailed('Its session is no longer on the board. Its notes and transcript are here.');
  };
  return (
    <Dialog
      title="Meetings"
      lede="The meetings whose notes Codealong took, the latest first. Each is a folder of plain files, yours to read, move or share."
      onClose={onClose}
      className="ca-meetingsdlg"
      aside={failed ? <span className="ca-meetings-failed">{failed}</span> : undefined}
      foot={
        <Button variant="primary" size="sm" onClick={onClose}>
          Done
        </Button>
      }
    >
      <div className="ca-dialog-body">
        {all && all.length === 0 ? (
        <p className="ca-meetings-none">No meetings yet. With Offer to take a meeting’s notes as it starts on (Settings › Experimental), Codealong asks as one in your calendar is about to start, and the ones you take are kept here.</p>
      ) : (
        <div className="ca-meetings">
          <ul className="ca-meetings-list" aria-label="Meetings">
            {(all ?? []).map((m) => (
              <li key={m.folder}>
                <button type="button" className={cx('ca-meetings-row', m.folder === picked && 'is-on')} aria-current={m.folder === picked || undefined} onClick={() => setPicked(m.folder)}>
                  <span className="ca-meetings-title">{m.title.trim() || 'A meeting'}</span>
                  <span className="ca-meetings-when">{when(m.start)}</span>
                  <span className="ca-meetings-who">{[meetingWith(m), m.lines === 0 ? 'nothing heard' : m.notes ? '' : 'no notes'].filter(Boolean).join(' · ')}</span>
                </button>
              </li>
            ))}
          </ul>
          {shown && (
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
          )}
        </div>
        )}
      </div>
    </Dialog>
  );
}
