// What a file a session sent you (Claude Code's SendUserFile) shows as in its conversation
// (ui/SentFiles.tsx): a picture, a film or a recording plays in place, and any other file is only
// listed. The engine serves only these to the windows (sentfiles.rs `mime`), so the two lists go
// together: one a window shows that the engine doesn't serve falls back to its name alone.

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

/** What a sent file shows as, by its name; null for one that's only listed. */
export function mediaKind(path: string): MediaKind | null {
  const name = path.slice(path.lastIndexOf('/') + 1);
  const dot = name.lastIndexOf('.');
  return dot > 0 ? (KINDS[name.slice(dot + 1).toLowerCase()] ?? null) : null;
}

/** Whether reading a file waits on macOS asking you first: one in Desktop, Documents or
 *  Downloads, on another volume, in iCloud Drive or a cloud folder, or in another app's container
 *  (.claude/rules/macos-privacy.md). Such a file shows once you ask for it, so a conversation
 *  scrolled past never brings up the prompt. */
export function asksFirst(path: string): boolean {
  return /^\/Users\/[^/]+\/(Desktop|Documents|Downloads|Library\/(Mobile Documents|CloudStorage|Containers|Group Containers))(\/|$)/.test(path) || path.startsWith('/Volumes/');
}
