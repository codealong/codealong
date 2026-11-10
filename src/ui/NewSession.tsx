// A new session waiting for its first prompt (newSession.ts), in the main window's Sessions panel
// in place of the selected row's: drawn as a session's panel is, with a header that says it's new
// and where it goes, what sending does where its conversation will be, and a box at the foot
// with the cursor in it. ⏎ sends and ⇧⏎ starts a new line, as in a session's box; files paste
// and drop in as they do there. What's written is kept per worktree (drafts.ts) until it's sent,
// so closing the panel loses nothing. Once sent it says it's starting until the window shows the
// session in its place (App.tsx), or says why the start failed, with what you wrote still there.

import { useEffect, useRef, useState, type ClipboardEvent } from 'react';
import { filesToAttach, keepFiles, useFileDrop } from '../attachInput';
import { readDraft, writeDraft } from '../drafts';
import { tildePath } from '../format';
import { useLatest } from '../latest';
import { newSessionDraft, type NewSession } from '../newSession';
import { sessionSkills, writtenFor } from '../skills';
import { useSkills } from '../useSkills';
import { keysIn } from '../platform';
import { errorText } from '../report';
import { mergeAttachments, splitAttachments, withAttachments } from '../todos';
import type { Attachment, Lane } from '../types';
import { AttachmentStrip } from './DescriptionEditor';
import { BoxText, Button, cx, Icon, OneLine } from './primitives';
import { ProjectPrefix } from './ProjectSaid';
import { BoxPlace } from './SessionAgent';
import { placeIcon } from './StartControls';

/** The most lines the box grows to, as a session's box does. */
const LINES = 8;

export interface NewSessionPanelProps {
  draft: NewSession;
  /** The worktree it starts in, when the board lists it. */
  lane: Lane | null;
  /** What was sent, as written, while its start runs; why the last start failed. The window
   *  keeps both (newSession.ts `NewSessionShown`): the panel goes while another view shows. */
  sending: string | null;
  error: string | null;
  /** Sent: the start went through and the window waits for the board to list the session. */
  started: boolean;
  /** Starts it on `prompt`, `written` being what was typed; a failure rejects. */
  onSend: (prompt: string, written: string) => Promise<void>;
  onClose: () => void;
}

export function NewSessionPanel({ draft, lane, sending, error, started, onSend, onClose }: NewSessionPanelProps) {
  const { spec } = draft;
  const kept = newSessionDraft(spec);
  // Shown again while it's on its way, from another view, as it was sent.
  const [initial] = useState(() => splitAttachments(sending ?? readDraft(kept)));
  const [text, setText] = useState(initial.text);
  const [attached, setAttached] = useState<Attachment[]>(initial.attachments);
  const [trouble, setTrouble] = useState<string | null>(null);
  const box = useRef<HTMLTextAreaElement>(null);
  const field = useRef<HTMLDivElement>(null);
  const busy = sending !== null || started;

  // It's opened to be written in.
  useEffect(() => box.current?.focus(), []);

  const keep = (t: string, a: Attachment[]) => writeDraft(kept, withAttachments(t, a));
  // Files are kept before they go in, while you may go on typing: they join what's there then.
  const written = useLatest({ text, attached });
  const attach = (added: Attachment[]) => {
    const next = mergeAttachments(written.current.attached, added);
    setAttached(next);
    keep(written.current.text, next);
  };
  useFileDrop(field, attach, setTrouble, !busy);
  const paste = (e: ClipboardEvent<HTMLTextAreaElement>) => {
    const files = filesToAttach(e);
    if (files.length === 0) return;
    e.preventDefault();
    keepFiles(files)
      .then(attach)
      .catch((err: unknown) => setTrouble(errorText(err)));
  };

  // A skill's name goes as the account's agent takes it, as from every box (skills.ts).
  const listed = useSkills(lane && !lane.device ? [lane.path] : [], [spec.account], text.includes('/'));
  const harness = draft.harness;
  const prompt = withAttachments(text, attached);
  const empty = !prompt.trim();
  const send = () => {
    if (empty || busy) return;
    setTrouble(null);
    // It's on its way, so the next new session here starts empty. A start that fails puts it back,
    // unless something newer was written here meanwhile; the window says why.
    writeDraft(kept, '');
    onSend(harness ? withAttachments(writtenFor(text, sessionSkills(listed, harness), harness), attached) : prompt, prompt).catch(() => {
      if (!readDraft(kept)) writeDraft(kept, prompt);
      requestAnimationFrame(() => box.current?.focus());
    });
  };

  const said = error ?? trouble;
  const note = said ?? (busy ? `Starting ${spec.account}…` : null);
  const where = spec.place === 'codealong' ? 'Codealong' : 'the background';
  return (
    <aside className="ca-insp ca-sessinsp ca-newsess" aria-label="New session">
      <header className="ca-sessinsp-head">
        <div className="ca-sessinsp-top">
          <h3 className="ca-sessinsp-title">New session</h3>
          <div className="ca-sessinsp-tools">
            <Button variant="ghost" size="sm" icon="x" aria-label="Close" title="Close: nothing was started, and what you wrote is kept for the next new session here" onClick={onClose} disabled={busy} />
          </div>
        </div>
        <div className="ca-sessinsp-meta">
          <span>{busy ? 'Starting' : 'Not started'}</span>
          <span aria-hidden="true">·</span>
          {lane && (
            <span className="ca-sessinsp-lane is-plain" title={tildePath(lane.path)}>
              <Icon name="folder" size={12} />
              <ProjectPrefix project={lane.project} beside={[lane.callsign]} />
              <span>{lane.callsign}</span>
            </span>
          )}
          {lane && <span aria-hidden="true">·</span>}
          <span>{spec.account}</span>
        </div>
      </header>
      <div className="ca-thread">
        <p className="ca-empty">{draft.does} Nothing starts until you send its first prompt.</p>
      </div>
      <div className="ca-composer">
        <div
          ref={field}
          className="ca-composer-field"
          onMouseDown={(e) => {
            // A press anywhere in the box but its buttons puts the cursor in it.
            if ((e.target as Element).closest('button, textarea')) return;
            e.preventDefault();
            box.current?.focus();
          }}
        >
          {attached.length > 0 && (
            <AttachmentStrip
              items={attached}
              onRemove={
                busy
                  ? undefined
                  : (a) => {
                      const next = attached.filter((x) => x.path !== a.path);
                      setAttached(next);
                      keep(text, next);
                    }
              }
            />
          )}
          <BoxText
            ref={box}
            value={text}
            lines={LINES}
            hint={`First prompt for ${spec.account}`}
            readOnly={busy}
            aria-label="First prompt for the new session"
            onChange={(e) => {
              setText(e.target.value);
              keep(e.target.value, attached);
              if (trouble) setTrouble(null);
            }}
            onKeyDown={(e) => {
              if (e.key !== 'Enter' || e.shiftKey || e.nativeEvent.isComposing) return;
              e.preventDefault();
              send();
            }}
            onPaste={paste}
          />
          <div className="ca-composer-footrow">
            <div className="ca-composer-tools ca-composer-who">
              <BoxPlace icon={placeIcon(spec.place)} label={draft.placeLabel} title={`Not started: sending starts it in ${where}`} />
            </div>
            {note && <OneLine className={cx('ca-composer-note', !!said && 'is-error')} role="status" text={note} />}
            <div className="ca-composer-buttons">
              <button type="button" className="ca-composer-send" aria-label="Send" title={keysIn('Start it on this prompt (⏎)')} disabled={empty || busy} onClick={send}>
                <Icon name="arrow-up" size={16} stroke={2} />
              </button>
            </div>
          </div>
        </div>
      </div>
    </aside>
  );
}
