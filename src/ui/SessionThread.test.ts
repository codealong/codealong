import { describe, expect, it } from 'vitest';
import type { TranscriptItem } from '../types';
import { editablePrompts, headingsOf, peerLabel, rowsOf } from './SessionThread';

let at = 0;
const step = (kind: TranscriptItem['kind'], more: Partial<TranscriptItem> = {}): TranscriptItem => ({ at: 0, kind, text: kind, failed: false, cursor: ++at, n: 0, ...more });
const call = (id: string, text = 'Bash · `ls`') => step('tool', { id, text });

/** Each row as a word: a line by its call's id (or its kind), a fold by how many calls it holds. */
const shape = (items: TranscriptItem[]) => rowsOf(items).map((r) => (r.kind === 'tools' ? r.calls.length : (r.item.id ?? r.item.kind)));

describe('rowsOf', () => {
  it('joins results to their calls and folds runs of three or more', () => {
    const items = [step('you'), call('a'), step('result', { id: 'a', output: 'ok' }), call('b'), step('agent'), call('c'), call('d'), call('e')];
    expect(shape(items)).toEqual(['you', 'a', 'b', 'agent', 3]);
    const rows = rowsOf(items);
    expect(rows[1].kind === 'item' && rows[1].result?.output).toBe('ok');
  });

  it('keeps a call that made a page out of a fold, with the calls around it folded', () => {
    const link = { url: 'https://claude.ai/artifact/Vuaw', title: 'Security dashboard', note: 'Updated · version 9' };
    const items = [call('a'), call('b'), call('c'), call('pub', 'Artifact · `security-dashboard.html`'), step('result', { id: 'pub', link }), call('d'), call('e'), call('f'), call('g')];
    expect(shape(items)).toEqual([3, 'pub', 4]);
    const made = rowsOf(items)[1];
    expect(made.kind === 'item' && made.result?.link).toEqual(link);
    // Its result can come in a later page than the call: until then it folds like any other.
    expect(shape(items.filter((it) => it.kind !== 'result'))).toEqual([8]);
  });

  it('keeps a call that sent you files out of a fold, before its result comes in too', () => {
    const sent = { files: ['/Users/me/app/tmp/teaser.mp4'], caption: '24 s, no audio' };
    const items = [call('a'), call('b'), call('c'), step('tool', { id: 'send', text: 'SendUserFile · `teaser.mp4`', sent }), call('d'), call('e'), call('f')];
    expect(shape(items)).toEqual([3, 'send', 3]);
  });

  it('joins what a command you ran with ! printed to it, between the calls around it', () => {
    const ran = step('you', { id: 'b1', text: '! ls', input: 'ls', shell: true });
    const items = [call('a'), call('b'), call('c'), ran, step('result', { id: 'b1', output: 'a.txt' }), step('agent')];
    expect(shape(items)).toEqual([3, 'b1', 'agent']);
    const row = rowsOf(items)[1];
    expect(row.kind === 'item' && row.result?.output).toBe('a.txt');
  });
});

describe('headingsOf', () => {
  const MIN = 60_000;
  /** Each message as its kind, upper case where it shows who said it. */
  const heads = (items: TranscriptItem[]) => {
    const { headed } = headingsOf(items);
    return items.filter((it) => it.kind === 'you' || it.kind === 'agent').map((it) => (headed.has(`${it.cursor}:${it.n}`) ? it.kind.toUpperCase() : it.kind));
  };

  it('heads the reply to a prompt once, the calls between its messages being its own', () => {
    const items = [step('you', { at: MIN }), step('agent', { at: 2 * MIN }), call('a'), step('result', { id: 'a', at: 3 * MIN }), step('agent', { at: 4 * MIN }), call('b'), step('agent', { at: 5 * MIN }), step('you', { at: 6 * MIN }), step('agent', { at: 7 * MIN })];
    expect(heads(items)).toEqual(['YOU', 'AGENT', 'agent', 'agent', 'YOU', 'AGENT']);
  });

  it('heads each message from another session with whose it is, and the reply after it', () => {
    const items = [step('agent', { at: MIN }), step('peer', { at: 2 * MIN, from: 'Review the parser' }), step('peer', { at: 2 * MIN + 1 }), step('agent', { at: 3 * MIN })];
    const { headed } = headingsOf(items);
    expect(items.map((it) => headed.has(`${it.cursor}:${it.n}`))).toEqual([true, true, true, true]);
    expect(items.slice(1, 3).map(peerLabel)).toEqual(['From “Review the parser”', 'From another session']);
  });

  it('heads a message again after a pause, a note, or a prompt taken back', () => {
    // Woken by a background job's report long after its last call finished.
    expect(heads([step('agent', { at: MIN }), call('a', undefined), step('result', { id: 'a', at: 2 * MIN }), step('agent', { at: 13 * MIN })])).toEqual(['AGENT', 'AGENT']);
    // A long call isn't a pause: what it gave back came just before.
    expect(heads([step('agent', { at: MIN }), step('result', { id: 'a', at: 30 * MIN }), step('agent', { at: 31 * MIN })])).toEqual(['AGENT', 'agent']);
    expect(heads([step('agent', { at: MIN }), step('note', { at: 2 * MIN }), step('agent', { at: 3 * MIN })])).toEqual(['AGENT', 'AGENT']);
    expect(heads([step('you', { at: MIN }), step('you', { at: 2 * MIN, withdrawn: true }), step('you', { at: 3 * MIN })])).toEqual(['YOU', 'YOU', 'you']);
    // A step whose record gave no time doesn't make one.
    expect(heads([step('agent', { at: 0 }), step('agent', { at: 20 * MIN })])).toEqual(['AGENT', 'agent']);
  });

  it('dates a message its record gave no time by the last one said before it, else not at all', () => {
    // Cursor's command line transcript: only prompts say when.
    const items = [step('agent'), step('you', { at: 5 * MIN }), step('agent'), call('a'), step('note'), step('agent')];
    const { headed } = headingsOf(items);
    expect(items.filter((it) => it.kind !== 'tool' && it.kind !== 'note').map((it) => headed.get(`${it.cursor}:${it.n}`))).toEqual([0, 5 * MIN, 5 * MIN, 5 * MIN]);
  });

  it('says who spoke last, for a message streaming in', () => {
    expect(headingsOf([step('you', { at: MIN }), step('agent', { at: 2 * MIN }), call('a', undefined)]).last).toEqual({ kind: 'agent', at: 2 * MIN });
    expect(headingsOf([]).last).toEqual({ kind: null, at: 0 });
  });
});

describe('editablePrompts', () => {
  const offered = (items: TranscriptItem[], fromStart: boolean) => items.map((it) => editablePrompts(items, fromStart).has(`${it.cursor}:${it.n}`));

  it('offers Edit from here on your prompts with something of the agent’s before them', () => {
    const items = [step('you'), step('agent'), step('you'), call('a'), step('you', { withdrawn: true }), step('you', { input: 'ls', text: '! ls' }), step('you', { text: ' ' }), step('peer'), step('you')];
    expect(offered(items, true)).toEqual([false, false, true, false, false, false, false, false, true]);
  });

  it('on the first prompt of a page too, when the page isn’t the session’s start', () => {
    expect(offered([step('you'), step('agent')], false)).toEqual([true, false]);
  });
});
