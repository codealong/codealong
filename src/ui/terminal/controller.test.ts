import { describe, expect, it, vi } from 'vitest';
import type { TerminalInput } from '../../types';

/** Every call the controller makes, in the order the server would take them, each held until
 *  the test lets it go. */
const sent: string[] = [];
const held: (() => void)[] = [];
const hold = (what: string, answer: unknown = undefined) =>
  new Promise((resolve) => {
    sent.push(what);
    held.push(() => resolve(answer));
  });

vi.mock('../../api', async (original) => ({
  ...(await original<typeof import('../../api')>()),
  terminalInput: vi.fn((_id: string, inputs: TerminalInput[]) => hold(inputs.map((i) => (i.kind === 'key' ? i.text : i.kind)).join(''))),
  terminalPaste: vi.fn((_id: string, text: string, force: boolean) => hold(`paste:${text}${force ? '!' : ''}`, !/\n/.test(text) || force)),
}));

const { TerminalController } = await import('./controller');

const key = (text: string): TerminalInput => ({ kind: 'key', code: 'KeyA', text, mods: {} });

/** Lets the call in flight answer, and waits for the next to go. */
async function answer() {
  held.shift()?.();
  await new Promise((r) => setTimeout(r, 0));
}

describe('what a window sends a terminal', () => {
  it('goes in the order it came, a paste among the keys, one call at a time', async () => {
    const unsafe: string[] = [];
    const c = new TerminalController('t', {} as HTMLElement, {} as HTMLCanvasElement, { onLeave: () => {}, onUnsafePaste: (t) => unsafe.push(t), onError: () => {}, onPointer: () => {} });
    c.send(key('a'));
    // While the first call is out, more keys and a paste wait their turn.
    c.send(key('b'));
    c.paste('p');
    c.send(key('c'));
    c.send(key('d'));
    expect(sent).toEqual(['a']);
    await answer();
    expect(sent).toEqual(['a', 'b']);
    await answer();
    expect(sent).toEqual(['a', 'b', 'paste:p']);
    await answer();
    expect(sent, 'the keys after it together, after it').toEqual(['a', 'b', 'paste:p', 'cd']);
    await answer();
    // A paste that could run commands comes back to be asked about, and goes once allowed.
    c.paste('rm x\nls');
    await answer();
    expect(unsafe).toEqual(['rm x\nls']);
    c.paste('rm x\nls', true);
    await answer();
    expect(sent[sent.length - 1]).toBe('paste:rm x\nls!');
  });
});
