import { describe, expect, it } from 'vitest';
import type { TranscriptItem } from './types';
import { foldTurns, formatWorked, rowsOf, summarizeTools, turnFoldLabel, turnMatchesFind, type ToolCall } from './turnFolds';

let at = 0;
const step = (kind: TranscriptItem['kind'], more: Partial<TranscriptItem> = {}): TranscriptItem => ({ at: 0, kind, text: kind, failed: false, cursor: ++at, n: 0, ...more });
const call = (id: string, text = 'Bash · `ls`', more: Partial<TranscriptItem> = {}) => step('tool', { id, text, ...more });
const tc = (text: string, more: Partial<TranscriptItem> = {}, res?: Partial<TranscriptItem>): ToolCall => ({
  item: call(`c${++at}`, text, more),
  result: res ? step('result', res) : undefined,
});

describe('summarizeTools', () => {
  it('summarizes homogeneous and mixed tool runs in priority order', () => {
    expect(summarizeTools([tc('Bash · `ls`'), tc('Bash · `pwd`')])).toBe('Ran 2 commands');
    expect(summarizeTools([tc('Skill · `review`'), tc('Bash · `ls`'), tc('Bash · `git status`')])).toBe('Used a skill and ran 2 commands');
    expect(
      summarizeTools([
        tc('Read · `a.ts`', { file: '/r/a.ts' }),
        tc('Read · `a.ts`', { file: '/r/a.ts' }),
        tc('Edit · `b.ts`', { file: '/r/b.ts' }),
        tc('Edit · `c.ts`', { file: '/r/c.ts' }),
        tc('Bash · `npm test`'),
      ]),
    ).toBe('Edited 2 files, ran 1 command and 2 other actions');
  });

  it('notes failed calls at the end', () => {
    expect(summarizeTools([tc('Bash · `npm test`', {}, { failed: true }), tc('Bash · `git status`')])).toBe('Ran 2 commands · 1 failed');
  });
});

describe('formatWorked', () => {
  it('formats short and long turn durations concisely', () => {
    expect(formatWorked(500)).toBeNull();
    expect(formatWorked(12_000)).toBe('12s');
    expect(formatWorked(125_000)).toBe('2m');
    expect(formatWorked(3_900_000)).toBe('1h 5m');
    expect(formatWorked(3_600_000)).toBe('1h');
  });
});

describe('rowsOf in compact vs detailed modes', () => {
  it('folds runs of 2+ in compact mode and keeps failed calls separate', () => {
    const items = [
      call('a', 'Bash · `ls`'),
      call('b', 'Bash · `pwd`'),
      call('c', 'Bash · `bad`'),
      step('result', { id: 'c', failed: true, output: 'err' }),
      call('d', 'Read · `a.ts`'),
    ];
    const compact = rowsOf(items, 'compact');
    expect(compact.map((r) => r.kind)).toEqual(['tools', 'item', 'item']);
    expect(compact[0].kind === 'tools' && compact[0].calls.length).toBe(2);

    const detailed = rowsOf(items, 'detailed');
    expect(detailed.map((r) => r.kind)).toEqual(['tools']);
    expect(detailed[0].kind === 'tools' && detailed[0].calls.length).toBe(4);
  });
});

describe('foldTurns', () => {
  const MIN = 60_000;

  it('collapses a completed turn’s tools and intermediate replies into one fold above the final reply', () => {
    const items = [
      step('you', { at: 1 * MIN, text: 'Streamline my workflow' }),
      call('a', 'Skill · `codealong`', { at: 1 * MIN + 5_000 }),
      call('b', 'Bash · `python3 -c`', { at: 1 * MIN + 10_000 }),
      step('agent', { at: 2 * MIN, text: 'Reading your sessions now.' }),
      call('c', 'Bash · `sed -n`', { at: 2 * MIN + 10_000 }),
      call('d', 'Bash · `git log`', { at: 2 * MIN + 20_000 }),
      step('agent', { at: 3 * MIN, text: 'Here are the numbers.' }),
    ];
    const rows = foldTurns(rowsOf(items, 'folded'), { mode: 'folded', active: false });
    expect(rows.map((r) => r.kind)).toEqual(['item', 'turn', 'item']);
    const fold = rows[1];
    expect(fold.kind).toBe('turn');
    if (fold.kind === 'turn') {
      expect(fold.hidden).toBe(false);
      expect(fold.calls).toBe(4);
      expect(turnFoldLabel(fold)).toBe('Worked for 2m · Used a skill and ran 3 commands');
    }
    expect(rows[2].kind === 'item' && rows[2].item.text).toBe('Here are the numbers.');
  });

  it('leaves active turns and failed turns unfolded in folded mode', () => {
    const activeItems = [step('you', { at: MIN }), call('a', 'Bash · `ls`'), call('b', 'Bash · `pwd`'), step('agent', { at: 2 * MIN, text: 'Checking…' })];
    const activeRows = foldTurns(rowsOf(activeItems, 'folded'), { mode: 'folded', active: true });
    expect(activeRows.some((r) => r.kind === 'turn')).toBe(false);

    const failedItems = [
      step('you', { at: MIN }),
      call('a', 'Bash · `cargo test`'),
      step('result', { id: 'a', failed: true, output: 'FAILED' }),
      step('agent', { at: 2 * MIN, text: 'Test failed.' }),
    ];
    const failedRows = foldTurns(rowsOf(failedItems, 'folded'), { mode: 'folded', active: false });
    expect(failedRows.some((r) => r.kind === 'turn')).toBe(false);
  });

  it('keeps artifacts, sent files, and questions outside a turn fold', () => {
    const link = { url: 'https://claude.ai/artifact/1', title: 'Dashboard', note: 'v1' };
    const items = [
      step('you', { at: MIN }),
      call('a', 'Bash · `npm run build`'),
      call('b', 'Artifact · `dash.html`'),
      step('result', { id: 'b', link }),
      call('c', 'Bash · `git status`'),
      step('agent', { at: 2 * MIN, text: 'Published.' }),
    ];
    const rows = foldTurns(rowsOf(items, 'folded'), { mode: 'folded', active: false });
    expect(rows.map((r) => (r.kind === 'item' ? (r.item.id ?? r.item.kind) : r.kind))).toEqual(['you', 'turn', 'b', 'turn', 'agent']);
  });

  it('hides intermediate work in zen mode while keeping failures visible and ⌘F searchable', () => {
    const items = [
      step('you', { at: MIN }),
      call('a', 'Read · `src/lighthouse.ts`'),
      step('agent', { at: MIN + 10_000, text: 'Checking lighthouse.' }),
      call('b', 'Bash · `npm test`'),
      step('result', { id: 'b', failed: true, output: 'boom' }),
      step('agent', { at: 2 * MIN, text: 'Final reply.' }),
    ];
    const rows = foldTurns(rowsOf(items, 'zen'), { mode: 'zen', active: false });
    expect(rows.map((r) => (r.kind === 'item' ? (r.item.id ?? r.item.kind) : r.kind))).toEqual(['you', 'turn', 'b', 'agent']);
    const fold = rows[1];
    expect(fold.kind === 'turn' && fold.hidden).toBe(true);
    if (fold.kind === 'turn') {
      expect(turnMatchesFind(fold.rows, /lighthouse/i)).toBe(true);
      expect(turnMatchesFind(fold.rows, /unrelated/i)).toBe(false);
    }
  });
});
