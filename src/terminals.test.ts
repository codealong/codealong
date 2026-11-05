import { afterEach, describe, expect, it, vi } from 'vitest';

/** The app's say, held until a test gives it; its focus events, sent when a test sends one; and
 *  their listeners being in, when a test says so. */
let answer: (has: boolean) => void = () => {};
let event: (has: boolean) => void = () => {};
let listening: () => void = () => {};
let asked = 0;

vi.mock('./api', async (original) => ({
  ...(await original<typeof import('./api')>()),
  inTauri: true,
  windowKeys: vi.fn(() => {
    asked++;
    return new Promise<boolean>((resolve) => (answer = resolve));
  }),
  onWindowKeys: vi.fn((cb: (has: boolean) => void, ready?: () => void) => {
    event = cb;
    listening = ready ?? (() => {});
    return () => {};
  }),
}));

afterEach(() => {
  vi.resetModules();
  vi.unstubAllGlobals();
});

async function fresh() {
  asked = 0;
  // A window that floats says it has the keyboard while it doesn't.
  vi.stubGlobal('document', { hasFocus: () => true });
  return import('./terminals');
}

describe('whether this window has the keyboard, for a terminal’s size and colors', () => {
  it('is no until the app says, whatever the page says, asked once its events are heard', async () => {
    const t = await fresh();
    const heard: boolean[] = [];
    t.onKeysHere((has) => heard.push(has));
    expect(t.hasKeys(), 'not yet known: a kept card behind the window you’re in mustn’t give its size').toBe(false);
    expect(asked, 'not asked before a change could be heard, or one between would be lost').toBe(0);
    listening();
    expect(asked).toBe(1);
    answer(false);
    await Promise.resolve();
    expect(t.hasKeys()).toBe(false);
    event(true);
    expect(t.hasKeys()).toBe(true);
    event(false);
    expect(t.hasKeys()).toBe(false);
    expect(heard).toEqual([false, true, false]);
  });

  it('goes by an event that came while the app was asked, which is newer than its answer', async () => {
    const t = await fresh();
    listening();
    event(true);
    answer(false);
    await Promise.resolve();
    expect(t.hasKeys()).toBe(true);
  });
});
