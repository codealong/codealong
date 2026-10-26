import { afterEach, describe, expect, it, vi } from 'vitest';
import { refresh, refreshUsage } from './api';
import { setPlatform } from './platform';
import { onReload, reloadPressed, reloadWindow } from './reload';

vi.mock('./api', () => ({
  refresh: vi.fn(() => Promise.resolve()),
  refreshUsage: vi.fn(() => Promise.resolve()),
}));

afterEach(() => {
  setPlatform(null);
  vi.clearAllMocks();
});

const key = (k: string, mods: { meta?: boolean; ctrl?: boolean; alt?: boolean; shift?: boolean } = {}) => ({ key: k, code: `Key${k.toUpperCase()}`, metaKey: !!mods.meta, ctrlKey: !!mods.ctrl, altKey: !!mods.alt, shiftKey: !!mods.shift });

describe('reloadPressed', () => {
  it('is ⌘R, and Ctrl+R on Linux', () => {
    setPlatform('mac');
    expect(reloadPressed(key('r', { meta: true }))).toBe(true);
    expect(reloadPressed(key('r', { ctrl: true }))).toBe(false);
    setPlatform('linux');
    expect(reloadPressed(key('r', { ctrl: true }))).toBe(true);
  });

  it('leaves ⌃⌘R, which renames a session and has a PR reviewed', () => {
    setPlatform('mac');
    expect(reloadPressed(key('r', { ctrl: true, meta: true }))).toBe(false);
    expect(reloadPressed(key('R', { shift: true, meta: true }))).toBe(false);
  });
});

describe('reloadWindow', () => {
  it('asks the engine about GitHub again, and usage only for a window that shows it', () => {
    reloadWindow({ usage: false });
    expect(refresh).toHaveBeenCalledTimes(1);
    expect(refreshUsage).not.toHaveBeenCalled();
    reloadWindow({ usage: true });
    expect(refresh).toHaveBeenCalledTimes(2);
    expect(refreshUsage).toHaveBeenCalledTimes(1);
  });

  it('has the caches forget what they hold, until one stops listening', () => {
    const forgot = vi.fn();
    const off = onReload(forgot);
    reloadWindow({ usage: false });
    expect(forgot).toHaveBeenCalledTimes(1);
    off();
    reloadWindow({ usage: false });
    expect(forgot).toHaveBeenCalledTimes(1);
  });
});
