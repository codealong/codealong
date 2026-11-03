import { describe, expect, it } from 'vitest';
import { addToDraft, answersDraft, askedAs, draftChange, pruneDrafts, putBack, readAnswers, readDraft, sideDraft, writeDraft, writtenAnswers, type DraftStorage } from './drafts';
import type { Question } from './types';

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

  it('keeps what is written to a session’s side conversation apart from what is written to it', () => {
    const store = memoryStorage();
    writeDraft('claude:claude:a', 'for the session', store);
    writeDraft(sideDraft('claude:claude:a'), 'for the copy', store);
    expect(readDraft('claude:claude:a', store)).toBe('for the session');
    expect(readDraft(sideDraft('claude:claude:a'), store)).toBe('for the copy');
    // Another window hears each as its own.
    const e = { key: 'codealong.draft.' + sideDraft('claude:claude:a'), newValue: store.getItem('codealong.draft.' + sideDraft('claude:claude:a')) };
    expect(draftChange('claude:claude:a', e)).toBeNull();
    expect(draftChange(sideDraft('claude:claude:a'), e)).toBe('for the copy');
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

  it('adds words that are nowhere else, even ones what’s written has in it', () => {
    const store = memoryStorage();
    const key = 'claude:claude:b';
    addToDraft(key, 'Run tests', true, store);
    expect(readDraft(key, store)).toBe('Run tests');
    writeDraft(key, 'Do not run tests yet  \n', store);
    addToDraft(key, 'run tests', true, store);
    expect(readDraft(key, store)).toBe('Do not run tests yet\n\nrun tests');
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

  it('keeps your answers to a call’s questions apart from the box’s draft, for that call and those questions only', () => {
    const store = memoryStorage();
    const key = 'claude:claude:a';
    const color: Question = { question: 'Which color?', multiSelect: false, options: [{ label: 'Red' }, { label: 'Blue' }] };
    const days: Question = { question: 'Which days?', multiSelect: true, options: [{ label: 'Mon' }, { label: 'Tue' }] };
    const asked = askedAs([color, days]);
    const answers = { call: 'ask-1a2b', asked, picks: { 'Which days?': ['Mon', 'Tue'], 'Which color?': [] }, own: { 'Which color?': 'Green' }, at: 1 };
    writeDraft(key, 'for the session', store);
    writeDraft(answersDraft(key), writtenAnswers(answers), store);
    expect(readDraft(key, store)).toBe('for the session');
    expect(readAnswers(readDraft(answersDraft(key), store), 'ask-1a2b', asked)).toEqual(answers);
    // Another call's questions start afresh, though they ask the same.
    expect(readAnswers(readDraft(answersDraft(key), store), 'ask-3c4d', asked)).toBeNull();
    // So do the call's questions asked otherwise: other choices, or one taking several no longer.
    expect(readAnswers(readDraft(answersDraft(key), store), 'ask-1a2b', askedAs([{ ...color, options: [{ label: 'Green' }, { label: 'Yellow' }] }, days]))).toBeNull();
    expect(readAnswers(readDraft(answersDraft(key), store), 'ask-1a2b', askedAs([color, { ...days, multiSelect: false }]))).toBeNull();
    expect(readAnswers(readDraft(answersDraft(key), store), 'ask-1a2b', askedAs([color]))).toBeNull();
    // They go with drafts left alone for a month.
    pruneDrafts(store, Date.now() + 31 * DAY);
    expect(readDraft(answersDraft(key), store)).toBe('');
  });

  it('writes no answers while nothing is picked, written or gone on to', () => {
    const none = { call: 'ask-1a2b', asked: '[]' };
    expect(writtenAnswers({ ...none, picks: {}, own: {}, at: 0 })).toBe('');
    expect(writtenAnswers({ ...none, picks: {}, own: { 'Which color?': '' }, at: 0 })).toBe('');
    // Untouched again, a choice taken back is still an answer begun: a suggested pick stays away.
    expect(writtenAnswers({ ...none, picks: { 'Which days?': [] }, own: {}, at: 0 })).not.toBe('');
    expect(writtenAnswers({ ...none, picks: {}, own: {}, at: 1 })).not.toBe('');
  });

  it('reads what of kept answers it can', () => {
    expect(readAnswers('', 'c', 'a')).toBeNull();
    expect(readAnswers('not json', 'c', 'a')).toBeNull();
    expect(readAnswers('"a"', 'c', 'a')).toBeNull();
    expect(readAnswers(JSON.stringify({ call: 'c', asked: 'a', picks: { a: ['x', 2], b: 'y' }, own: { a: 'w', b: 3 }, at: -1 }), 'c', 'a')).toEqual({
      call: 'c',
      asked: 'a',
      picks: { a: ['x'] },
      own: { a: 'w' },
      at: 0,
    });
    expect(readAnswers(JSON.stringify({ call: 'c', asked: 'a' }), 'c', 'a')).toEqual({ call: 'c', asked: 'a', picks: {}, own: {}, at: 0 });
    // Kept before they were kept by call.
    expect(readAnswers(JSON.stringify({ asked: 'a' }), 'c', 'a')).toBeNull();
  });
});
