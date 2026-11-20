// @vitest-environment jsdom
import { beforeEach, describe, expect, it } from 'vitest';
import { act, engine, mountHook, settle } from './testing/dom';
import { useBarPlace } from './useBarPlace';

// Where the agents bar shows, as the main window's title bar says it and changes it (useBarPlace.ts).

let did: string[] = [];
beforeEach(() => {
  did = [];
  engine.answer('bar_mode', { mode: 'notch', notch: null });
});
const say = (m: string) => void did.push(`say “${m}”`);
async function mount() {
  const w = mountHook(() => useBarPlace(say));
  await settle();
  return w;
}

describe('where the agents bar shows', () => {
  it('is read as the window opens, floating until then, and followed as it changes elsewhere', async () => {
    const w = mountHook(() => useBarPlace(say));
    expect(w.current.barPlace).toBe('floating');
    await settle();
    expect(w.current.barPlace).toBe('notch');
    act(() => engine.emit('bar-mode', { mode: 'menubar', notch: null }));
    expect(w.current.barPlace).toBe('menubar');
  });

  it('moves to the place picked, shown at once', async () => {
    const w = await mount();
    act(() => w.current.pickBarPlace('floating'));
    expect(w.current.barPlace).toBe('floating');
    await settle();
    expect(engine.sent('set_bar_mode')).toEqual([{ mode: 'floating' }]);
  });

  it('turns off when the place it’s in is picked', async () => {
    const w = await mount();
    act(() => w.current.pickBarPlace('notch'));
    expect(w.current.barPlace).toBe('off');
    await settle();
    expect(engine.sent('set_bar_mode')).toEqual([{ mode: 'off' }]);
  });

  it('goes back to where it was, saying why, when the engine turns the move down', async () => {
    engine.answer('set_bar_mode', () => {
      throw new Error('This Mac has no notch.');
    });
    const w = await mount();
    act(() => w.current.pickBarPlace('menubar'));
    expect(w.current.barPlace).toBe('menubar');
    await settle();
    expect(w.current.barPlace).toBe('notch');
    expect(did).toEqual(['say “Error: This Mac has no notch.”']);
  });

  it('stops following once the window goes', async () => {
    const w = await mount();
    w.unmount();
    await settle();
    expect(engine.listening('bar-mode')).toBe(0);
  });
});
