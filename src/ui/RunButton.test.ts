import { describe, expect, it } from 'vitest';
import { isCommandBlock, runFolder, runsInSession, sessionCommand, terminalCommand } from './RunButton';

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
  it('goes into a Claude Code session Codealong types into, and nowhere else', () => {
    const claude = { harness: 'claude' as const };
    expect(runsInSession(claude, { kind: 'typed', app: 'Ghostty' })).toBe(true);
    expect(runsInSession(claude, { kind: 'background' })).toBe(true);
    // Its socket or app would hand the agent the words; not running, there's no shell.
    expect(runsInSession(claude, { kind: 'message' })).toBe(false);
    expect(runsInSession(claude, { kind: 'resume', app: 'Ghostty' })).toBe(false);
    expect(runsInSession(claude, null)).toBe(false);
    expect(runsInSession({ ...claude, hosted: true }, { kind: 'typed', app: 'Ghostty' })).toBe(false);
    expect(runsInSession({ harness: 'codex' }, { kind: 'typed', app: 'Ghostty' })).toBe(false);
  });

  it('is led by ! in the session, and loses it in a new terminal', () => {
    expect(sessionCommand('! cd /w && node send.mjs')).toBe('! cd /w && node send.mjs');
    expect(sessionCommand('  ls -la\n')).toBe('! ls -la');
    expect(terminalCommand('! cd /w && node send.mjs')).toBe('cd /w && node send.mjs');
    expect(terminalCommand('!ls')).toBe('ls');
    expect(terminalCommand('ls')).toBe('ls');
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
