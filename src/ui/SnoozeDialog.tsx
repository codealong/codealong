// Snoozes sessions, or your review of PRs, until a time you pick: Pick a time… at the end of a
// Snooze menu in any window (snooze.ts `timesMenu`) brings the main window forward with this. A
// day and a time, said back as the menus say when a snooze ends. Every window shows the snooze at
// once (ahead.ts); one the engine didn't take goes back, and the window's toast says why.

import { useState } from 'react';
import { snoozeAhead, snoozeReviewsAhead } from '../ahead';
import { errorText } from '../report';
import { nextMorning, untilWords, type SnoozeAsk } from '../snooze';
import { Button, Dialog } from './primitives';

interface SnoozeDialogProps {
  ask: SnoozeAsk;
  /** What it snoozes, as `askWords` says it. */
  what: string;
  onClose: () => void;
  /** Why the engine didn't take the snooze. */
  onFailed: (why: string) => void;
}

const pad = (n: number) => String(n).padStart(2, '0');

/** A time as the date and time fields write it, on this Mac's clock: `2026-10-28` and `09:00`. */
export function snoozeFields(at: Date): { day: string; time: string } {
  return { day: `${at.getFullYear()}-${pad(at.getMonth() + 1)}-${pad(at.getDate())}`, time: `${pad(at.getHours())}:${pad(at.getMinutes())}` };
}

/** The time the fields name, or null while either is empty or half typed. */
export function fieldsTime(day: string, time: string): number | null {
  const d = /^(\d{4})-(\d{2})-(\d{2})$/.exec(day);
  const t = /^(\d{2}):(\d{2})$/.exec(time);
  if (!d || !t) return null;
  return new Date(Number(d[1]), Number(d[2]) - 1, Number(d[3]), Number(t[1]), Number(t[2])).getTime();
}

export function SnoozeDialog({ ask, what, onClose, onFailed }: SnoozeDialogProps) {
  // It opens on the next morning, the menus' time for tomorrow, to change the day or the hour of.
  const [{ day, time }, setFields] = useState(() => snoozeFields(nextMorning(Date.now())));
  // Drawn again when a time picked has passed by the time it's sent, to say so.
  const [, redraw] = useState(0);
  const now = Date.now();
  const until = fieldsTime(day, time);
  const ahead = until != null && until > now;
  const save = () => {
    // Checked as it's sent, not as it was drawn: a time gone by would end the snooze at once,
    // and any it had before.
    if (until == null || until <= Date.now()) {
      redraw((n) => n + 1);
      return;
    }
    // Picked, for the window it was asked from to hear (ahead.ts `snoozedFrom`).
    (ask.reviews ? snoozeReviewsAhead : snoozeAhead)(ask.keys, until, true).catch((e: unknown) => onFailed(errorText(e)));
    onClose();
  };

  return (
    <Dialog
      className="ca-snoozedlg"
      title={`Snooze ${what}`}
      lede={ask.reviews ? 'It waits under Snoozed until then, out of what waits on your review.' : 'It stays off what waits on you until then, on every device.'}
      onClose={onClose}
      onSubmit={save}
      aside={
        <span className="ca-snoozedlg-when" role="status">
          {until == null ? 'Pick a day and a time.' : ahead ? `Until ${untilWords(until, now)}` : 'That time has passed.'}
        </span>
      }
      foot={
        <>
          <Button variant="ghost" size="sm" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" size="sm" type="submit" disabled={!ahead}>
            Snooze
          </Button>
        </>
      }
    >
      <div className="ca-dialog-body ca-snoozedlg-fields">
        <label className="ca-field">
          <span>Day</span>
          <input type="date" value={day} autoFocus onChange={(e) => setFields({ day: e.target.value, time })} />
        </label>
        <label className="ca-field">
          <span>Time</span>
          <input type="time" value={time} onChange={(e) => setFields({ day, time: e.target.value })} />
        </label>
      </div>
    </Dialog>
  );
}
