import { afterEach, describe, expect, it, vi } from 'vitest';

/** The app's say, held until a test gives it, and its focus events, sent when a test sends one. */
let answer: (has: boolean) => void = () => {};
let event: (has: boolean) => void = () => {};

vi.mock('./api', async (original) => ({
  ...(await original<typeof import('./api')>()),
  inTauri: true,
  windowKeys: vi.fn(() => new Promise<boolean>((resolve) => (answer = resolve))),
  onWindowKeys: vi.fn((cb: (has: boolean) => void) => {
    event = cb;
    return () => {};
  }),
}));

afterEach(() => {
  vi.resetModules();
  vi.unstubAllGlobals();
});

async function fresh() {
  // A window that floats says it has the keyboard while it doesn't.
  vi.stubGlobal('document', { hasFocus: () => true });
  return import('./terminals');
}

describe('whether this window has the keyboard, for a terminal’s size and colors', () => {
  it('is no until the app says, whatever the page says, and then what the app says', async () => {
    const t = await fresh();
    const heard: boolean[] = [];
    t.onKeysHere((has) => heard.push(has));
    expect(t.hasKeys(), 'not yet known: a kept card behind the window you’re in mustn’t give its size').toBe(false);
    answer(false);
    await Promise.resolve();
    expect(t.hasKeys()).toBe(false);
    event(true);
    expect(t.hasKeys()).toBe(true);
    event(false);
    expect(t.hasKeys()).toBe(false);
    expect(heard).toEqual([false, true, false]);
  });

  it('goes by an event that came before the app’s answer, which is older', async () => {
    const t = await fresh();
    event(true);
    answer(false);
    await Promise.resolve();
    expect(t.hasKeys()).toBe(true);
  });
});
