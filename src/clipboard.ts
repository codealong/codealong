// Putting text on the clipboard, including text that's still to be read from the engine (a
// terminal's selection, say). WebKit takes a write only while it's handling the key or click that
// asked for it: in a WKWebView (2026-11-15), `writeText` after a round trip of half a second or
// more was refused with NotAllowedError, though one of 50 ms got through. A `ClipboardItem` made
// from the round trip's promise, written while the key is handled, was taken after any wait
// tried (up to 3 s), and one whose promise failed left the clipboard as it was. So text still
// to come goes in as a promise, at once.

/** Puts `text` on the clipboard. Text still being read is handed over as a promise while the key
 *  or click is handled, so call this from its handler, not after awaiting anything. Resolves
 *  false when the text came as nothing (nothing selected), leaving the clipboard as it was;
 *  rejects with the read's error when the read failed. */
export async function copyText(text: string | Promise<string | null>): Promise<boolean> {
  if (typeof text === 'string') {
    await navigator.clipboard.writeText(text).catch(refused);
    return true;
  }
  if (typeof ClipboardItem === 'undefined' || typeof navigator.clipboard.write !== 'function') {
    // A webview without it (an older WebKitGTK) gets what the key's handler can do: the text
    // once it's read.
    const read = await text;
    if (!read) return false;
    await navigator.clipboard.writeText(read).catch(refused);
    return true;
  }
  // Nothing read fails the item, so the clipboard keeps what it had.
  const blob = text.then((read) => {
    if (!read) throw new Error('Nothing to copy.');
    return new Blob([read], { type: 'text/plain' });
  });
  // A clipboard that refuses at once never waits for the item; what the read came to is said
  // from `text` below.
  blob.catch(() => {});
  try {
    await navigator.clipboard.write([new ClipboardItem({ 'text/plain': blob })]);
    return true;
  } catch (e) {
    // What the read came to says which it was, once it's in: a clipboard can refuse before it
    // is. A read that failed says why better than the clipboard's refusal of it.
    if (!(await text)) return false;
    return refused(e);
  }
}

function refused(e: unknown): never {
  throw new Error(`Couldn’t copy: the clipboard refused it${e instanceof DOMException ? ` (${e.name})` : ''}.`);
}
