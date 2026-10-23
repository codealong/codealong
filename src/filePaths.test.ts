import { describe, expect, it } from 'vitest';
import { pathIn } from './filePaths';

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

  it('leaves what only looks like one: a relative path, a skill, a command, a glob, a link', () => {
    for (const code of [
      'src/ui/Markdown.tsx',
      './run.sh',
      '/release',
      '/codealong:orchestrate',
      '/',
      '~/',
      '/usr/bin/open -R /Users/me/a.md',
      'ls /Users/me',
      '/Users/me/projects/*.md',
      '/Users/me/{a,b}.md',
      '/Users/me/$HOME/a',
      '/Users/me/a.md | head',
      'https://example.com/a/b',
      '//cdn.example.com/a.js',
      ' /Users/me/a.md',
      '/Users/me/a.md ',
    ]) {
      expect(pathIn(code), code).toBeNull();
    }
  });
});
