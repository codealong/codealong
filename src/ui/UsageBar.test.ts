import { describe, expect, it } from 'vitest';
import * as fx from '../testing/fixtures';
import type { Meter, Severity } from '../types';
import { chipClocksReset, fitFor, headlineMeter, inOrder, listsInOrder } from './UsageBar';

const meter = (key: string, percent: number, severity: Severity = 'normal'): Meter => fx.meter({ key, percent, severity });

describe('headlineMeter', () => {
  it('picks the fullest limit', () => {
    expect(headlineMeter([meter('5h', 38), meter('week', 67), meter('fable', 96, 'critical')])?.key).toBe('fable');
    expect(headlineMeter([meter('5h', 71), meter('week', 12)])?.key).toBe('5h');
  });

  it('goes by the harness warning before the percentage', () => {
    expect(headlineMeter([meter('5h', 74), meter('week', 70, 'warning')])?.key).toBe('week');
    expect(headlineMeter([meter('5h', 91, 'critical'), meter('week', 95, 'warning')])?.key).toBe('5h');
  });

  it('keeps the first of equals, and has nothing for no limits', () => {
    expect(headlineMeter([meter('5h', 0), meter('week', 0)])?.key).toBe('5h');
    expect(headlineMeter([])).toBeNull();
  });
});

describe('chipClocksReset', () => {
  const at = (percent: number, resetsAt: number | null = 1_800_000_000_000): Meter => ({ ...meter('week', percent), resetsAt });
  it('marks a limit’s reset with the clock only once the percent the chip shows is over 70', () => {
    expect(chipClocksReset(at(40))).toBe(false);
    expect(chipClocksReset(at(70))).toBe(false);
    expect(chipClocksReset(at(70.2))).toBe(false);
    expect(chipClocksReset(at(70.6))).toBe(true);
    expect(chipClocksReset(at(96))).toBe(true);
  });

  it('and never for a limit with no reset', () => {
    expect(chipClocksReset(at(96, null))).toBe(false);
  });
});

describe('fitFor', () => {
  const needs = { all: 1200, one: 800 };

  it('shows every limit while they fit, then one, then that one alone', () => {
    expect(fitFor('all', needs, 1200)).toBe('all');
    expect(fitFor('all', needs, 1199)).toBe('one');
    expect(fitFor('all', needs, 800)).toBe('one');
    expect(fitFor('all', needs, 799)).toBe('tight');
  });

  it('shows more again only with room to spare', () => {
    expect(fitFor('one', needs, 1210)).toBe('one');
    expect(fitFor('one', needs, 1216)).toBe('all');
    expect(fitFor('tight', needs, 810)).toBe('tight');
    expect(fitFor('tight', needs, 816)).toBe('one');
    expect(fitFor('tight', needs, 1216)).toBe('all');
  });

  it('shows less at once when the room goes', () => {
    expect(fitFor('one', needs, 799)).toBe('tight');
    expect(fitFor('all', needs, 500)).toBe('tight');
  });
});

describe('dragging a chip to another place', () => {
  it('shows the order it left until the board has it', () => {
    const board = ['claude', 'claude2', 'codex'].map((account) => fx.usage({ account }));
    expect(inOrder(board, ['codex', 'claude']).map((u) => u.account)).toEqual(['codex', 'claude', 'claude2']);
    expect(listsInOrder(board, ['codex', 'claude', 'claude2'])).toBe(false);
    expect(listsInOrder(board, ['claude', 'claude2', 'codex'])).toBe(true);
    // An account added or gone meanwhile doesn't keep it waiting.
    expect(listsInOrder(board, ['claude', 'gone', 'codex'])).toBe(true);
  });
});
