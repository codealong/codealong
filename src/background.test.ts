import { describe, expect, it } from 'vitest';
import { lastLines, localAddress, taskChipWords, taskCounts, taskName, taskState } from './background';
import type { BackgroundTask } from './types';

const NOW = 1_800_000_000_000;
const MIN = 60_000;
const task = (t: Partial<BackgroundTask>): BackgroundTask => ({ id: 'b1', command: 'npm run dev', startedAt: NOW - 12 * MIN, ...t });

describe('a session’s background commands', () => {
  it('counts what runs apart from what ended, and the chip says the first it has', () => {
    const tasks = [task({}), task({ id: 'b2', endedAt: NOW - MIN, status: 'completed', exitCode: 0 })];
    expect(taskCounts(tasks)).toEqual({ running: 1, ended: 1 });
    expect(taskChipWords({ tasks })).toBe('1 running');
    expect(taskChipWords({ tasks: tasks.slice(1) })).toBe('1 ended');
    expect(taskChipWords({ tasks: [] })).toBeNull();
    expect(taskChipWords({})).toBeNull();
  });

  it('says where each stands, as its harness put it', () => {
    expect(taskState(task({}), NOW)).toBe('Running · 12m');
    const ended = { endedAt: NOW - 3 * MIN };
    expect(taskState(task({ ...ended, status: 'completed', exitCode: 0 }), NOW)).toBe('Done · 3m ago');
    expect(taskState(task({ ...ended, status: 'completed' }), NOW)).toBe('Done · 3m ago');
    expect(taskState(task({ ...ended, status: 'completed', exitCode: 1 }), NOW)).toBe('Ended · exit 1 · 3m ago');
    expect(taskState(task({ ...ended, status: 'failed', exitCode: 144 }), NOW)).toBe('Failed · exit 144 · 3m ago');
    expect(taskState(task({ ...ended, status: 'killed' }), NOW)).toBe('Stopped · 3m ago');
    expect(taskState(task({ ...ended, status: 'stopped' }), NOW)).toBe('Stopped as its session ended · 3m ago');
  });

  it('names one by its command, else what it’s for', () => {
    expect(taskName(task({}))).toBe('npm run dev');
    expect(taskName(task({ command: '', about: 'Build the app' }))).toBe('Build the app');
    expect(taskName(task({ command: ' ' }))).toBe('A command its harness didn’t record');
  });

  it('finds the last address on this machine a server printed', () => {
    expect(localAddress('  VITE ready\n  ➜  Local:   http://localhost:5173/\n  ➜  Network: use --host')).toBe('http://localhost:5173/');
    expect(localAddress('listening on http://0.0.0.0:8080, then http://127.0.0.1:3000/admin.')).toBe('http://127.0.0.1:3000/admin');
    expect(localAddress('{"message":"Dev server running at http://localhost:4321 (pid 79746)"}')).toBe('http://localhost:4321');
    expect(localAddress('see https://example.com')).toBeNull();
  });

  it('keeps the last lines of what it printed', () => {
    expect(lastLines('a\nb\nc\n\n', 2)).toBe('b\nc');
    expect(lastLines('one', 5)).toBe('one');
  });
});
