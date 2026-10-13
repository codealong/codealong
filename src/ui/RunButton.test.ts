import { describe, expect, it } from 'vitest';
import { bangCommand, isCommandBlock, runFolder } from './RunButton';

describe('runFolder', () => {
  it('runs in the worktree a session is shown in, or in its own folder when that is inside it', () => {
    // Started in the main checkout, working in a worktree of its own.
    expect(runFolder({ path: '/p/app-fix' }, { cwd: '/p/app' })).toBe('/p/app-fix');
    expect(runFolder({ path: '/p/app' }, { cwd: '/p/app/src-tauri' })).toBe('/p/app/src-tauri');
    expect(runFolder({ path: '/p/app' }, { cwd: '/p/app' })).toBe('/p/app');
    // A sibling whose name starts the same isn't inside.
    expect(runFolder({ path: '/p/app' }, { cwd: '/p/app-other' })).toBe('/p/app');
    // An app's chats share a lane; each runs in its own folder.
    expect(runFolder({ path: '/chats/' }, { cwd: '/chats/scratch-1' })).toBe('/chats/scratch-1');
    expect(runFolder({ path: '/p/app' }, { cwd: '' })).toBe('/p/app');
  });
});

describe('commands led by !', () => {
  it('runs in a tab of yours when the box is led by !, and not an image that opens it', () => {
    expect(bangCommand('! gcloud auth login')).toBe(true);
    expect(bangCommand('  !ls -la\n')).toBe(true);
    expect(bangCommand('!cd app &&\nnpm test')).toBe(true);
    expect(bangCommand('![shot](images/ab12.png) what is this?')).toBe(false);
    expect(bangCommand('!')).toBe(false);
    expect(bangCommand('!  ')).toBe(false);
    expect(bangCommand('run ! this')).toBe(false);
  });
});

describe('blocks with no language', () => {
  const known = new Set(['git', 'npm', 'cd', 'export', 'make', 'ls', 'grep', 'echo', 'zsh']);
  const runs = (text: string) => isCommandBlock(text, known);

  it('run when each line starts with a command this Mac has', () => {
    // As an agent wrote one for you to run (2026-10-09).
    expect(runs('git -C ~/projects/app-infra pull --ff-only')).toBe(true);
    expect(runs('# first\ncd ~/app && npm ci\n\nnpm test   # a minute')).toBe(true);
    expect(runs('NODE_ENV=test CI=1 npm test')).toBe(true);
    expect(runs('make;')).toBe(true);
    expect(runs('ls | grep x')).toBe(true);
    // A line carried on into the next, which needn't start with a command.
    expect(runs('git log \\\n  --oneline -5 |\n  grep fix')).toBe(true);
  });

  it("don't for output, paths, settings, code, or a command shown with what it printed", () => {
    expect(runs('git status\nOn branch main')).toBe(false);
    expect(runs('~/projects/app/src/main.ts')).toBe(false);
    expect(runs('src/ui/Markdown.tsx:153')).toBe(false);
    expect(runs('./scripts/relaunch.sh')).toBe(false);
    expect(runs('[server]\nport = 8080')).toBe(false);
    expect(runs('$ git status')).toBe(false);
    expect(runs('zsh: command not found: foo')).toBe(false);
    expect(runs('echo("hi")')).toBe(false);
    expect(runs('Git pull')).toBe(false);
    expect(runs('deploy --prod')).toBe(false);
    expect(runs('# just a note')).toBe(false);
    expect(runs('')).toBe(false);
  });
});
