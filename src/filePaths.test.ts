import { describe, expect, it } from 'vitest';
import { fileInLine, fileUrlAt, isMarkdownFile, localImagePath, pathIn } from './filePaths';

describe('fileInLine', () => {
  it('finds the file a call worked on where its line names it', () => {
    expect(fileInLine('Read · `statusbar-rest.png`', '/Users/me/projects/atlas/shots/statusbar-rest.png')).toEqual({ before: 'Read · ', name: 'statusbar-rest.png', after: '' });
    // The last of two the same, which is the file where a line names something else first.
    expect(fileInLine('Edit · `a.ts` in `a.ts`', '/Users/me/a.ts')).toEqual({ before: 'Edit · `a.ts` in ', name: 'a.ts', after: '' });
    expect(fileInLine('Read · `main.rs`', 'C:\\demo\\src\\main.rs')).toEqual({ before: 'Read · ', name: 'main.rs', after: '' });
    // A POSIX name can have a backslash in it, and a backtick, which the engine writes as a quote.
    expect(fileInLine('Read · `notes\\draft.md`', '/tmp/notes\\draft.md')).toEqual({ before: 'Read · ', name: 'notes\\draft.md', after: '' });
    expect(fileInLine("Read · `it's.md`", '/tmp/it`s.md')).toEqual({ before: 'Read · ', name: 'it`s.md', after: '' });
  });

  it('puts it after a line that does not name it', () => {
    expect(fileInLine('View file · Reading the config', '/Users/me/projects/atlas/config.toml')).toEqual({ before: 'View file · Reading the config · ', name: 'config.toml', after: '' });
  });
});

describe('pathIn', () => {
  it('takes a full path, or one under home, as it is written', () => {
    expect(pathIn('/private/tmp/claude-501/northwind/scratchpad/export-steps.md')).toEqual({ path: '/private/tmp/claude-501/northwind/scratchpad/export-steps.md', line: null });
    expect(pathIn('~/projects/atlas/README.md')).toEqual({ path: '~/projects/atlas/README.md', line: null });
    expect(pathIn('~/notes.md')).toEqual({ path: '~/notes.md', line: null });
    expect(pathIn('/Users/me/Library/Application Support/Kestrel/config.toml')).toEqual({ path: '/Users/me/Library/Application Support/Kestrel/config.toml', line: null });
    expect(pathIn('/Users/me/projects/atlas/')).toEqual({ path: '/Users/me/projects/atlas/', line: null });
  });

  it('keeps a line after the path apart, and a column after it off', () => {
    expect(pathIn('/Users/me/projects/atlas/src/main.rs:42')).toEqual({ path: '/Users/me/projects/atlas/src/main.rs', line: 42 });
    expect(pathIn('/Users/me/projects/atlas/src/main.rs:42:7')).toEqual({ path: '/Users/me/projects/atlas/src/main.rs', line: 42 });
    // A colon that isn't before a line is part of the name.
    expect(pathIn('/Users/me/shots/12:30 run.png')).toEqual({ path: '/Users/me/shots/12:30 run.png', line: null });
  });

  it('takes any name a file can have, leaving it to the engine to say whether one is there', () => {
    for (const path of ['/Users/me/Documents/Plan - final.md', "/Users/me/Documents/Client's notes.md", '/Users/me/Research & Development/plan.md', '/Users/me/projects/atlas/app/[id]/page.tsx']) {
      expect(pathIn(path), path).toEqual({ path, line: null });
    }
    // A command is asked about too, and found to be nothing.
    expect(pathIn('/usr/bin/open -R /Users/me/a.md')?.path).toBe('/usr/bin/open -R /Users/me/a.md');
  });

  it('leaves what can’t be one: a relative path, a skill, a command that doesn’t start with one, a link', () => {
    for (const code of ['src/ui/Markdown.tsx', './run.sh', '/release', '/codealong:orchestrate', '/', '~/', 'ls /Users/me', 'https://example.com/a/b', '//cdn.example.com/a.js', ' /Users/me/a.md', '/Users/me/a.md ']) {
      expect(pathIn(code), code).toBeNull();
    }
  });
});

describe('fileUrlAt', () => {
  it('parses local file:// URLs, percent-decoding paths and reading line anchors', () => {
    expect(fileUrlAt('file:///Users/me/brain/mocks.md')).toEqual({ path: '/Users/me/brain/mocks.md', line: null });
    expect(fileUrlAt('file://localhost/Users/me/My%20Docs/notes.md')).toEqual({ path: '/Users/me/My Docs/notes.md', line: null });
    expect(fileUrlAt('file:///Users/me/src/Markdown.tsx#L344-L359')).toEqual({ path: '/Users/me/src/Markdown.tsx', line: 344 });
    expect(fileUrlAt('file:///Users/me/src/Markdown.tsx#L12')).toEqual({ path: '/Users/me/src/Markdown.tsx', line: 12 });
    expect(fileUrlAt('file:///Users/me/src/main.rs:42:7')).toEqual({ path: '/Users/me/src/main.rs', line: 42 });
    expect(fileUrlAt('file:///C:/Users/me/notes.md#L5')).toEqual({ path: 'C:/Users/me/notes.md', line: 5 });
  });

  it('rejects non-file URLs, remote hosts and bad encodings', () => {
    for (const url of ['https://example.com/a.md', 'file://otherhost/Users/me/a.md', 'file:///', 'file:///Users/%E0%A4%A/a.md', '/Users/me/a.md']) {
      expect(fileUrlAt(url), url).toBeNull();
    }
  });
});

describe('localImagePath', () => {
  it('recognizes absolute and file:// local image paths and rejects others', () => {
    expect(localImagePath('/Users/me/brain/mock-light.png')).toBe('/Users/me/brain/mock-light.png');
    expect(localImagePath('file:///Users/me/brain/mock%20dark.webp')).toBe('/Users/me/brain/mock dark.webp');
    expect(localImagePath('</Users/me/My Shots/a.jpg>')).toBe('/Users/me/My Shots/a.jpg');
    expect(localImagePath('/Users/me/a.png "Light"')).toBe('/Users/me/a.png');
    expect(localImagePath('C:\\shots\\after.png')).toBe('C:\\shots\\after.png');
    for (const target of ['https://example.com/a.png', '//cdn.example.com/a.png', 'images/0123456789abcdef.png', '/Users/me/notes.md', '/Users/me/clip.mp4', 'relative/shot.png']) {
      expect(localImagePath(target), target).toBeNull();
    }
  });
});

describe('isMarkdownFile', () => {
  it('matches .md and .markdown paths regardless of case', () => {
    expect(isMarkdownFile('/Users/me/tool-rendering-mocks.md')).toBe(true);
    expect(isMarkdownFile('/Users/me/README.MARKDOWN')).toBe(true);
    expect(isMarkdownFile('/Users/me/shot.png')).toBe(false);
    expect(isMarkdownFile('/Users/me/notes.md.bak')).toBe(false);
  });
});
