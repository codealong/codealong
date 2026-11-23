// An inline local image written in Markdown (`![alt](/abs/path.png)` or `![alt](file:///…)`),
// served through `codealong-file://` (sentfiles.rs) with the same privacy and existence checks as
// SentFiles.tsx: macOS-protected folders wait for a click, missing or remote images say so cleanly,
// a click opens the image in Preview, and ResizeObserver keeps the conversation scrolled in place.

import { useContext, useLayoutEffect, useRef, useState } from 'react';
import { sentFileUrl } from '../api';
import { useSentState } from '../filePaths';
import { useLatest } from '../latest';
import { ImagesFrom } from './DescriptionEditor';
import { FileWhy, useFileActions } from './fileActions';
import { Icon } from './primitives';
import { ThreadGrows } from './SentFiles';

function fileName(path: string): string {
  const cut = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
  return path.slice(cut + 1) || path;
}

export function LocalImage({ path, alt }: { path: string; alt: string }) {
  const away = useContext(ImagesFrom);
  const state = useSentState(away ? null : path);
  const [asked, setAsked] = useState(false);
  const [failed, setFailed] = useState(false);
  const f = useFileActions({ path, line: null }, 'file');

  const root = useRef<HTMLDivElement>(null);
  const grows = useLatest(useContext(ThreadGrows));
  useLayoutEffect(() => {
    const el = root.current;
    if (!el) return;
    let height = el.offsetHeight;
    const watch = new ResizeObserver(() => {
      const now = el.offsetHeight;
      if (now !== height) grows.current?.(el, now - height);
      height = now;
    });
    watch.observe(el);
    return () => watch.disconnect();
  }, [grows]);

  const label = alt.trim() || fileName(path);

  if (away) {
    return (
      <span className="ca-desc-noimg" title={`${path}, on the session’s machine`}>
        <Icon name="photo" size={13} />
        <span>{label} · on the session’s machine</span>
      </span>
    );
  }

  if (state === null) return null;

  if (state === 'gone' || failed) {
    return (
      <span className="ca-desc-noimg" title={path}>
        <Icon name="photo" size={13} />
        <span>{label} · not on this machine</span>
      </span>
    );
  }

  if (state === 'asksFirst' && !asked) {
    return (
      <div className="ca-md-img" ref={root}>
        <div className="ca-md-img-ask">
          <button
            type="button"
            className="ca-btn ca-btn--secondary ca-btn--sm"
            title="macOS may ask you to let Codealong read this folder"
            onClick={(e) => {
              e.stopPropagation();
              setAsked(true);
            }}
          >
            <Icon name="photo" size={13} />
            <span>Show image</span>
          </button>
          <span className="ca-md-img-cap">{label}</span>
        </div>
      </div>
    );
  }

  return (
    <div className="ca-md-img" ref={root}>
      <figure className="ca-md-img-fig">
        <button
          type="button"
          className="ca-desc-img"
          title={`Open ${path}`}
          onClick={(e) => {
            e.stopPropagation();
            f.open();
          }}
          onContextMenu={f.menu}
        >
          <img src={sentFileUrl(path, 0, asked)} alt={alt} draggable={false} onError={() => setFailed(true)} />
        </button>
        {alt.trim() && <figcaption className="ca-md-img-cap">{alt.trim()}</figcaption>}
      </figure>
      <FileWhy why={f.why} />
    </div>
  );
}
