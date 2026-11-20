// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, mountHook } from './testing/dom';
import type { SavedView } from './savedViews';
import { useSavedViews } from './useSavedViews';

// The saved views as a window has them (useSavedViews.ts).

afterEach(() => {
  vi.restoreAllMocks();
});

const KEY = 'codealong.savedViews';
const needs: SavedView = { id: 'a', name: 'Needs me', view: 'sessions', project: 'All projects', device: [], age: 'all', filter: ['needs'] };
const reviews: SavedView = { id: 'b', name: 'Reviews', view: 'prs', project: 'Northwind', device: [], age: '3d', filter: [] };

describe('a window’s saved views', () => {
  it('are read from the page’s storage as it opens, none where there are none', () => {
    expect(mountHook(useSavedViews).current[0]).toEqual([]);
    window.localStorage.setItem(KEY, JSON.stringify([needs]));
    expect(mountHook(useSavedViews).current[0]).toEqual([needs]);
  });

  it('are kept there as they change here', () => {
    const w = mountHook(useSavedViews);
    act(() => w.current[1]([needs, reviews]));
    expect(w.current[0]).toEqual([needs, reviews]);
    expect(JSON.parse(window.localStorage.getItem(KEY) ?? '[]')).toEqual([needs, reviews]);
  });

  it('follow what another window saves', () => {
    const w = mountHook(useSavedViews);
    act(() => void window.dispatchEvent(new StorageEvent('storage', { key: KEY, newValue: JSON.stringify([reviews]) })));
    expect(w.current[0]).toEqual([reviews]);
    // Another key's change is none of theirs.
    act(() => void window.dispatchEvent(new StorageEvent('storage', { key: 'codealong.view', newValue: '"prs"' })));
    expect(w.current[0]).toEqual([reviews]);
  });

  it('stop listening to the page’s storage once the window goes', () => {
    const added = vi.spyOn(window, 'addEventListener');
    const removed = vi.spyOn(window, 'removeEventListener');
    /** The listeners to the page's storage that `spy` was handed. */
    const toStorage = (spy: typeof added | typeof removed) => spy.mock.calls.filter(([type]) => type === 'storage').map(([, listener]) => listener);
    const w = mountHook(useSavedViews);
    expect([toStorage(added).length, toStorage(removed).length]).toEqual([1, 0]);
    w.unmount();
    // The one it listened with, taken off: a listener left behind would hear every later save.
    expect(toStorage(removed)).toEqual(toStorage(added));
    expect(toStorage(removed)[0]).toBe(toStorage(added)[0]);
  });
});
