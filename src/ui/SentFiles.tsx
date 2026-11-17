// The files a call sent you (Claude Code's SendUserFile), under its line in a conversation
// (SessionThread.tsx): what the agent said of them, then each file. A picture, a film or a
// recording (sentFiles.ts `mediaKind`) shows in place over its name, side by side as they fit: a
// picture opens on a click as its name does, and a film or a recording plays where it is, with
// its controls, served by the app (sentfiles.rs). Any other file is its name and folder on a line
// of its own. Every name opens the file on a click as a double-click in the Finder would, and
// offers the file's menu on a right-click (Markdown.tsx `FileLink`).
//
// One in a folder macOS asks about before a read shows once you ask for it (sentFiles.ts
// `asksFirst`), so scrolling past it never brings up the prompt. One that can't be read (gone
// from a scratch folder, as they often are by the time you look) is left as its name alone. A
// session on another machine has its files there, so they're named but not shown.

import { useContext, useState, type MouseEvent } from 'react';
import { sentFileUrl } from '../api';
import { asksFirst, mediaKind, type MediaKind } from '../sentFiles';
import type { TranscriptItem } from '../types';
import { ImagesFrom } from './DescriptionEditor';
import { FileLink, FileWhy, useFileActions } from './Markdown';
import { Icon } from './primitives';

const ASK: Record<MediaKind, { icon: string; label: string }> = {
  image: { icon: 'photo', label: 'Show image' },
  video: { icon: 'movie', label: 'Show video' },
  audio: { icon: 'music', label: 'Show audio' },
};

/** `at` is the call that sent them (its cursor), which each file's address carries. */
export function SentFiles({ sent, at }: { sent: NonNullable<TranscriptItem['sent']>; at: number }) {
  const away = useContext(ImagesFrom);
  return (
    <div className="ca-thread-sent">
      {sent.caption && <p className="ca-thread-sentcaption">{sent.caption}</p>}
      <ul aria-label="Files it sent you">
        {sent.files.map((f) => {
          const kind = away || !f.startsWith('/') ? null : mediaKind(f);
          return kind ? (
            <SentMedia key={f} path={f} kind={kind} at={at} />
          ) : (
            <li key={f}>
              <FileLink path={f} />
            </li>
          );
        })}
      </ul>
    </div>
  );
}

function SentMedia({ path, kind, at }: { path: string; kind: MediaKind; at: number }) {
  const [asked, setAsked] = useState(() => !asksFirst(path));
  const [failed, setFailed] = useState(false);
  const f = useFileActions({ path, line: null }, 'file');
  if (failed) {
    return (
      <li>
        <FileLink path={path} />
      </li>
    );
  }
  const src = sentFileUrl(path, at);
  const gone = () => setFailed(true);
  const shown = !asked ? (
    <button type="button" className="ca-btn ca-btn--secondary ca-btn--sm" title="macOS may ask to let Codealong read this folder" onClick={() => setAsked(true)}>
      <Icon name={ASK[kind].icon} size={13} />
      <span>{ASK[kind].label}</span>
    </button>
  ) : kind === 'image' ? (
    <button type="button" className="ca-desc-img" title={`Open ${path}`} onClick={f.open} onContextMenu={f.menu}>
      <img src={src} alt="" draggable={false} onError={gone} />
    </button>
  ) : kind === 'video' ? (
    // Its first frame stands in for it until it plays: WebKit shows none at its very start.
    <video src={`${src}#t=0.001`} controls preload="metadata" onError={gone} onContextMenu={f.menu} />
  ) : (
    <audio src={src} controls preload="metadata" onError={gone} onContextMenu={f.menu} />
  );
  return (
    // A click here is the file's, never the card's or the row's it's shown in.
    <li className="is-media" onClick={(e: MouseEvent) => e.stopPropagation()}>
      <figure className="ca-thread-sentfile">
        {shown}
        <figcaption>
          <FileLink path={path} />
        </figcaption>
      </figure>
      <FileWhy why={f.why} />
    </li>
  );
}
