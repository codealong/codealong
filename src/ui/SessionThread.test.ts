import { describe, expect, it } from 'vitest';
import type { TranscriptItem } from '../types';
import { rowsOf } from './SessionThread';

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

  it('joins what a command you ran with ! printed to it, between the calls around it', () => {
    const ran = step('you', { id: 'b1', text: '! ls', input: 'ls', shell: true });
    const items = [call('a'), call('b'), call('c'), ran, step('result', { id: 'b1', output: 'a.txt' }), step('agent')];
    expect(shape(items)).toEqual([3, 'b1', 'agent']);
    const row = rowsOf(items)[1];
    expect(row.kind === 'item' && row.result?.output).toBe('a.txt');
  });
});
