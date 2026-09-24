// A todo's description: text with screenshots in it. It reads as text and images; click it to
// edit the text, where images are `![image](images/<name>)` lines. Paste an image, drop one
// from the Finder, or pick a file, and it goes in at the cursor. Changes save as you type.

import { Fragment, useEffect, useRef, useState, type ReactNode } from 'react';
import { imageUrl, openImage, openUrl } from '../api';
import { keepImages, pastedImages, useImageDrop } from '../imageInput';
import { imageNames, insertImage, pieces, withoutImage } from '../todos';
import { Icon } from './primitives';

const URL_RE = /(https?:\/\/[^\s<>()]+[^\s<>().,;:!?'"])/g;

/** Text with its web links clickable; they open in the browser. */
function Linked({ text }: { text: string }) {
  const parts = text.split(URL_RE);
  return (
    <>
      {parts.map((p, i) =>
        i % 2 ? (
          <a
            key={i}
            href={p}
            onClick={(e) => {
              e.preventDefault();
              e.stopPropagation();
              void openUrl(p);
            }}
          >
            {p}
          </a>
        ) : (
          <Fragment key={i}>{p}</Fragment>
        ),
      )}
    </>
  );
}

/** Thumbnails of the images a text holds. Click one to open it; × takes it out of the text. */
export function ImageStrip({ names, onRemove }: { names: string[]; onRemove?: (name: string) => void }) {
  if (names.length === 0) return null;
  return (
    <div className="ca-imgstrip">
      {names.map((n) => (
        <span key={n} className="ca-imgthumb">
          <button type="button" className="ca-imgthumb-open" title="Open in Preview" onMouseDown={(e) => e.preventDefault()} onClick={() => void openImage(n)}>
            <img src={imageUrl(n)} alt="" draggable={false} />
          </button>
          {onRemove && (
            <button type="button" className="ca-imgthumb-x" aria-label="Remove image" title="Remove image" onMouseDown={(e) => e.preventDefault()} onClick={() => onRemove(n)}>
              <Icon name="x" size={10} stroke={2.25} />
            </button>
          )}
        </span>
      ))}
    </div>
  );
}

/** A description as it reads: paragraphs, links and images. */
export function DescriptionView({ text }: { text: string }) {
  return (
    <>
      {pieces(text).map((p, i): ReactNode => {
        if (p.kind === 'image') {
          return (
            <button
              key={i}
              type="button"
              className="ca-desc-img"
              title="Open in Preview"
              onClick={(e) => {
                e.stopPropagation();
                void openImage(p.name);
              }}
            >
              <img src={imageUrl(p.name)} alt={p.alt} draggable={false} />
            </button>
          );
        }
        const t = p.text.replace(/^\n+|\n+$/g, '');
        return t ? (
          <p key={i} className="ca-desc-text">
            <Linked text={t} />
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
  // Where images go: the cursor when you last left it, else the end.
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

  // Images go in at once and save at once, editing or not.
  const addImages = (names: string[]) => {
    let text = draftRef.current;
    let at = cursor.current ?? text.length;
    for (const n of names) {
      const r = insertImage(text, Math.min(at, text.length), n);
      text = r.text;
      at = r.cursor;
    }
    cursor.current = at;
    setDraft(text);
    onSave(text);
    if (editing) {
      window.requestAnimationFrame(() => areaRef.current?.setSelectionRange(at, at));
    }
  };
  const addBlobs = (blobs: Blob[]) => {
    keepImages(blobs)
      .then(addImages)
      .catch((e: unknown) => onError(String(e).replace(/^Error: /, '')));
  };
  useImageDrop(boxRef, addImages, onError);

  const names = imageNames(editing ? draft : value);
  return (
    <div ref={boxRef} className={['ca-desc', editing && 'is-editing'].filter(Boolean).join(' ')}>
      {editing ? (
        <>
          <textarea
            ref={areaRef}
            className="ca-desc-area"
            value={draft}
            rows={4}
            spellCheck
            placeholder="What should the agent know? Paste or drop screenshots here."
            onChange={(e) => {
              setDraft(e.target.value);
              cursor.current = e.target.selectionStart;
            }}
            onSelect={(e) => (cursor.current = e.currentTarget.selectionStart)}
            onPaste={(e) => {
              const images = pastedImages(e);
              if (images.length === 0) return;
              e.preventDefault();
              cursor.current = e.currentTarget.selectionStart;
              addBlobs(images);
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
          <ImageStrip
            names={names}
            onRemove={(n) => {
              const text = withoutImage(draftRef.current, n);
              setDraft(text);
              onSave(text);
            }}
          />
        </>
      ) : (
        <div
          className={['ca-desc-view', !value.trim() && 'is-empty'].filter(Boolean).join(' ')}
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
          {value.trim() ? <DescriptionView text={value} /> : <span>Add a description. Paste or drop screenshots here.</span>}
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
          <Icon name="photo" size={13} />
          Add image
        </button>
        {editing && <small>Esc or ⌘⏎ when you’re done</small>}
        <input
          ref={fileRef}
          type="file"
          accept="image/png,image/jpeg,image/gif,image/webp"
          multiple
          hidden
          onChange={(e) => {
            const files = [...(e.target.files ?? [])];
            e.target.value = '';
            if (files.length > 0) addBlobs(files);
          }}
        />
      </div>
    </div>
  );
}
