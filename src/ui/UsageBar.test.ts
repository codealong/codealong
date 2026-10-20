import { describe, expect, it } from 'vitest';
import * as fx from '../testing/fixtures';
import type { Meter, Severity } from '../types';
import { headlineMeter, inOrder, listsInOrder, shouldCollapse } from './UsageBar';

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

describe('shouldCollapse', () => {
  it('collapses when every limit needs more room than there is', () => {
    expect(shouldCollapse(false, 1200, 1199)).toBe(true);
    expect(shouldCollapse(false, 1200, 1200)).toBe(false);
  });

  it('opens up again only with room to spare', () => {
    expect(shouldCollapse(true, 1200, 1210)).toBe(true);
    expect(shouldCollapse(true, 1200, 1216)).toBe(false);
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
