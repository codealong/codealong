// @vitest-environment jsdom
import { describe, expect, it } from 'vitest';
import { act, engine, mountHook, settle } from './testing/dom';
import { useSettingsAt } from './useSettingsAt';

// Where the main window's Settings is open (useSettingsAt.ts).

describe('Settings in the main window', () => {
  it('opens at General the first time, and at the section asked for, on an account when one is named', () => {
    const w = mountHook(useSettingsAt);
    expect(w.current.settings).toBeNull();
    act(() => w.current.openSettings());
    expect(w.current.settings).toEqual({ section: 'general', focus: null });
    act(() => w.current.openSettings('accounts', 'northwind'));
    expect(w.current.settings).toEqual({ section: 'accounts', focus: 'northwind' });
    act(() => w.current.closeSettings());
    expect(w.current.settings).toBeNull();
  });

  it('comes back to the section you left, wherever you went in it', () => {
    const w = mountHook(useSettingsAt);
    act(() => w.current.openSettings('projects'));
    // The dialog's own tabs.
    act(() => w.current.setSettings({ section: 'automation', focus: null }));
    act(() => w.current.closeSettings());
    act(() => w.current.openSettings());
    expect(w.current.settings).toEqual({ section: 'automation', focus: null });
  });

  it('opens from the app menu’s Settings…, sent to this window, at the section you left', async () => {
    const w = mountHook(useSettingsAt);
    await settle();
    act(() => w.current.openSettings('accounts', 'northwind'));
    act(() => w.current.closeSettings());
    act(() => engine.emitTo('quick', 'settings', null));
    expect(w.current.settings).toBeNull();
    act(() => engine.emitTo('main', 'settings', null));
    expect(w.current.settings).toEqual({ section: 'accounts', focus: null });
  });

  it('keeps the same ways to open and close it from render to render', () => {
    const w = mountHook(useSettingsAt);
    const { openSettings, closeSettings } = w.current;
    act(() => w.current.openSettings('accounts'));
    expect([w.current.openSettings, w.current.closeSettings]).toEqual([openSettings, closeSettings]);
  });
});
