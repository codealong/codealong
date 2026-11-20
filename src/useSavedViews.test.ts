// @vitest-environment jsdom
import { describe, expect, it } from 'vitest';
import { act, mountHook } from './testing/dom';
import type { SavedView } from './savedViews';
import { useSavedViews } from './useSavedViews';

// The saved views as a window has them (useSavedViews.ts).

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

  it('follow what another window saves, and stop once this one goes', () => {
    const w = mountHook(useSavedViews);
    const fromAnother = (views: SavedView[]) => act(() => void window.dispatchEvent(new StorageEvent('storage', { key: KEY, newValue: JSON.stringify(views) })));
    fromAnother([reviews]);
    expect(w.current[0]).toEqual([reviews]);
    // Another key's change is none of theirs.
    act(() => void window.dispatchEvent(new StorageEvent('storage', { key: 'codealong.view', newValue: '"prs"' })));
    expect(w.current[0]).toEqual([reviews]);
    const last = w.current[0];
    w.unmount();
    fromAnother([needs]);
    expect(w.current[0]).toBe(last);
  });
});
