// What you wrote to a session and haven't sent, kept per session in local storage, which every
// window of Codealong shares: the floating bar's peek, the quick panel's reply view and a
// session's panel show the same draft. It outlasts the peek closing or moving to another session,
// the session changing its place on the bar, and a restart. Sending it clears it.

/** Each session's draft is its own item, so windows writing drafts for different sessions never
 *  write over each other's. */
const PREFIX = 'codealong.draft.';
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

/** Drops drafts left alone for a month. */
export function pruneDrafts(store: DraftStorage, now = Date.now()): void {
  try {
    const stale: string[] = [];
    for (let i = 0; i < store.length; i++) {
      const k = store.key(i);
      if (!k?.startsWith(PREFIX)) continue;
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

/** Calls `cb` with a session's draft when another window changes it: adds to it, or sends it. */
export function onDraftChange(key: string, cb: (text: string) => void): () => void {
  const listener = (e: StorageEvent) => {
    const text = draftChange(key, e);
    if (text !== null) cb(text);
  };
  window.addEventListener('storage', listener);
  return () => window.removeEventListener('storage', listener);
}
