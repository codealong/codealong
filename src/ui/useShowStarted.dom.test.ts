// @vitest-environment jsdom
import { describe, expect, it } from 'vitest';
import { act, engine, mountHook } from '../testing/dom';
import { useShowStarted } from './useShowStarted';

// The quick panel's switch for showing what it starts as it starts (useShowStarted.ts): what it
// says, what flipping it asks of the engine, and how the board takes over.

/** The switch on a board that says `saved`, with what it says went wrong. */
function mounted(saved: boolean | undefined) {
  const said: string[] = [];
  const w = mountHook(({ saved: now }: { saved: boolean | undefined }) => useShowStarted(now, (m) => void said.push(m)), { saved });
  return { w, said };
}
/** Lets what waits on the engine's answer run. */
const answered = () => act(() => new Promise((resolve) => setTimeout(resolve, 0)));

describe('the quick panel’s show-as-it-starts switch', () => {
  it('says what config.toml says, off when the board doesn’t say', () => {
    expect(mounted(undefined).w.current[0]).toBe(false);
    expect(mounted(true).w.current[0]).toBe(true);
  });

  it('shows a flip at once and keeps it in config.toml, then goes by the board once it says so', async () => {
    const { w, said } = mounted(false);
    act(() => w.current[1](true));
    expect(w.current[0]).toBe(true);
    await answered();
    expect(engine.sent('keep_show_started')).toEqual([{ on: true }]);
    w.rerender({ saved: true });
    expect(w.current[0]).toBe(true);
    // Changed in config.toml by hand, or in Settings: the board says so, and so does the switch.
    w.rerender({ saved: false });
    expect([w.current[0], said]).toEqual([false, []]);
  });

  it('goes back and says why when config.toml won’t take it', async () => {
    engine.answer('keep_show_started', () => {
      throw new Error('config.toml has an error');
    });
    const { w, said } = mounted(false);
    act(() => w.current[1](true));
    await answered();
    expect(w.current[0]).toBe(false);
    expect(said).toEqual(['Couldn’t keep that in config.toml: config.toml has an error']);
  });
});
