// The files a call sent you (Claude Code's SendUserFile), under its line in a conversation
// (SessionThread.tsx): what the agent said of them, then each file. A picture, a film or a
// recording (sentFiles.ts `mediaKind`) shows in place over its name, side by side as they fit: a
// picture opens on a click as its name does, and a film or a recording plays where it is, with
// its controls, served by the app (sentfiles.rs). Any other file is its name and folder on a line
// of its own. Every name opens the file on a click as a double-click in the Finder would, and
// offers the file's menu on a right-click (Markdown.tsx `FileLink`).
//
// Which can show is the app's to say, each file only looked up (api.ts `sentFiles`): one macOS
// asks about before a read shows once you ask for it, so scrolling past it never brings up the
// prompt, and one that's gone (from a scratch folder, as they often are by the time you look) is
// named alone, as every file is until the app has said. One the app can't play says so in its
// place. A session on another machine has its files there, so they're named but not shown.
//
// Pictures and films take their room as they load, after the conversation placed its view, so
// the block tells the conversation what it grew by (`ThreadGrows`), which keeps you at the end or
// keeps what you're reading where it was, as it does for an image in a prompt.

import { createContext, useContext, useEffect, useLayoutEffect, useMemo, useRef, useState, type MouseEvent } from 'react';
import { sentFiles, sentFileUrl } from '../api';
import { useLatest } from '../latest';
import { mediaKind, type MediaKind } from '../sentFiles';
import type { SentState, TranscriptItem } from '../types';
import { ImagesFrom } from './DescriptionEditor';
import { FileLink, FileWhy, useFileActions } from './Markdown';
import { Icon } from './primitives';

/** What the conversation does when a block of sent files under one of its calls grows by `by`
 *  pixels after it was placed (shrinks, when it's negative). */
export const ThreadGrows = createContext<((el: Element, by: number) => void) | null>(null);

const ASK: Record<MediaKind, { icon: string; label: string }> = {
  image: { icon: 'photo', label: 'Show image' },
  video: { icon: 'movie', label: 'Show video' },
  audio: { icon: 'music', label: 'Show audio' },
};

/** What the app says of each of `paths`, by path; null until it has. */
function useSentStates(paths: string[]): Record<string, SentState> | null {
  const key = paths.join('\n');
  const asked = useMemo(() => (key ? key.split('\n') : []), [key]);
  const [said, setSaid] = useState<{ key: string; of: Record<string, SentState> } | null>(null);
  useEffect(() => {
    if (!asked.length) return;
    let live = true;
    const settle = (states: SentState[]) => {
      if (live) setSaid({ key, of: Object.fromEntries(asked.map((p, i) => [p, states[i] ?? 'gone'])) });
    };
    // Unanswered, each is tried as if here: one that isn't says it can't be shown in its place.
    sentFiles(asked).then(settle, () => settle(asked.map(() => 'here')));
    return () => {
      live = false;
    };
  }, [key, asked]);
  return said?.key === key ? said.of : null;
}

/** `at` is the call that sent them (its cursor), which each file's address carries. */
export function SentFiles({ sent, at }: { sent: NonNullable<TranscriptItem['sent']>; at: number }) {
  const away = useContext(ImagesFrom);
  const media = useMemo(() => (away ? [] : sent.files.filter((f) => mediaKind(f))), [away, sent.files]);
  const states = useSentStates(media);
  const shown = media.filter((f) => states && states[f] !== 'gone');
  const listed = sent.files.filter((f) => !shown.includes(f));

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

  return (
    <div className="ca-thread-sent" ref={root}>
      {sent.caption && <p className="ca-thread-sentcaption">{sent.caption}</p>}
      {shown.length > 0 && (
        <ul className="ca-thread-sentmedia" aria-label="Images, videos and audio it sent you">
          {shown.map((f) => (
            <SentMedia key={f} path={f} kind={mediaKind(f)!} asksFirst={states?.[f] === 'asksFirst'} at={at} />
          ))}
        </ul>
      )}
      {listed.length > 0 && (
        <ul aria-label="Files it sent you">
          {listed.map((f) => (
            <li key={f}>
              <FileLink path={f} />
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function SentMedia({ path, kind, asksFirst, at }: { path: string; kind: MediaKind; asksFirst: boolean; at: number }) {
  const [asked, setAsked] = useState(false);
  const [failed, setFailed] = useState(false);
  const f = useFileActions({ path, line: null }, 'file');
  const src = sentFileUrl(path, at, asked);
  const cant = () => setFailed(true);
  const shown =
    asksFirst && !asked ? (
      <div className="ca-thread-sentbox is-note">
        <button type="button" className="ca-btn ca-btn--secondary ca-btn--sm" title="macOS may ask you to let Codealong read this folder" onClick={() => setAsked(true)}>
          <Icon name={ASK[kind].icon} size={13} />
          <span>{ASK[kind].label}</span>
        </button>
      </div>
    ) : failed ? (
      <div className="ca-thread-sentbox is-note">
        <Icon name={ASK[kind].icon} size={14} />
        <span>Can’t show it here</span>
      </div>
    ) : kind === 'image' ? (
      <button type="button" className="ca-thread-sentbox is-image" title={`Open ${path}`} onClick={f.open} onContextMenu={f.menu}>
        <img src={src} alt="" draggable={false} onError={cant} />
      </button>
    ) : kind === 'video' ? (
      // Its first frame stands in for it until it plays: WebKit shows none at its very start.
      <video className="ca-thread-sentbox" src={`${src}#t=0.001`} controls preload="metadata" onError={cant} onContextMenu={f.menu} />
    ) : (
      <audio src={src} controls preload="metadata" onError={cant} onContextMenu={f.menu} />
    );
  return (
    // A click here is the file's, never the card's or the row's it's shown in.
    <li onClick={(e: MouseEvent) => e.stopPropagation()}>
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
