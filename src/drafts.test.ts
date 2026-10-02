import { describe, expect, it } from 'vitest';
import { draftChange, pruneDrafts, putBack, readDraft, writeDraft, type DraftStorage } from './drafts';

/** Local storage as the app's windows share it, in memory. */
function memoryStorage(): DraftStorage & { items: Map<string, string> } {
  const items = new Map<string, string>();
  return {
    items,
    get length() {
      return items.size;
    },
    key: (i: number) => [...items.keys()][i] ?? null,
    getItem: (k: string) => items.get(k) ?? null,
    setItem: (k: string, v: string) => void items.set(k, v),
    removeItem: (k: string) => void items.delete(k),
  };
}

const DAY = 24 * 60 * 60 * 1000;

describe('drafts', () => {
  it('keeps each session its own draft, and drops it once empty', () => {
    const store = memoryStorage();
    writeDraft('claude:claude:a', 'half a thought', store);
    writeDraft('codex:codex:b', 'another', store);
    expect(readDraft('claude:claude:a', store)).toBe('half a thought');
    expect(readDraft('codex:codex:b', store)).toBe('another');
    expect(readDraft('claude:claude:c', store)).toBe('');
    writeDraft('claude:claude:a', '', store);
    expect(readDraft('claude:claude:a', store)).toBe('');
    expect([...store.items.keys()]).toEqual(['codealong.draft.codex:codex:b']);
  });

  it('keeps a draft in the window when storage refuses it', () => {
    const store = memoryStorage();
    writeDraft('claude:claude:a', 'older', store);
    const full: DraftStorage = {
      ...store,
      length: store.length,
      setItem: () => {
        throw new Error('QuotaExceededError');
      },
    };
    writeDraft('claude:claude:a', 'newer', full);
    expect(readDraft('claude:claude:a', full)).toBe('newer');
    // Once storage takes it again, the stored one is the draft.
    writeDraft('claude:claude:a', 'newest', store);
    expect(readDraft('claude:claude:a', store)).toBe('newest');
    writeDraft('claude:claude:x', 'no storage at all', null);
    expect(readDraft('claude:claude:x', null)).toBe('no storage at all');
  });

  it('drops drafts left alone for a month, and leaves other items be', () => {
    const store = memoryStorage();
    const now = 100 * DAY;
    writeDraft('claude:claude:old', 'from long ago', store, now - 31 * DAY);
    writeDraft('claude:claude:new', 'from yesterday', store, now - DAY);
    store.setItem('codealong.draft.junk', 'not json');
    store.setItem('codealong.bar.project', 'Acme');
    pruneDrafts(store, now);
    expect([...store.items.keys()].sort()).toEqual(['codealong.bar.project', 'codealong.draft.claude:claude:new']);
  });

  it('puts a prompt the session took back in an empty box, once', () => {
    const store = memoryStorage();
    const key = 'claude:claude:a';
    expect(putBack(key, '900:0', 'Who is the sponsor?', false, store)).toBe(true);
    expect(readDraft(key, store)).toBe('Who is the sponsor?');
    // Cleared, it stays clear: the same prompt doesn't come back by itself.
    writeDraft(key, '', store);
    expect(putBack(key, '900:0', 'Who is the sponsor?', false, store)).toBe(false);
    expect(readDraft(key, store)).toBe('');
    // What you're writing isn't replaced; asked for, it goes after it.
    writeDraft(key, 'Something new', store);
    expect(putBack(key, '1400:0', 'Another question', false, store)).toBe(false);
    expect(readDraft(key, store)).toBe('Something new');
    expect(putBack(key, '1400:0', 'Another question', true, store)).toBe(true);
    expect(readDraft(key, store)).toBe('Something new\n\nAnother question');
    // Once is enough: it's there already.
    expect(putBack(key, '1400:0', 'Another question', true, store)).toBe(false);
    // Its note goes when drafts do.
    pruneDrafts(store, Date.now() + 31 * DAY);
    expect(store.items.size).toBe(0);
  });

  it('hears another window change a session’s draft', () => {
    const k = 'codealong.draft.claude:claude:a';
    expect(draftChange('claude:claude:a', { key: k, newValue: JSON.stringify({ text: 'more', at: 1 }) })).toBe('more');
    // Sent from the other window.
    expect(draftChange('claude:claude:a', { key: k, newValue: null })).toBe('');
    // Another session's draft, or something else.
    expect(draftChange('claude:claude:a', { key: 'codealong.draft.claude:claude:b', newValue: '{"text":"x","at":1}' })).toBeNull();
    expect(draftChange('claude:claude:a', { key: 'codealong.bar.project', newValue: 'All projects' })).toBeNull();
    // Storage cleared.
    expect(draftChange('claude:claude:a', { key: null, newValue: null })).toBe('');
  });
});
