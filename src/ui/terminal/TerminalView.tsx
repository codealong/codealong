// One terminal of Codealong's own, drawn and typed into (controller.ts does the work): a canvas
// for the screen and, over it, a text field nobody sees that holds the keyboard, so an input
// method composes in it and a paste lands in it with the clipboard's text. It takes the keyboard
// when its drawer asks (`focusAt`), as a terminal just opened does. Its right-click menu copies,
// and for a terminal beside a session, sends the selection to the session or adds it, or all its
// text, to the session's box (terminals.ts `sendToTheSession`, `addToBox`).

import { useEffect, useRef, useState } from 'react';
import { sentWorthSaying } from '../../ahead';
import { showContextMenu } from '../../contextMenu';
import { errorText } from '../../report';
import { addToBox, sendToTheSession } from '../../terminals';
import type { Session } from '../../types';
import { Button, cx } from '../primitives';
import { TerminalController } from './controller';

export interface TerminalViewProps {
  id: string;
  /** When its drawer last asked for the keyboard to come here. */
  focusAt: number;
  /** ⌃` in the terminal: the keyboard goes back. */
  onLeave: () => void;
  /** The session its text can be sent to or added to the box of: the one it's beside, not one it
   *  hosts. */
  session?: Session;
}

/** How long the note of where its text went stays. */
const NOTE_MS = 4000;

export function TerminalView({ id, focusAt, onLeave, session }: TerminalViewProps) {
  const wrap = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const field = useRef<HTMLTextAreaElement>(null);
  const control = useRef<TerminalController | null>(null);
  const leave = useRef(onLeave);
  leave.current = onLeave;
  const [unsafe, setUnsafe] = useState<string | null>(null);
  const [cursor, setCursor] = useState('text');
  const [failed, setFailed] = useState<string | null>(null);
  const [note, setNote] = useState<{ text: string; error: boolean } | null>(null);
  const said = (text: string, error = false) => setNote({ text, error });
  useEffect(() => {
    if (!note) return;
    const t = window.setTimeout(() => setNote(null), NOTE_MS);
    return () => window.clearTimeout(t);
  }, [note]);

  useEffect(() => {
    if (!wrap.current || !canvas.current) return;
    const c = new TerminalController(id, wrap.current, canvas.current, {
      onLeave: () => leave.current(),
      onUnsafePaste: setUnsafe,
      onError: setFailed,
      onPointer: setCursor,
    });
    control.current = c;
    c.start();
    return () => {
      c.stop();
      control.current = null;
    };
  }, [id]);

  useEffect(() => {
    if (focusAt) field.current?.focus({ preventScroll: true });
  }, [focusAt, id]);

  const lines = unsafe ? unsafe.split('\n').length : 0;
  return (
    <div className="ca-term" onMouseDown={(e) => {
      // A press anywhere in it gives it the keyboard; the field keeps it.
      if ((e.target as Element).closest('button')) return;
      e.preventDefault();
      field.current?.focus({ preventScroll: true });
    }}>
      <div ref={wrap} className="ca-term-screen" style={{ cursor }}>
        <canvas
          ref={canvas}
          className="ca-term-canvas"
          onPointerDown={(e) => {
            e.currentTarget.setPointerCapture(e.pointerId);
            control.current?.pointer(e.nativeEvent, 'down');
          }}
          onPointerUp={(e) => control.current?.pointer(e.nativeEvent, 'up')}
          onPointerCancel={() => control.current?.letGo()}
          onLostPointerCapture={() => control.current?.letGo()}
          onPointerMove={(e) => control.current?.pointer(e.nativeEvent, 'move')}
          onWheel={(e) => control.current?.wheel(e.nativeEvent)}
          onContextMenu={(e) => {
            e.preventDefault();
            const c = control.current;
            // A program that takes the mouse gets the right-click itself (`pointer`).
            if (!c || c.takesRightClick(e.nativeEvent)) return;
            const at = { x: e.clientX, y: e.clientY };
            // The menu acts on what was selected as you asked for it, read before it shows, so
            // nothing done meanwhile changes what it copies or sends, and what needs a selection
            // is dimmed without one.
            c.readForMenu((selected) => {
              // What it reads goes into the box's text as the session's draft.
              const toBox = (key: string, text: string, all: boolean) =>
                void addToBox(key, text).then(
                  (added) => added && said(all ? 'Added all its text to the session’s box.' : 'Added the selection to the session’s box.'),
                  (e: unknown) => said(errorText(e), true),
                );
              // Sent, it goes from here as a message from the box would; one that didn't go is
              // in the session's box, with what you were writing there. The terminal's keys don't
              // wait for it: what's typed meanwhile goes on to the terminal.
              const toSession = (s: Session, text: string) =>
                void sendToTheSession(s, text).then(
                  (sent) => said(!sent ? 'Nothing’s selected to send.' : sentWorthSaying(sent) ? sent.message : 'Sent the selection to the session.'),
                  (e: unknown) => said(errorText(e), true),
                );
              const some = !!selected;
              // Not returned: the terminal's input doesn't wait for the menu to close.
              void showContextMenu(
                [
                  { label: 'Copy', enabled: some, run: () => selected && void navigator.clipboard.writeText(selected).catch((e: unknown) => said(errorText(e), true)) },
                  { label: 'Select all', run: () => c.selectAll() },
                  'separator',
                  { label: 'Copy all text', run: () => c.copyAll() },
                  ...(session
                    ? ([
                        'separator',
                        { label: 'Send to the session', enabled: some, run: () => selected && toSession(session, selected) },
                        { label: 'Add to the session’s box', enabled: some, run: () => selected && toBox(session.key, selected, false) },
                        { label: 'Add all text to the session’s box', run: () => c.read(true, (all) => all && toBox(session.key, all, true)) },
                      ] as const)
                    : []),
                ],
                at,
              ).catch(() => {
                // A menu that didn't show leaves nothing undone: the terminal is as it was.
              });
            });
          }}
        />
        <textarea
          ref={field}
          className="ca-term-field"
          aria-label="Terminal"
          autoCapitalize="off"
          autoCorrect="off"
          autoComplete="off"
          spellCheck={false}
          onKeyDown={(e) => {
            if (control.current?.key(e.nativeEvent, true)) {
              e.preventDefault();
              e.stopPropagation();
            }
          }}
          onKeyUp={(e) => {
            if (control.current?.key(e.nativeEvent, false)) e.preventDefault();
          }}
          onCompositionEnd={(e) => {
            control.current?.typed(e.data);
            e.currentTarget.value = '';
          }}
          onInput={(e) => {
            // A dead key's character, or anything else typed that wasn't a key the terminal took:
            // not while an input method is still composing.
            const native = e.nativeEvent as InputEvent;
            if (native.isComposing) return;
            const text = e.currentTarget.value;
            e.currentTarget.value = '';
            if (text) control.current?.typed(text);
          }}
          onPaste={(e) => {
            e.preventDefault();
            control.current?.paste(e.clipboardData.getData('text/plain'));
          }}
          onFocus={() => control.current?.focus(true)}
          onBlur={() => control.current?.focus(false)}
        />
      </div>
      {unsafe !== null && (
        <div className="ca-term-ask" role="alertdialog" aria-label="Paste into the terminal?">
          <span>
            Paste {lines} {lines === 1 ? 'line' : 'lines'}? Each line break could run what's before it.
          </span>
          <Button size="sm" variant="ghost" onClick={() => setUnsafe(null)}>
            Cancel
          </Button>
          <Button
            size="sm"
            onClick={() => {
              const text = unsafe;
              setUnsafe(null);
              control.current?.paste(text, true);
              field.current?.focus({ preventScroll: true });
            }}
          >
            Paste
          </Button>
        </div>
      )}
      {failed && (
        <p className="ca-term-failed" role="status">
          {failed}
        </p>
      )}
      {note && !failed && (
        <p className={cx('ca-term-added', note.error && 'is-error')} role="status">
          {note.text}
        </p>
      )}
    </div>
  );
}
