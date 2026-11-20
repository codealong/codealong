// @vitest-environment jsdom
import { beforeEach, describe, expect, it } from 'vitest';
import { act, engine, mountHook, settle } from './testing/dom';
import { reloadCount } from './reload';
import * as fx from './testing/fixtures';
import type { Board } from './types';
import { useZoomReload } from './useZoomReload';
import { pageZoom } from './zoom';

// Zooming and reloading the main window (useZoomReload.ts), from its keys and from View's menu.

let did: string[] = [];
beforeEach(() => {
  did = [];
});
const say = (m: string) => void did.push(`say “${m}”`);
const mount = (board: Board | null = fx.board()) => mountHook((b: Board | null) => useZoomReload(b, say), board);

describe('zooming', () => {
  it('asks the engine for the step and says the zoom it came to', async () => {
    engine.answer('zoom', 110);
    const w = mount();
    w.current.zoom('in');
    await settle();
    expect(engine.sent('zoom')).toEqual([{ step: 'in' }]);
    expect(did).toEqual(['say “Zoomed to 110%.”']);
  });

  it('says why when the engine turns it down', async () => {
    engine.answer('zoom', () => {
      throw new Error('It’s as big as it gets.');
    });
    mount().current.zoom('in');
    await settle();
    expect(did).toEqual(['say “It’s as big as it gets.”']);
  });

  it('zooms from View’s menu, sent to this window', async () => {
    engine.answer('zoom', 90);
    mount();
    await settle();
    act(() => engine.emitTo('main', 'zoom', 'out'));
    await settle();
    expect(engine.sent('zoom')).toEqual([{ step: 'out' }]);
  });

  it('draws the page at the zoom the board says', () => {
    const w = mount(fx.board({ zoom: 125 }));
    expect(pageZoom()).toBe(1.25);
    w.rerender(fx.board({ zoom: 100 }));
    expect(pageZoom()).toBe(1);
  });
});

describe('reloading', () => {
  it('reads again what the window shows, usage too, and says so', async () => {
    const before = reloadCount();
    mount().current.reload();
    await settle();
    expect(reloadCount()).toBe(before + 1);
    expect(engine.sent('refresh_usage')).toHaveLength(1);
    expect(did).toEqual(['say “Reloading…”']);
  });

  it('reloads from View’s menu, sent to this window', async () => {
    const before = reloadCount();
    mount();
    await settle();
    act(() => engine.emitTo('main', 'reload', null));
    expect(reloadCount()).toBe(before + 1);
  });

  it('stops hearing the menu once the window goes', async () => {
    const w = mount();
    await settle();
    w.unmount();
    await settle();
    expect([engine.listening('zoom'), engine.listening('reload')]).toEqual([0, 0]);
  });
});
