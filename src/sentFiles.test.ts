import { describe, expect, it } from 'vitest';
import { mediaKind } from './sentFiles';

describe('mediaKind', () => {
  it('shows pictures, films and recordings, by their names', () => {
    expect(mediaKind('/tmp/videos/teaser-a-480p.mp4')).toBe('video');
    expect(mediaKind('/tmp/shots/After.PNG')).toBe('image');
    expect(mediaKind('/tmp/voiceover.m4a')).toBe('audio');
    expect(mediaKind('C:\\Users\\me\\Videos\\teaser.mov')).toBe('video');
  });

  it('lists everything else', () => {
    expect(mediaKind('/tmp/report.pdf')).toBeNull();
    expect(mediaKind('/tmp/logo.svg')).toBeNull();
    expect(mediaKind('/tmp/.mp4')).toBeNull();
    expect(mediaKind('/tmp/mp4')).toBeNull();
    expect(mediaKind('/tmp/films.mp4/notes')).toBeNull();
    expect(mediaKind('shots/after.png')).toBeNull();
  });
});
