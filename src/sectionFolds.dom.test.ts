// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { act, mountHook } from './testing/dom';
import { SESSION_FOLDS_KEY, useSectionFolds } from './sectionFolds';

describe('useSectionFolds', () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  afterEach(() => {
    window.localStorage.clear();
  });

  it('persists ambient sections across mounts while keeping urgent bays, days, and Archived ephemeral', () => {
    const first = mountHook((searching: boolean) => useSectionFolds(searching), false);

    expect(first.current.isFolded('working')).toBe(false);
    expect(first.current.isFolded('needs')).toBe(false);
    expect(first.current.isFolded('archived')).toBe(true);
    expect(first.current.showArchived).toBe(false);

    act(() => {
      first.current.toggle('working');
      first.current.toggle('orchestrators');
      first.current.toggle('needs');
      first.current.toggle('today');
      first.current.toggle('archived');
    });

    expect(first.current.isFolded('working')).toBe(true);
    expect(first.current.isFolded('orchestrators')).toBe(true);
    expect(first.current.isFolded('needs')).toBe(true);
    expect(first.current.isFolded('today')).toBe(true);
    expect(first.current.isFolded('archived')).toBe(false);
    expect(first.current.showArchived).toBe(true);
    expect(JSON.parse(window.localStorage.getItem(SESSION_FOLDS_KEY) ?? '[]')).toEqual(['working', 'orchestrators']);

    first.unmount();

    // Fresh mount: ambient folds stay folded; urgent, day, and Archived reset to defaults.
    const second = mountHook((searching: boolean) => useSectionFolds(searching), false);
    expect(second.current.isFolded('working')).toBe(true);
    expect(second.current.isFolded('orchestrators')).toBe(true);
    expect(second.current.isFolded('needs')).toBe(false);
    expect(second.current.isFolded('today')).toBe(false);
    expect(second.current.isFolded('archived')).toBe(true);

    act(() => second.current.unfold('working'));
    expect(second.current.isFolded('working')).toBe(false);
    expect(JSON.parse(window.localStorage.getItem(SESSION_FOLDS_KEY) ?? '[]')).toEqual(['orchestrators']);
    second.unmount();
  });

  it('auto-expands active sections during search and ignores active toggles while searching', () => {
    window.localStorage.setItem(SESSION_FOLDS_KEY, JSON.stringify(['working']));
    const view = mountHook((searching: boolean) => useSectionFolds(searching), true);

    expect(view.current.isFolded('working')).toBe(false);
    expect(view.current.isFolded('archived')).toBe(true);
    expect(view.current.toggleFor('working')).toBeUndefined();
    expect(view.current.toggleFor('archived')).toBeTypeOf('function');

    // Toggling an active section while searching does not mutate stored state; Archived still toggles.
    act(() => {
      view.current.toggle('working');
      view.current.toggle('pinned');
      view.current.toggle('archived');
    });
    expect(JSON.parse(window.localStorage.getItem(SESSION_FOLDS_KEY) ?? '[]')).toEqual(['working']);
    expect(view.current.isFolded('archived')).toBe(false);
    view.unmount();
  });
});
