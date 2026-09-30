import { describe, expect, it } from 'vitest';
import { runFolder } from './RunButton';

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
