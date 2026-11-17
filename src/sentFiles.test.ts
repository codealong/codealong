import { describe, expect, it } from 'vitest';
import { asksFirst, mediaKind } from './sentFiles';

describe('mediaKind', () => {
  it('shows pictures, films and recordings, by their names', () => {
    expect(mediaKind('/tmp/videos/teaser-a-480p.mp4')).toBe('video');
    expect(mediaKind('/tmp/shots/After.PNG')).toBe('image');
    expect(mediaKind('/tmp/voiceover.m4a')).toBe('audio');
  });

  it('lists everything else', () => {
    expect(mediaKind('/tmp/report.pdf')).toBeNull();
    expect(mediaKind('/tmp/logo.svg')).toBeNull();
    expect(mediaKind('/tmp/.mp4')).toBeNull();
    expect(mediaKind('/tmp/mp4')).toBeNull();
    expect(mediaKind('/tmp/films.mp4/notes')).toBeNull();
  });
});

describe('asksFirst', () => {
  it('waits for a click where macOS asks before a read', () => {
    expect(asksFirst('/Users/me/Downloads/clip.mp4')).toBe(true);
    expect(asksFirst('/Users/me/Desktop/shot.png')).toBe(true);
    expect(asksFirst('/Users/me/Documents/northwind/after.png')).toBe(true);
    expect(asksFirst('/Users/me/Library/Mobile Documents/com~apple~CloudDocs/a.png')).toBe(true);
    expect(asksFirst('/Volumes/Atlas/renders/b.mov')).toBe(true);
  });

  it('shows the rest at once', () => {
    expect(asksFirst('/private/tmp/scratchpad/teaser.mp4')).toBe(false);
    expect(asksFirst('/Users/me/Library/Application Support/Codealong/chats/videos/a.mp4')).toBe(false);
    expect(asksFirst('/Users/me/projects/kestrel/DesktopShots/a.png')).toBe(false);
    expect(asksFirst('/Users/me/DownloadsArchive/a.png')).toBe(false);
  });
});
