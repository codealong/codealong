// The toasts at the foot of the main window: what went wrong with something you did here (a
// click puts it away), what's still under way once it takes a moment (opens, starts, a start
// the quick panel made for this window), and the way back from an archive or a todo change.
// One shows at a time, in that order. What goes wrong in the background is a notice instead
// (notices.ts).

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { onQuickStart } from '../api';
import type { Report } from '../report';
import { Icon, StatusGlyph } from './primitives';

/** A change that can be put back, and how. */
export interface UndoToast {
  message: string;
  icon?: string;
  undo: () => void;
}

/** A session just archived, for its Undo. */
export interface Archived {
  key: string;
  title: string;
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
}

/** The main window's toasts. A toast goes after six seconds, a way back after eight. */
export function useToasts(): ToastState {
  const [toast, setToast] = useState<string | null>(null);
  // Opens and starts still waiting on their app: a spinner shows once one takes a moment.
  const [busy, setBusy] = useState<{ id: number; label: string }[]>([]);
  const busySeq = useRef(0);
  const [archived, setArchived] = useState<Archived | null>(null);
  const [todoUndo, setTodoUndo] = useState<UndoToast | null>(null);

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
    if (!toast) return;
    const id = window.setTimeout(() => setToast(null), 6000);
    return () => window.clearTimeout(id);
  }, [toast]);
  useEffect(() => {
    if (!archived) return;
    const id = window.setTimeout(() => setArchived(null), 8000);
    return () => window.clearTimeout(id);
  }, [archived]);
  useEffect(() => {
    if (!todoUndo) return;
    const id = window.setTimeout(() => setTodoUndo(null), 8000);
    return () => window.clearTimeout(id);
  }, [todoUndo]);

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

  return { toast, setToast, busy, track, report, archived, setArchived, todoUndo, setTodoUndo };
}

/** The toast that shows now. `onUnarchive` puts an archived session back. */
export function Toasts({ state, onUnarchive }: { state: ToastState; onUnarchive: (archived: Archived) => void }) {
  const { toast, setToast, busy, archived: undo, setArchived: setUndo, todoUndo, setTodoUndo } = state;
  return (
    <>
      {busy.length > 0 && !toast && (
        <div className="ca-toast is-busy" role="status" aria-live="polite">
          <StatusGlyph status="working" size={14} title="In progress" />
          <span>{busy[busy.length - 1].label}</span>
        </div>
      )}
      {undo && !toast && busy.length === 0 && (
        <div className="ca-toast" role="status">
          <Icon name="archive" size={16} />
          <span>Archived “{undo.title}”.</span>
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
      {todoUndo && !toast && !undo && busy.length === 0 && (
        <div className="ca-toast" role="status">
          <Icon name={todoUndo.icon ?? 'trash'} size={16} />
          <span>{todoUndo.message}</span>
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
      {toast && (
        <div className="ca-toast" role="status" onClick={() => setToast(null)}>
          <Icon name="alert" size={16} />
          <span>{toast}</span>
        </div>
      )}
    </>
  );
}
