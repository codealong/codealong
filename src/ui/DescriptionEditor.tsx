// A todo's description: text with attachments in it. It reads as text, with images shown in
// place and other files as chips; click it to edit the text, where an image is an
// `![image](images/…)` line and a file a `[name](files/…)` line. Paste files or a screenshot, drop
// them from the Finder, or pick them with Attach, and they go in at the cursor. Changes save as
// you type.

import { Fragment, useEffect, useRef, useState, type ReactNode } from 'react';
import { imageUrl, openAttachment, openUrl } from '../api';
import { filesToAttach, keepFiles, useFileDrop } from '../attachInput';
import { attachmentsIn, inlines, insertAttachments, pieces, withoutAttachment } from '../todos';
import { prLabel, prRefOf } from '../prLinks';
import type { Attachment } from '../types';
import { cx, Icon } from './primitives';
import { PrLink } from './PrLink';
import { errorText } from '../report';

const open = (path: string) => void openAttachment(path);

/** A kept file in text: its name, and a click opens it. */
function FileChip({ name, path, onRemove }: { name: string; path: string; onRemove?: () => void }) {
  return (
    <span className="ca-filechip">
      <button
        type="button"
        className="ca-filechip-open"
        title={`Open ${name}`}
        onMouseDown={(e) => e.preventDefault()}
        onClick={(e) => {
          e.stopPropagation();
          open(path);
        }}
      >
        <Icon name="paperclip" size={12} />
        <span>{name}</span>
      </button>
      {onRemove && (
        <button type="button" className="ca-filechip-x" aria-label={`Remove ${name}`} title="Remove" onMouseDown={(e) => e.preventDefault()} onClick={onRemove}>
          <Icon name="x" size={10} stroke={2.25} />
        </button>
      )}
    </span>
  );
}

/** A web link in text, which opens in the browser. A PR's reads as GitHub shortens it, and
 *  hovering it shows the PR (PrLink.tsx). */
function TextLink({ url }: { url: string }) {
  const pr = prRefOf(url);
  if (pr) {
    return (
      <PrLink url={url} pr={pr}>
        {prLabel(pr)}
      </PrLink>
    );
  }
  return (
    <a
      href={url}
      onClick={(e) => {
        e.preventDefault();
        e.stopPropagation();
        void openUrl(url);
      }}
    >
      {url}
    </a>
  );
}

/** Text with its web links and kept files clickable: links open in the browser, files where
 *  they open. */
function InlineText({ text }: { text: string }) {
  return (
    <>
      {inlines(text).map((part, i) =>
        part.kind === 'url' ? (
          <TextLink key={i} url={part.url} />
        ) : part.kind === 'file' ? (
          <FileChip key={i} name={part.name} path={part.path} />
        ) : (
          <Fragment key={i}>{part.text}</Fragment>
        ),
      )}
    </>
  );
}

/** What a text has attached: images as thumbnails, other files as chips. Click one to open it;
 *  × takes it out of the text. */
export function AttachmentStrip({ items, onRemove }: { items: Attachment[]; onRemove?: (a: Attachment) => void }) {
  if (items.length === 0) return null;
  return (
    <div className="ca-imgstrip">
      {items.map((a) =>
        a.kind === 'image' ? (
          <span key={a.path} className="ca-imgthumb">
            <button type="button" className="ca-imgthumb-open" title="Open in Preview" onMouseDown={(e) => e.preventDefault()} onClick={() => open(a.path)}>
              <img src={imageUrl(a.path.replace(/^images\//, ''))} alt="" draggable={false} />
            </button>
            {onRemove && (
              <button type="button" className="ca-imgthumb-x" aria-label="Remove image" title="Remove" onMouseDown={(e) => e.preventDefault()} onClick={() => onRemove(a)}>
                <Icon name="x" size={10} stroke={2.25} />
              </button>
            )}
          </span>
        ) : (
          <FileChip key={a.path} name={a.name} path={a.path} onRemove={onRemove ? () => onRemove(a) : undefined} />
        ),
      )}
    </div>
  );
}

/** A kept image in text: a click opens it. One that isn't on this Mac (it was sent from another
 *  device, or cleared since) says so in place of a broken picture. */
function KeptImage({ name, alt }: { name: string; alt: string }) {
  const [missing, setMissing] = useState(false);
  if (missing) {
    return (
      <span className="ca-desc-noimg">
        <Icon name="photo" size={13} />
        Image not on this Mac
      </span>
    );
  }
  return (
    <button
      type="button"
      className="ca-desc-img"
      title="Open in Preview"
      onClick={(e) => {
        e.stopPropagation();
        open(`images/${name}`);
      }}
    >
      <img src={imageUrl(name)} alt={alt} draggable={false} onError={() => setMissing(true)} />
    </button>
  );
}

/** A description as it reads: paragraphs with their links and files, and images. */
export function DescriptionView({ text }: { text: string }) {
  return (
    <>
      {pieces(text).map((p, i): ReactNode => {
        if (p.kind === 'image') return <KeptImage key={`${i}:${p.name}`} name={p.name} alt={p.alt} />;
        const t = p.text.replace(/^\n+|\n+$/g, '');
        return t ? (
          <p key={i} className="ca-desc-text">
            <InlineText text={t} />
          </p>
        ) : null;
      })}
    </>
  );
}

interface DescriptionEditorProps {
  /** The saved text. */
  value: string;
  onSave: (text: string) => void;
  onError: (message: string) => void;
}

export function DescriptionEditor({ value, onSave, onError }: DescriptionEditorProps) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(value);
  const areaRef = useRef<HTMLTextAreaElement>(null);
  const boxRef = useRef<HTMLDivElement>(null);
  const fileRef = useRef<HTMLInputElement>(null);
  // Where attachments go: the cursor when you last left it, else the end.
  const cursor = useRef<number | null>(null);
  const draftRef = useRef(draft);
  draftRef.current = draft;
  // The latest save, so a new board (and with it a new callback) doesn't restart the wait.
  const save = useRef(onSave);
  save.current = onSave;
  const latest = useRef({ editing, draft, value });
  latest.current = { editing, draft, value };
  // Selecting another todo swaps the panel before the text area sees a blur: save what's typed.
  useEffect(
    () => () => {
      const l = latest.current;
      if (l.editing && l.draft !== l.value) save.current(l.draft);
    },
    [],
  );

  // While you're not editing, the text follows the saved one (another window may change it).
  useEffect(() => {
    if (!editing) setDraft(value);
  }, [value, editing]);

  // Saves a moment after you stop typing, and when you leave.
  useEffect(() => {
    if (!editing || draft === value) return;
    const id = window.setTimeout(() => save.current(draft), 700);
    return () => window.clearTimeout(id);
  }, [draft, editing, value]);

  useEffect(() => {
    const el = areaRef.current;
    if (!el) return;
    el.style.height = 'auto';
    el.style.height = `${el.scrollHeight + 2}px`;
  }, [draft, editing]);

  const startEditing = () => {
    setEditing(true);
    window.requestAnimationFrame(() => {
      const el = areaRef.current;
      if (!el) return;
      el.focus();
      const end = el.value.length;
      el.setSelectionRange(end, end);
    });
  };

  const finish = () => {
    if (draftRef.current !== value) onSave(draftRef.current);
    setEditing(false);
  };

  // Attachments go in at once and save at once, editing or not.
  const addAttachments = (kept: Attachment[]) => {
    const current = draftRef.current;
    const r = insertAttachments(current, cursor.current ?? current.length, kept);
    cursor.current = r.cursor;
    setDraft(r.text);
    onSave(r.text);
    if (editing) {
      window.requestAnimationFrame(() => areaRef.current?.setSelectionRange(r.cursor, r.cursor));
    }
  };
  const addFiles = (files: File[]) => {
    keepFiles(files)
      .then(addAttachments)
      .catch((e: unknown) => onError(errorText(e)));
  };
  useFileDrop(boxRef, addAttachments, onError);

  const attached = attachmentsIn(editing ? draft : value);
  return (
    <div ref={boxRef} className={cx('ca-desc', editing && 'is-editing')}>
      {editing ? (
        <>
          <textarea
            ref={areaRef}
            className="ca-desc-area"
            value={draft}
            rows={4}
            spellCheck
            placeholder="What should the agent know? Paste or drop screenshots and files here."
            onChange={(e) => {
              setDraft(e.target.value);
              cursor.current = e.target.selectionStart;
            }}
            onSelect={(e) => (cursor.current = e.currentTarget.selectionStart)}
            onPaste={(e) => {
              const files = filesToAttach(e);
              if (files.length === 0) return;
              e.preventDefault();
              cursor.current = e.currentTarget.selectionStart;
              addFiles(files);
            }}
            onBlur={finish}
            onKeyDown={(e) => {
              if (e.key === 'Escape' || (e.key === 'Enter' && e.metaKey)) {
                e.preventDefault();
                e.stopPropagation();
                areaRef.current?.blur();
              }
            }}
          />
          <AttachmentStrip
            items={attached}
            onRemove={(a) => {
              const text = withoutAttachment(draftRef.current, a.path);
              setDraft(text);
              onSave(text);
            }}
          />
        </>
      ) : (
        <div
          className={cx('ca-desc-view', !value.trim() && 'is-empty')}
          role="button"
          tabIndex={0}
          title="Click to edit"
          onClick={startEditing}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault();
              e.stopPropagation();
              startEditing();
            }
          }}
        >
          {value.trim() ? <DescriptionView text={value} /> : <span>Add a description. Paste or drop screenshots and files here.</span>}
        </div>
      )}
      <div className="ca-desc-foot">
        <button
          type="button"
          className="ca-linkbtn"
          onMouseDown={(e) => e.preventDefault()}
          onClick={() => {
            if (editing && areaRef.current) cursor.current = areaRef.current.selectionStart;
            fileRef.current?.click();
          }}
        >
          <Icon name="paperclip" size={13} />
          Attach
        </button>
        {editing && <small>Esc or ⌘⏎ when you’re done</small>}
        <input
          ref={fileRef}
          type="file"
          multiple
          hidden
          onChange={(e) => {
            const files = [...(e.target.files ?? [])];
            e.target.value = '';
            if (files.length > 0) addFiles(files);
          }}
        />
      </div>
    </div>
  );
}
