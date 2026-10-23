import { describe, expect, it, vi } from 'vitest';
import type { UpdateView } from '../types';
import { checkedSays, devUpdateWords, untilChecked, updateSays } from './Updates';

// The app's side of updates, as a test drives it: a change it pushes, and the moment it's heard.
let push: ((view: UpdateView) => void) | null = null;
let heard: (() => void) | null = null;
let stands: UpdateView = { current: '0.5.0', state: 'checking' };
vi.mock('../api', () => ({
  onUpdateStatus: vi.fn((cb: (view: UpdateView) => void, listening?: () => void) => {
    push = cb;
    heard = listening ?? null;
    return () => {
      push = null;
    };
  }),
  updateStatus: vi.fn(() => Promise.resolve(stands)),
  checkForUpdates: vi.fn(),
  restartToUpdate: vi.fn(),
  askAgent: vi.fn(),
}));

describe('untilChecked', () => {
  it('reads where updates stand only once it hears changes, and waits out the check', async () => {
    const { updateStatus } = await import('../api');
    stands = { current: '0.5.0', state: 'checking' };
    const outcome = untilChecked();
    expect(updateStatus).not.toHaveBeenCalled();
    heard?.();
    await Promise.resolve();
    push?.({ current: '0.5.0', state: 'installing', version: '0.6.0' });
    push?.({ current: '0.5.0', state: 'ready', version: '0.6.0', notes: null });
    await expect(outcome).resolves.toMatchObject({ state: 'ready', version: '0.6.0' });
    expect(push).toBeNull();
  });

  it('settles at once on what stands when no check is under way', async () => {
    stands = { current: '0.5.0', state: 'off' };
    const outcome = untilChecked();
    heard?.();
    await expect(outcome).resolves.toMatchObject({ state: 'off' });
  });
});

describe('checkedSays', () => {
  it('says what the app menu’s check came to', () => {
    expect(checkedSays({ current: '0.5.0', state: 'upToDate', checkedAt: 0 }, 0)).toBe('Codealong 0.5.0 is the latest version.');
    expect(checkedSays({ current: '0.5.0', state: 'ready', version: '0.6.0', notes: 'Faster' }, 0)).toBe('Codealong 0.6.0 is ready: restart to update from the sidebar.');
    expect(checkedSays({ current: '0.5.0', state: 'failed', error: 'Couldn’t check for updates: offline', at: 0 }, 0)).toBe('Couldn’t check for updates: offline');
    expect(checkedSays({ current: '0.5.0', state: 'off' }, 0)).toBe('This is a development build, which never updates itself.');
  });
});

describe('a development build behind origin/main', () => {
  const behind = { built: '1a2b3c4d5e6f', commits: 3, source: '/Users/me/projects/codealong', laneId: '/Users/me/projects/codealong', checkedAt: 0 };

  it('says so in Settings, with the commit it was built from', () => {
    expect(updateSays({ current: '0.5.0', state: 'off', behind }, 0)).toBe('This is a development build, from 1a2b3c4d. origin/main has 3 commits since: Update has an agent pull them and relaunch it.');
  });

  it('offers Update in the sidebar, counting what’s new', () => {
    expect(devUpdateWords(behind)).toEqual({ label: 'Update: 3 new commits', title: 'origin/main has 3 new commits since this build of Codealong Dev (1a2b3c4d): an agent pulls them and relaunches it' });
    expect(devUpdateWords({ ...behind, commits: 1 }).label).toBe('Update: 1 new commit');
  });
});
