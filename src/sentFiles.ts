// What a file a session sent you (Claude Code's SendUserFile) shows as in its conversation
// (ui/SentFiles.tsx): a picture, a film or a recording plays in place, and any other file is only
// listed. The engine serves only these to the windows (sentfiles.rs `mime`), so the two lists go
// together: one a window shows that the engine doesn't serve says it can't be shown.

export type MediaKind = 'image' | 'video' | 'audio';

const KINDS: Record<string, MediaKind> = {
  png: 'image',
  jpg: 'image',
  jpeg: 'image',
  gif: 'image',
  webp: 'image',
  avif: 'image',
  heic: 'image',
  mp4: 'video',
  m4v: 'video',
  mov: 'video',
  webm: 'video',
  mp3: 'audio',
  m4a: 'audio',
  aac: 'audio',
  wav: 'audio',
  flac: 'audio',
};

/** What a sent file shows as, by its name; null for one that's only listed, and for one that
 *  isn't a full path (`/…`, or on Windows `C:\…` or a share's `\\server\…`), which the engine
 *  wouldn't know where to find. */
export function mediaKind(path: string): MediaKind | null {
  if (!/^(\/|[A-Za-z]:[\\/]|\\\\)/.test(path)) return null;
  const name = path.slice(Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\')) + 1);
  const dot = name.lastIndexOf('.');
  return dot > 0 ? (KINDS[name.slice(dot + 1).toLowerCase()] ?? null) : null;
}
