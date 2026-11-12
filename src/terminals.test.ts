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

  it('goes by the answer over an event that came before it was asked, which may have missed another', async () => {
    // Tauri hears focus before it hears blur: a blur while only focus was heard is lost.
    const t = await fresh();
    event(true);
    listening();
    answer(false);
    await Promise.resolve();
    expect(t.hasKeys()).toBe(false);
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

describe('a terminal’s text in a session’s box', () => {
  it('goes in a code block, without the blank lines at its ends, its fence longer than its backticks', async () => {
    const { boxedText } = await import('./terminals');
    expect(boxedText('\n\n  $ ls\n  a  b\n\n')).toEqual({ block: '```\n  $ ls\n  a  b\n```' });
    expect(boxedText('echo ```hi``` and ````')).toEqual({ block: '`````\necho ```hi``` and ````\n`````' });
    expect(boxedText(' \n\n')).toBeNull();
  });

  it('goes as a file past the lines a box takes', async () => {
    const { boxedText, BOX_LINES } = await import('./terminals');
    const lines = (n: number) => Array.from({ length: n }, (_, i) => `line ${i}`).join('\n');
    expect(boxedText(lines(BOX_LINES))).toHaveProperty('block');
    expect(boxedText(lines(BOX_LINES + 1))).toEqual({ file: `${lines(BOX_LINES + 1)}\n` });
  });

  it('is added to the end of what’s written to the session, a long one as an attached file', async () => {
    const { addToBox, BOX_LINES } = await import('./terminals');
    const api = await import('./api');
    const drafts = await import('./drafts');
    const saved = vi.spyOn(api, 'saveAttachment').mockResolvedValue({ kind: 'file', path: 'files/abc/terminal.txt', name: 'terminal.txt' });
    const added = vi.spyOn(drafts, 'addToDraft').mockImplementation(() => {});
    expect(await addToBox('s1', 'ok\n')).toBe(true);
    expect(added).toHaveBeenLastCalledWith('s1', '```\nok\n```');
    expect(await addToBox('s1', Array.from({ length: BOX_LINES + 5 }, () => 'x').join('\n'))).toBe(true);
    expect(saved).toHaveBeenCalledWith(expect.any(Blob), 'terminal.txt');
    expect(added).toHaveBeenLastCalledWith('s1', '[terminal.txt](files/abc/terminal.txt)');
    expect(await addToBox('s1', '\n')).toBe(false);
    expect(added).toHaveBeenCalledTimes(2);
  });
});

describe('a terminal that asks something', () => {
  const glance = (id: string, asks?: string, running = true) => ({ id, session: 's', title: '', shell: 'zsh', folder: '/Users/me/projects/northwind-app', startedAt: 0, running, asks });
  it('shows once as it starts asking, again for another question, and not while it goes on asking the same', async () => {
    const t = await fresh();
    const first = [glance('a', 'Continue? [y/N]'), glance('b'), glance('c', 'Name?', false)];
    expect(t.startedAsking(first, new Map()).map((g) => g.id)).toEqual(['a']);
    const before = t.askingNow(first);
    expect([...before]).toEqual([['a', 'Continue? [y/N]']]);
    expect(t.startedAsking(first, before)).toEqual([]);
    expect(t.startedAsking([glance('a', 'Password:'), glance('b', 'operator:')], before).map((g) => g.id)).toEqual(['a', 'b']);
  });
});
