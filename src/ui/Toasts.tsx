// The main window's toasts: what went wrong with something you did here (a click puts it away),
// what's still under way once it takes a moment (opens, starts, a start the quick panel made for
// this window), and the way back from an archive or a todo change, yours or a session an agent
// marked done. One shows at a time, in that order. What goes wrong in the background is a notice
// instead (notices.ts).
// A toast is a line in the usage bar (UsageBar.tsx `toast`), beside the notices: floating over the
// window's foot, it covered the list's last rows, and before that the session panel's message box.
// Its words fit one line there, all of them under it on hover (primitives.tsx `OneLine`), and it
// stays while the pointer is on it.

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { onAgentDone, onQuickStart } from '../api';
import type { Report } from '../report';
import { Icon, OneLine, StatusGlyph } from './primitives';

/** A change that can be put back, and how. */
export interface UndoToast {
  message: string;
  icon?: string;
  undo: () => void;
}

/** Sessions just archived, for their Undo, which takes them all out of the archive. */
export interface Archived {
  keys: string[];
  /** One's title, which the toast says without `message`. */
  title: string;
  /** What was done besides, when it was marked done: its terminal closed, say. */
  message?: string;
}

export interface ToastState {
  toast: string | null;
  /** Says something went wrong, or how something went; null takes it away. */
  setToast: (message: string | null) => void;
  busy: { id: number; label: string }[];
  /** Shows `label` while `work` takes more than a moment, and gives `work` back. */
  track: <T>(label: string, work: Promise<T>) => Promise<T>;
  /** How the menus say how things went (menus.ts): a toast, and the spinner while something
   *  takes a moment. */
  report: Report;
  archived: Archived | null;
  setArchived: (archived: Archived | null) => void;
  todoUndo: UndoToast | null;
  setTodoUndo: (undo: UndoToast | null) => void;
  /** Which shows, of those up: what went wrong first, then what's under way, then a way back. */
  shown: 'toast' | 'busy' | 'archived' | 'todo' | null;
  /** The pointer is on the toast, which stays until it leaves. */
  setHeld: (held: boolean) => void;
}

/** The main window's toasts. A toast goes after six seconds, a way back after eight, counted
 *  again from when the pointer leaves it. */
export function useToasts(): ToastState {
  const [toast, setToast] = useState<string | null>(null);
  // Opens and starts still waiting on their app: a spinner shows once one takes a moment.
  const [busy, setBusy] = useState<{ id: number; label: string }[]>([]);
  const busySeq = useRef(0);
  const [archived, setArchived] = useState<Archived | null>(null);
  const [todoUndo, setTodoUndo] = useState<UndoToast | null>(null);
  const [held, setHeld] = useState(false);

  const track = useCallback(<T,>(label: string, work: Promise<T>): Promise<T> => {
    const id = ++busySeq.current;
    const timer = window.setTimeout(() => setBusy((b) => [...b, { id, label }]), 300);
    const done = () => {
      window.clearTimeout(timer);
      setBusy((b) => b.filter((x) => x.id !== id));
    };
    work.then(done, done);
    return work;
  }, []);
  const report = useMemo<Report>(() => ({ error: setToast, info: setToast, busy: track }), [track]);

  useEffect(() => {
    if (!toast || held) return;
    const id = window.setTimeout(() => setToast(null), 6000);
    return () => window.clearTimeout(id);
  }, [toast, held]);
  useEffect(() => {
    if (!archived || held) return;
    const id = window.setTimeout(() => setArchived(null), 8000);
    return () => window.clearTimeout(id);
  }, [archived, held]);
  useEffect(() => {
    if (!todoUndo || held) return;
    const id = window.setTimeout(() => setTodoUndo(null), 8000);
    return () => window.clearTimeout(id);
  }, [todoUndo, held]);
  const shown = toast ? 'toast' : busy.length > 0 ? 'busy' : archived ? 'archived' : todoUndo ? 'todo' : null;
  // A toast that takes the place of the one the pointer was on, or comes back once another goes,
  // isn't held: the one before went without saying the pointer left it.
  const showing = toast ?? (shown === 'busy' ? 'busy' : (archived ?? todoUndo));
  useEffect(() => setHeld(false), [showing]);

  // Starts this window asked the quick panel for. A spinner shows while the harness takes the
  // session; then what's next. The new session isn't selected when it shows up: by then you may
  // be reading another, and the panel stays on what you picked. A failed start brings the panel
  // back with what you wrote and why.
  useEffect(() => {
    const timers = new Map<number, number>();
    const off = onQuickStart((e) => {
      if (e.phase === 'begin') {
        timers.set(e.id, window.setTimeout(() => setBusy((b) => [...b, { id: e.id, label: e.label }]), 300));
        return;
      }
      window.clearTimeout(timers.get(e.id));
      timers.delete(e.id);
      setBusy((b) => b.filter((x) => x.id !== e.id));
      if (e.message) setToast(e.message);
    });
    return () => {
      off();
      timers.forEach((t) => window.clearTimeout(t));
    };
  }, []);

  // A session an agent marked done from the command (agentdone.rs) leaves the lists with nothing
  // of yours doing it: the toast says which session did, with the way back.
  useEffect(() => onAgentDone((e) => setArchived({ keys: [e.key], title: e.title, message: e.message })), []);

  return { toast, setToast, busy, track, report, archived, setArchived, todoUndo, setTodoUndo, shown, setHeld };
}

/** The toast that shows now, as a line in the usage bar. `onUnarchive` puts archived sessions
 *  back. */
export function Toasts({ state, onUnarchive }: { state: ToastState; onUnarchive: (archived: Archived) => void }) {
  const { toast, setToast, busy, archived: undo, setArchived: setUndo, todoUndo, setTodoUndo, shown, setHeld } = state;
  // The pointer on a toast holds it, for its words to be read on hover.
  const hold = { onMouseEnter: () => setHeld(true), onMouseLeave: () => setHeld(false) };
  return (
    <>
      {shown === 'busy' && (
        <div className="ca-toast is-busy" role="status" aria-live="polite">
          <StatusGlyph status="working" size={14} title="In progress" />
          <Words text={busy[busy.length - 1].label} />
        </div>
      )}
      {shown === 'archived' && undo && (
        <div className="ca-toast" role="status" {...hold}>
          <Icon name="archive" size={14} />
          <Words text={undo.message || `Archived “${undo.title}”.`} />
          <button
            type="button"
            className="ca-toast-action"
            onClick={() => {
              onUnarchive(undo);
              setUndo(null);
            }}
          >
            Undo
          </button>
        </div>
      )}
      {shown === 'todo' && todoUndo && (
        <div className="ca-toast" role="status" {...hold}>
          <Icon name={todoUndo.icon ?? 'trash'} size={14} />
          <Words text={todoUndo.message} />
          <button
            type="button"
            className="ca-toast-action"
            onClick={() => {
              todoUndo.undo();
              setTodoUndo(null);
            }}
          >
            Undo
          </button>
        </div>
      )}
      {shown === 'toast' && toast && (
        <div className="ca-toast" role="status" {...hold} onClick={() => setToast(null)}>
          <Icon name="alert" size={14} />
          <Words text={toast} />
        </div>
      )}
    </>
  );
}

/** A toast's words, on one line: cut short, they're all there under it on hover. */
function Words({ text }: { text: string }) {
  return <OneLine className="ca-toast-text" down text={text} />;
}
