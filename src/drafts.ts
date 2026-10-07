// What you wrote to a session and haven't sent, kept per session in local storage, which every
// window of Codealong shares: the floating bar's peek, the waiting window and a
// session's panel show the same draft. It outlasts the peek closing or moving to another session,
// the session changing its place on the bar, and a restart. Sending it clears it.

/** Each session's draft is its own item, so windows writing drafts for different sessions never
 *  write over each other's. */
const PREFIX = 'codealong.draft.';
/** The last prompt put back in a session's box, so each goes back once: a box you cleared of it
 *  stays clear. */
const PUT_BACK = 'codealong.putback.';
/** Tells this window's boxes that a prompt was put back; other windows hear `storage`. */
const HERE = 'codealong-draft';
/** A draft nobody has touched for this long is dropped: its session has most likely ended. */
const KEEP_MS = 30 * 24 * 60 * 60 * 1000;

/** The part of `Storage` drafts use, so tests can hand in their own. */
export type DraftStorage = Pick<Storage, 'getItem' | 'setItem' | 'removeItem' | 'key' | 'length'>;

interface Stored {
  text: string;
  /** When it was last written, in ms. */
  at: number;
}

/** Drafts for a window whose storage can't be used: they last as long as the window. */
const unstored = new Map<string, string>();
let pruned = false;

function storage(): DraftStorage | null {
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

function parse(raw: string | null): Stored | null {
  if (raw == null) return null;
  try {
    const v: unknown = JSON.parse(raw);
    if (typeof v === 'object' && v !== null && typeof (v as Stored).text === 'string') {
      const at = (v as Stored).at;
      return { text: (v as Stored).text, at: typeof at === 'number' ? at : 0 };
    }
  } catch {
    // Not one of ours.
  }
  return null;
}

/** The draft for a session, or ''. One this window couldn't store is newer than any stored. */
export function readDraft(key: string, store: DraftStorage | null = storage()): string {
  const kept = unstored.get(key);
  if (kept !== undefined || !store) return kept ?? '';
  try {
    return parse(store.getItem(PREFIX + key))?.text ?? '';
  } catch {
    return '';
  }
}

/** Keeps what's written to a session; an empty text drops its draft. */
export function writeDraft(key: string, text: string, store: DraftStorage | null = storage(), now = Date.now()): void {
  if (store) {
    if (!pruned) {
      pruned = true;
      pruneDrafts(store, now);
    }
    try {
      const stored: Stored = { text, at: now };
      if (text) store.setItem(PREFIX + key, JSON.stringify(stored));
      else store.removeItem(PREFIX + key);
      unstored.delete(key);
      return;
    } catch {
      // Storage is full or unavailable: this window keeps it at least.
    }
  }
  if (text) unstored.set(key, text);
  else unstored.delete(key);
}

/** Where what's written to a session's side conversation (side.ts) is kept: a draft apart from
 *  the session's own, so what was typed for the copy is never sent to the session, from this
 *  window or another, and the session's draft is there again once the side conversation is put
 *  away. */
export function sideDraft(key: string): string {
  return `side:${key}`;
}

/** A prompt the session took back before its agent saw it, put back in its box as Claude Code
 *  puts it back in its own (claude/conversation.rs): once for each prompt (`id` names it), and only when the
 *  box is empty, unless you asked for it (`asked`), which adds it after what's there. Says
 *  whether it put it there. */
export function putBack(key: string, id: string, text: string, asked = false, store: DraftStorage | null = storage(), now = Date.now()): boolean {
  if (!asked && store) {
    try {
      if (parse(store.getItem(PUT_BACK + key))?.text === id) return false;
      store.setItem(PUT_BACK + key, JSON.stringify({ text: id, at: now } satisfies Stored));
    } catch {
      // Without storage it may come back once more, in another window.
    }
  }
  const draft = readDraft(key, store);
  const tell = (t: string) => typeof window !== 'undefined' && window.dispatchEvent(new CustomEvent(HERE, { detail: { key, text: t, focus: asked } }));
  if (draft.includes(text)) {
    // Asked for when it's there already, the box takes the cursor.
    if (asked) tell(draft);
    return false;
  }
  if (draft.trim() && !asked) return false;
  const next = draft.trim() ? `${draft.trimEnd()}\n\n${text}` : text;
  writeDraft(key, next, store, now);
  tell(next);
  return true;
}

/** Drops drafts, and notes of prompts put back, left alone for a month. */
export function pruneDrafts(store: DraftStorage, now = Date.now()): void {
  try {
    const stale: string[] = [];
    for (let i = 0; i < store.length; i++) {
      const k = store.key(i);
      if (!k?.startsWith(PREFIX) && !k?.startsWith(PUT_BACK)) continue;
      const v = parse(store.getItem(k));
      if (!v || now - v.at > KEEP_MS) stale.push(k);
    }
    for (const k of stale) store.removeItem(k);
  } catch {
    // Nothing to tidy when storage can't be read.
  }
}

/** The draft a storage event carries for a session: its new text, or null when the event is
 *  about something else. An event with no key means storage was cleared. */
export function draftChange(key: string, e: { key: string | null; newValue: string | null }): string | null {
  if (e.key === null) return '';
  if (e.key !== PREFIX + key) return null;
  return parse(e.newValue)?.text ?? '';
}

/** Calls `cb` with a session's draft when another window changes it (adds to it, or sends it),
 *  or a prompt is put back in it. `focus`: you asked for it here, so the box takes the cursor. */
export function onDraftChange(key: string, cb: (text: string, focus: boolean) => void): () => void {
  const listener = (e: StorageEvent) => {
    const text = draftChange(key, e);
    if (text !== null) cb(text, false);
  };
  const here = (e: Event) => {
    const d = (e as CustomEvent<{ key: string; text: string; focus: boolean }>).detail;
    if (d.key === key) cb(d.text, d.focus);
  };
  window.addEventListener('storage', listener);
  window.addEventListener(HERE, here);
  return () => {
    window.removeEventListener('storage', listener);
    window.removeEventListener(HERE, here);
  };
}
