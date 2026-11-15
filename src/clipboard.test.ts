import { afterEach, describe, expect, it, vi } from 'vitest';
import { copyText } from './clipboard';

/** A clipboard that, as WebKit's, waits for an item's promise and writes nothing when it fails;
 *  with `refuse`, one that refuses at once, not waiting for the item. */
function fakeClipboard(opts: { items?: boolean; refuse?: boolean } = {}) {
  const log: string[] = [];
  let held: string | null = 'before';
  class Item {
    constructor(readonly data: Record<string, Promise<Blob>>) {}
  }
  const clipboard = {
    writeText: async (text: string) => {
      log.push(`writeText ${text}`);
      if (opts.refuse) throw new DOMException('The request is not allowed', 'NotAllowedError');
      held = text;
    },
    write: async (items: Item[]) => {
      log.push('write');
      if (opts.refuse) throw new DOMException('The request is not allowed', 'NotAllowedError');
      const blob = await items[0]!.data['text/plain']!.catch(() => {
        throw new DOMException('The request is not allowed', 'NotAllowedError');
      });
      held = await blob.text();
    },
  };
  vi.stubGlobal('navigator', { clipboard: opts.items === false ? { writeText: clipboard.writeText } : clipboard });
  if (opts.items !== false) vi.stubGlobal('ClipboardItem', Item);
  return { log, held: () => held };
}

/** A read of the engine's that answers when told. */
function later() {
  let answer!: (text: string | null) => void;
  let fail!: (e: unknown) => void;
  const read = new Promise<string | null>((resolve, reject) => {
    answer = resolve;
    fail = reject;
  });
  return { read, answer, fail };
}

describe('copying text', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('hands the clipboard text still being read at once, before the read answers', async () => {
    const clip = fakeClipboard();
    const r = later();
    const copied = copyText(r.read);
    expect(clip.log, 'written while the key is handled').toEqual(['write']);
    r.answer('npm test');
    expect(await copied).toBe(true);
    expect(clip.held()).toBe('npm test');
  });

  it('leaves the clipboard alone when nothing was selected', async () => {
    const clip = fakeClipboard();
    expect(await copyText(Promise.resolve(null))).toBe(false);
    expect(clip.held()).toBe('before');
  });

  it('says why the read failed, not that the clipboard refused it', async () => {
    const clip = fakeClipboard();
    await expect(copyText(Promise.reject(new Error('No terminal t-4.')))).rejects.toThrow('No terminal t-4.');
    expect(clip.held()).toBe('before');
  });

  it('writes text it already has as it is', async () => {
    const clip = fakeClipboard();
    expect(await copyText('ls -la')).toBe(true);
    expect(clip.log).toEqual(['writeText ls -la']);
  });

  it('says in words that the clipboard refused it', async () => {
    fakeClipboard({ refuse: true });
    await expect(copyText('ls')).rejects.toThrow('Couldn’t copy: the clipboard refused it (NotAllowedError).');
    const r = later();
    const copied = copyText(r.read);
    // Refused before the read answers, which then has text to copy.
    await new Promise((resolve) => setTimeout(resolve, 0));
    r.answer('ls');
    await expect(copied).rejects.toThrow('Couldn’t copy: the clipboard refused it (NotAllowedError).');
  });

  it('says nothing of a clipboard that refused before the read came back with nothing to copy', async () => {
    const clip = fakeClipboard({ refuse: true });
    const r = later();
    const copied = copyText(r.read);
    await new Promise((resolve) => setTimeout(resolve, 0));
    r.answer(null);
    expect(await copied).toBe(false);
    expect(clip.held()).toBe('before');
  });

  it('waits for the text where the webview has no ClipboardItem', async () => {
    const clip = fakeClipboard({ items: false });
    const r = later();
    const copied = copyText(r.read);
    expect(clip.log).toEqual([]);
    r.answer('make');
    expect(await copied).toBe(true);
    expect(clip.held()).toBe('make');
  });
});
