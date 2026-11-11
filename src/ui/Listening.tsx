// A session that listens (src-tauri/src/listen.rs), wherever it shows:
//
// - `Told`: what Codealong told it, in its conversation. What was heard is drawn as the meeting
//   (the time, who, the words), so the conversation reads as what was said and what its agent made
//   of it, in turn; that its listening began and ended are a line each.
// - `LiveStrip`: over its box while it hears: that it's listening and to what, the lines heard
//   that it hasn't been told of yet, the words being said right now, its notes, and Stop.
// - `ListeningChip`: on its row.
// - `ListenHost`: the dialog a session's Listen… opens, which the board's windows show; any other
//   window hands the asking to the main window, which has the room for it.

import { useEffect, useState } from 'react';
import { askMainToListen, hearingStatus, listeningOf, onHearing, onListenAsk, onListeningNotes, startListening, stopListening } from '../api';
import { ASK, askProblem, clock, heardLine, hears, macSays, onAskedToListen, ROLES, type HeardLine, type ListenAsked, type Told as ToldWhat } from '../listening';
import { errorText } from '../report';
import { below, showContextMenu } from '../contextMenu';
import { hearingNamed, hearingSays } from '../said';
import type { BarListening, HearingStatus, ListenAsk, ListeningGlance } from '../types';
import { Markdown } from './Markdown';
import { Button, CheckBox, Chip, cx, Dialog, Icon, Seg } from './primitives';
import { useNow } from './useNow';

/** One line of a meeting: when, who, the words. One with nobody's name is Codealong's own, about
 *  the transcript (its hearing stopped and started again). */
function Line({ line, live }: { line: HeardLine; live?: boolean }) {
  if (line.who === null) return <li className="ca-heard-aside">{line.text}</li>;
  return (
    <li className={cx('ca-heard-line', line.who === 'You' && 'is-you', live && 'is-live')}>
      <span className="ca-heard-at">{line.at.slice(0, 5)}</span>
      <span className="ca-heard-who">{line.who}</span>
      <span className="ca-heard-text">{line.text}</span>
    </li>
  );
}

/** A meeting's lines, as its conversation and its transcript both draw them. */
export function HeardLines({ lines }: { lines: HeardLine[] }) {
  return (
    <ol className="ca-heard-lines">
      {lines.map((l, i) => (
        <Line key={i} line={l} />
      ))}
    </ol>
  );
}

/** What Codealong told a listening session, in its conversation. */
export function Told({ told }: { told: ToldWhat }) {
  const [open, setOpen] = useState(false);
  if (told.kind === 'ended') return <div className="ca-thread-note">Listening ended. {told.why}</div>;
  if (told.kind === 'began') {
    return (
      <div className="ca-heard-began">
        <button type="button" className="ca-thread-note ca-heard-toggle" aria-expanded={open} onClick={() => setOpen(!open)}>
          <Icon name="microphone" size={11} stroke={2} />
          Listening began
          <Icon name={open ? 'chevron-down' : 'chevron-right'} size={11} />
        </button>
        {open && <p className="ca-heard-brief">{told.words}</p>}
      </div>
    );
  }
  return (
    <div className="ca-heard" data-heard={`${told.from}-${told.to}`}>
      {told.lines ? (
        <ol className="ca-heard-lines">
          {told.lines.map((l, i) => (
            <Line key={told.from + i} line={l} />
          ))}
        </ol>
      ) : (
        <p className="ca-heard-many">
          Lines {told.from} to {told.to} of the transcript: too many to show here.
        </p>
      )}
      {told.ended && <div className="ca-thread-note">Listening ended. {told.ended}</div>}
    </div>
  );
}

/** What a hearing listening has heard that its session hasn't been told of yet, kept up as it's
 *  heard, and its notes: read once as the strip shows, then from what the server sends. */
function useHeard(key: string, id: string) {
  const [lines, setLines] = useState<{ n: number; said: string }[]>([]);
  const [saying, setSaying] = useState<{ who: string; text: string } | null>(null);
  const [notes, setNotes] = useState<string | null>(null);
  useEffect(() => {
    let over = false;
    setLines([]);
    setSaying(null);
    setNotes(null);
    // What was told while the read was out, so it doesn't bring those lines back.
    let told = 0;
    const read = (all: boolean) =>
      void listeningOf(key).then(
        (tail) => {
          if (over || !tail || tail.listening.id !== id) return;
          setNotes(tail.notes ?? null);
          if (!all) return;
          const heard = tail.untold.map((said, i) => ({ n: tail.first + i, said })).filter((l) => l.n > Math.max(told, tail.listening.told));
          setLines((now) => [...heard.filter((l) => !now.some((have) => have.n === l.n)), ...now].sort((a, b) => a.n - b.n));
        },
        // The strip shows what comes from here on; the conversation has the rest.
        () => {},
      );
    read(true);
    const offHearing = onHearing((h) => {
      if (h.session !== key) return;
      if (h.told !== undefined) {
        const upTo = h.told;
        told = Math.max(told, upTo);
        setLines((now) => now.filter((l) => l.n > upTo));
      } else if (h.line !== undefined && h.said !== undefined) {
        const line = { n: h.line, said: h.said };
        setLines((now) => (now.some((l) => l.n === line.n) ? now : [...now, line]));
        setSaying(null);
      } else if (h.who !== undefined) {
        setSaying(h.text ? { who: h.who, text: h.text } : null);
      }
    });
    const offNotes = onListeningNotes((session) => {
      if (session === key) read(false);
    });
    return () => {
      over = true;
      offHearing();
      offNotes();
    };
  }, [key, id]);
  return { lines, saying, notes };
}

/** How many of the lines not yet told the strip shows: the latest, since the rest are on their
 *  way into the conversation above it. */
const SHOWN = 4;

/** Over a listening session's box while it hears. */
export function LiveStrip({ sessionKey, listening }: { sessionKey: string; listening: ListeningGlance }) {
  // How long it's been listening goes on by the second, as a recording's clock does: on a clock
  // of the strip's own, since the panel's is for words like "5 minutes ago" and ticks far slower.
  const now = useNow(1000);
  const { lines, saying, notes } = useHeard(sessionKey, listening.id);
  const [showNotes, setShowNotes] = useState(false);
  const [stopping, setStopping] = useState(false);
  // Why it didn't stop, said here, where Stop was pressed.
  const [error, setError] = useState<string | null>(null);
  const stop = () => {
    setStopping(true);
    setError(null);
    stopListening(sessionKey).catch((e: unknown) => {
      setStopping(false);
      setError(errorText(e));
    });
  };
  const shown = lines.slice(-SHOWN);
  return (
    <section className="ca-live" aria-label="Listening">
      <header className="ca-live-head">
        <span className="ca-live-dot" aria-hidden="true" />
        <span className="ca-live-title">Listening</span>
        <span className="ca-live-meta">
          {clock(now - listening.startedAt)} · {hears(listening)}
        </span>
        {notes !== null && (
          <Button variant="ghost" size="sm" icon="notes" aria-pressed={showNotes} onClick={() => setShowNotes(!showNotes)}>
            Notes
          </Button>
        )}
        <Button variant="secondary" size="sm" icon="player-stop" disabled={stopping} onClick={stop}>
          Stop
        </Button>
      </header>
      {(error ?? listening.problem) && (
        <p className="ca-live-problem" role="status">
          {error ?? listening.problem}
        </p>
      )}
      {listening.waiting && !listening.problem && <p className="ca-live-waiting">{listening.waiting}…</p>}
      {showNotes && notes !== null && (
        <div className="ca-live-notes">
          <Markdown text={notes} />
        </div>
      )}
      {(shown.length > 0 || saying) && (
        <ol className="ca-heard-lines ca-live-lines" aria-live="off">
          {lines.length > shown.length && <li className="ca-heard-aside">{lines.length - shown.length} more heard</li>}
          {shown.map((l) => (
            <Line key={l.n} line={heardLine(l.said)} />
          ))}
          {saying && <Line line={{ at: '', who: saying.who, text: saying.text }} live />}
        </ol>
      )}
    </section>
  );
}

/** A listening session's chip, on its row. */
export function ListeningChip({ listening }: { listening: ListeningGlance | undefined }) {
  if (!listening) return null;
  return (
    <Chip className={cx('ca-listeningchip', listening.problem && 'is-troubled')} title={listening.problem ?? `Listening: ${hears(listening)}. Its turns don’t wait on you while it does`}>
      <Icon name="microphone" size={12} />
      Listening
    </Chip>
  );
}

/** The sessions hearing now, in the main window's bar whatever it shows: a recording light with
 *  the session's name and how long it has listened, and a click shows the session. Nothing while
 *  none hears. On the bar that floats it's a pill (Bar.tsx `ListeningPill`). */
export function ListeningNow({ listenings, onShow }: { listenings: readonly BarListening[]; onShow: (l: BarListening) => void }) {
  return listenings.length ? <HearingNow listenings={listenings} onShow={onShow} /> : null;
}

/** A part of its own, so a clock runs by the second only while a session hears. The window's bar
 *  has room for one light: several sessions are counted in it, the clock the first's to begin,
 *  and a click drops a menu of them to pick from. */
function HearingNow({ listenings, onShow }: { listenings: readonly BarListening[]; onShow: (l: BarListening) => void }) {
  const now = useNow(1000);
  const [first] = listenings;
  const one = listenings.length === 1;
  const says = hearingSays(listenings) ?? '';
  return (
    <button
      type="button"
      className="ca-hearing-now"
      title={`${says}\n${one ? 'Show it' : 'Pick one to show'}`}
      aria-label={`${says.replace(/\n/g, '. ')}. ${one ? 'Show it' : 'Pick one to show'}`}
      onClick={(e) => (one ? onShow(first) : void showContextMenu(listenings.map((l) => ({ label: `${l.title} · ${clock(now - l.startedAt)}`, run: () => onShow(l) })), below(e.currentTarget)))}
    >
      <span className="ca-live-dot" aria-hidden="true" />
      <span className="ca-hearing-now-title">{hearingNamed(listenings)}</span>
      <span className="ca-hearing-now-clock">{clock(now - first.startedAt)}</span>
      {listenings.some((l) => l.problem) && <Icon name="alert" size={12} />}
    </button>
  );
}

/** The dialog that sets a session's listening up. */
function ListenDialog({ asked, onClose }: { asked: ListenAsked; onClose: () => void }) {
  const [ask, setAsk] = useState<ListenAsk>(ASK);
  const [hearing, setHearing] = useState<HearingStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let over = false;
    hearingStatus().then(
      (h) => !over && setHearing(h),
      (e: unknown) => !over && setError(errorText(e)),
    );
    return () => {
      over = true;
    };
  }, []);
  const set = (next: Partial<ListenAsk>) => setAsk((a) => ({ ...a, ...next }));
  const cannot = hearing?.status.why ?? null;
  const problem = cannot ?? askProblem(ask);
  const role = ROLES.find((r) => r.id === ask.role) ?? ROLES[0];
  const playing = (hearing?.apps ?? []).filter((a) => a.playing || a.recording);
  const start = () => {
    if (problem || busy) return;
    setBusy(true);
    setError(null);
    startListening(asked.key, ask).then(onClose, (e: unknown) => {
      setBusy(false);
      setError(errorText(e));
    });
  };
  return (
    <Dialog
      className="ca-listendlg"
      title="Listen"
      lede={
        <>
          <b>{asked.title}</b> hears a meeting as it’s said, turned into text on this Mac. No audio is kept.
        </>
      }
      busy={busy}
      onClose={onClose}
      onSubmit={start}
      aside={<span className="ca-listendlg-aside">{error ?? problem ?? macSays(hearing?.status, ask)}</span>}
      foot={
        <>
          <Button variant="ghost" size="sm" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <Button variant="primary" size="sm" type="submit" icon="microphone" disabled={!!problem || busy}>
            {busy ? 'Starting…' : 'Start listening'}
          </Button>
        </>
      }
    >
      <div className="ca-dialog-body">
        <div className="ca-listendlg-field">
          <span>What it does</span>
          <Seg label="What it does with what it hears" options={ROLES.map((r) => ({ id: r.id, label: r.label }))} value={ask.role} onPick={(id) => set({ role: id })} />
          <small>{role.says}</small>
        </div>
        {ask.role === 'requests' && (
          <label className="ca-listendlg-check">
            <CheckBox checked={ask.start} onChange={(on) => set({ start: on })} />
            <span>Start a session on each right away</span>
            <small>Each in a worktree of its own, as this session’s helper. Otherwise the todos wait for you to start the ones you want.</small>
          </label>
        )}
        <label className="ca-listendlg-field">
          <span>{ask.role === 'custom' ? 'What to do with it' : 'Anything to add'}</span>
          <textarea className="ca-listendlg-prompt" rows={ask.role === 'custom' ? 4 : 2} value={ask.prompt} placeholder={ask.role === 'coach' ? 'What you want out of this meeting' : ask.role === 'custom' ? 'Count how often anyone says “synergy”' : 'Who’s on the call, what it’s about'} onChange={(e) => set({ prompt: e.target.value })} />
        </label>
        <div className="ca-listendlg-field">
          <span>Hears</span>
          <label className="ca-listendlg-check">
            <CheckBox checked={ask.mic} onChange={(on) => set({ mic: on })} />
            <span>You, through the microphone</span>
          </label>
          <label className="ca-listendlg-check">
            <CheckBox checked={ask.system} onChange={(on) => set({ system: on, apps: on ? ask.apps : [] })} />
            <span>Everyone else, from what this Mac plays</span>
          </label>
          {ask.system && playing.length > 0 && (
            <div className="ca-listendlg-apps">
              <Seg
                label="Which audio"
                options={[{ id: '', label: 'Everything' }, ...playing.map((a) => ({ id: a.bundle, label: `Only ${a.name}` }))]}
                value={ask.apps[0] ?? ''}
                onPick={(bundle) => set({ apps: bundle ? [bundle] : [] })}
              />
            </div>
          )}
          {ask.mic && ask.system && (
            <label className="ca-listendlg-check">
              <CheckBox checked={ask.speakers} onChange={(on) => set({ speakers: on })} />
              <span>I’m on speakers, not headphones</span>
              <small>The microphone hears the others too: what the Mac plays is taken out of it, so they aren’t written down twice.</small>
            </label>
          )}
        </div>
      </div>
    </Dialog>
  );
}

/** Shows the dialog a session's Listen… asks for. Only a board window has the room for it (`here`):
 *  another window hands the asking to the main window and brings that forward on the session. */
export function ListenHost({ here }: { here: boolean }) {
  const [asked, setAsked] = useState<ListenAsked | null>(null);
  useEffect(() => {
    // Not said when it fails: the main window is there for as long as the app is, and one that's
    // going takes this window with it.
    if (!here) return onAskedToListen((a) => void askMainToListen(a).catch(() => {}));
    const offHere = onAskedToListen(setAsked);
    const offElsewhere = onListenAsk(setAsked);
    return () => {
      offHere();
      offElsewhere();
    };
  }, [here]);
  return asked ? <ListenDialog key={asked.key} asked={asked} onClose={() => setAsked(null)} /> : null;
}
