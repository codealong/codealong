import { describe, expect, it } from 'vitest';
import type { TranscriptItem } from '../types';
import { editablePrompts, headingsOf, peerLabel, heardOf, rowsOf, scrolledUp, type Heard } from './SessionThread';

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

describe('scrolledUp', () => {
  // A view 500 tall over 2000 of conversation: by its sizes its end is at 1500, and its end is
  // everything from 1499 on, the real end being up to a pixel either side at a zoom.
  const view = (scrollTop: number, scrollHeight = 2000, clientHeight = 500) => ({ scrollTop, scrollHeight, clientHeight });
  const atEnd = heardOf(view(1500));

  it('counts a scroll up from the end however little, once it’s above the end', () => {
    expect(scrolledUp(view(1492), atEnd)).toBe(true);
    expect(scrolledUp(view(1498.5), atEnd)).toBe(true);
    // Up while more came in below, before the scroll was heard.
    expect(scrolledUp(view(1498.5, 2100), atEnd)).toBe(true);
  });

  it('not a move within the end, nor one down or none', () => {
    expect(scrolledUp(view(1499.5), atEnd)).toBe(false);
    expect(scrolledUp(view(1499), heardOf(view(1500.5)))).toBe(false);
    expect(scrolledUp(view(1500), heardOf(view(1492)))).toBe(false);
    expect(scrolledUp(view(1200), heardOf(view(1200)))).toBe(false);
  });

  it('not the end coming up to meet the view, as what was below grows shorter or the view taller', () => {
    expect(scrolledUp(view(1300, 1800), atEnd)).toBe(false);
    expect(scrolledUp(view(1299.5, 1800), atEnd)).toBe(false);
    expect(scrolledUp(view(1400, 2000, 600), atEnd)).toBe(false);
  });

  /** Whether the view follows after each of these scrolls from `from`, as a conversation's scroll
   *  tells it: near the end is at it, but not on the way up. */
  const follows = (from: Heard, tops: number[]) => {
    let heard = from;
    return tops.map((top) => {
      const el = view(top);
      const yes = !scrolledUp(el, heard) && el.scrollHeight - el.scrollTop - el.clientHeight < 40;
      heard = heardOf(el);
      return yes;
    });
  };

  it('keeps a view that went up off its end through every step up after, however small', () => {
    // At a zoom whose real end is half a pixel past where the sizes put it.
    const halves = Array.from({ length: 12 }, (_, i) => 1500.5 - (i + 1) * 0.5);
    expect(follows(heardOf(view(1500.5)), [...halves, 1499])).toEqual([true, true, true, ...Array(9).fill(false), true]);
  });

  it('not the way back from a bounce past either end, wherever the real end is, though a step up from past it is one', () => {
    for (const end of [1499, 1499.5, 1500, 1500.5]) expect(follows(heardOf(view(end)), [end + 10, end + 5, end + 0.5, end])).toEqual([true, true, true, true]);
    expect(follows(atEnd, [1510, 1498])).toEqual([true, false]);
    expect(scrolledUp(view(-5), heardOf(view(-10)))).toBe(false);
    expect(scrolledUp(view(-10), heardOf(view(0)))).toBe(false);
  });
});
