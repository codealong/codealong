// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, engine, mountHook, settle } from './testing/dom';
import * as fx from './testing/fixtures';
import type { StartOptions } from './types';
import { OPTIONS_FOR_MS, RETRY_MS, useRemoteOptions } from './useRemoteOptions';

// Where another machine's accounts can start, as the quick panel asks it (`useRemoteOptions`):
// what goes to the engine (`device_start_options`) as the panel shows and a machine is chosen,
// and what the panel has of each machine's answer meanwhile. The clock is the test's.

const MINI = 'a1b2c3d4e5';
const STUDIO = 'f6e7d8c9b0';
/** What a machine answers: one account, named for the machine. */
const options = (device: string): StartOptions => ({ accounts: [fx.startAccount({ name: `on-${device}` })], codexProjects: [], claudeModels: [] });
const asked = () => engine.sent('device_start_options').map((a) => a.device);

let now = fx.NOW;
beforeEach(() => {
  now = fx.NOW;
  vi.spyOn(Date, 'now').mockImplementation(() => now);
  engine.answer('device_start_options', (args: Record<string, unknown>) => options(String(args.device)));
});
afterEach(() => {
  vi.restoreAllMocks();
});

interface Panel {
  shown: boolean;
  deviceNow: string | null;
}

/** The panel, up or not, with the machine a session would start on. */
async function panel(shown: boolean, deviceNow: string | null) {
  const w = mountHook((p: Panel) => useRemoteOptions(p.shown, p.deviceNow), { shown, deviceNow });
  await settle();
  return {
    get remote() {
      return w.current.remote;
    },
    /** The panel is put away and opened again. */
    reopen: async () => {
      w.rerender({ shown: false, deviceNow });
      w.rerender({ shown: true, deviceNow });
      await settle();
    },
    choose: async (device: string | null) => {
      deviceNow = device;
      w.rerender({ shown, deviceNow });
      await settle();
    },
    askAgain: async (device: string) => {
      act(() => w.current.askAgain(device));
      await settle();
    },
  };
}

describe('where another machine’s accounts can start, as the quick panel asks', () => {
  it('asks nothing while the panel isn’t up, or while a session would start on this machine', async () => {
    const away = await panel(false, MINI);
    const here = await panel(true, null);
    expect([asked(), away.remote, here.remote]).toEqual([[], undefined, undefined]);
  });

  it('asks the machine chosen once the panel is up, and has what it answered with when', async () => {
    const p = await panel(true, MINI);
    expect(asked()).toEqual([MINI]);
    expect(p.remote).toEqual({ at: fx.NOW, options: options(MINI), error: null });
  });

  it('has nothing of a machine until it answers', async () => {
    let answer: (o: StartOptions) => void = () => {};
    engine.answer('device_start_options', () => new Promise<StartOptions>((resolve) => (answer = resolve)));
    const p = await panel(true, MINI);
    expect([asked(), p.remote]).toEqual([[MINI], undefined]);
    answer(options(MINI));
    await settle();
    expect(p.remote?.options).toEqual(options(MINI));
  });

  it('keeps each machine’s answer: another machine is asked for its own, and going back asks nothing', async () => {
    const p = await panel(true, MINI);
    await p.choose(STUDIO);
    expect([asked(), p.remote?.options]).toEqual([[MINI, STUDIO], options(STUDIO)]);
    await p.choose(MINI);
    expect([asked(), p.remote?.options]).toEqual([[MINI, STUDIO], options(MINI)]);
    await p.choose(null);
    expect([asked(), p.remote]).toEqual([[MINI, STUDIO], undefined]);
  });

  it('uses an answer for a minute, and asks again the next time the panel opens after that', async () => {
    const p = await panel(true, MINI);
    now += OPTIONS_FOR_MS - 1;
    await p.reopen();
    expect(asked()).toEqual([MINI]);
    now += 1;
    await p.reopen();
    expect(asked()).toEqual([MINI, MINI]);
    expect(p.remote?.at).toBe(fx.NOW + OPTIONS_FOR_MS);
  });

  it('says why a machine didn’t answer, keeping what it said before, and asks it again five seconds on', async () => {
    const p = await panel(true, MINI);
    engine.answer('device_start_options', () => {
      throw new Error('Mac mini isn’t connected');
    });
    now += OPTIONS_FOR_MS;
    await p.reopen();
    expect(p.remote).toEqual({ at: fx.NOW + OPTIONS_FOR_MS, options: options(MINI), error: 'Mac mini isn’t connected' });
    expect(asked()).toHaveLength(2);
    now += RETRY_MS - 1;
    await p.reopen();
    expect(asked()).toHaveLength(2);
    now += 1;
    await p.reopen();
    expect(asked()).toHaveLength(3);
  });

  it('has no accounts and why for a machine that never answered', async () => {
    engine.answer('device_start_options', () => {
      throw new Error('Mac mini isn’t connected');
    });
    const p = await panel(true, MINI);
    expect(p.remote).toEqual({ at: fx.NOW, options: null, error: 'Mac mini isn’t connected' });
  });

  it('forgets a machine’s answer when told to ask again, asking at once while it’s the one chosen', async () => {
    const p = await panel(true, MINI);
    await p.askAgain(MINI);
    expect([asked(), p.remote?.options]).toEqual([[MINI, MINI], options(MINI)]);
    // One that isn't chosen is asked when it next is.
    await p.choose(STUDIO);
    await p.choose(MINI);
    await p.askAgain(STUDIO);
    expect(asked()).toEqual([MINI, MINI, STUDIO]);
    await p.choose(STUDIO);
    expect(asked()).toEqual([MINI, MINI, STUDIO, STUDIO]);
  });

  it('keeps an answer that comes after another machine was chosen for the machine it was asked of', async () => {
    const waiting: Record<string, (o: StartOptions) => void> = {};
    engine.answer('device_start_options', (args: Record<string, unknown>) => new Promise<StartOptions>((resolve) => (waiting[String(args.device)] = resolve)));
    const p = await panel(true, MINI);
    await p.choose(STUDIO);
    waiting[MINI](options(MINI));
    await settle();
    expect(p.remote).toBeUndefined();
    waiting[STUDIO](options(STUDIO));
    await settle();
    expect(p.remote?.options).toEqual(options(STUDIO));
    await p.choose(MINI);
    expect([p.remote?.options, asked()]).toEqual([options(MINI), [MINI, STUDIO]]);
  });
});
