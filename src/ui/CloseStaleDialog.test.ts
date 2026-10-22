import { describe, expect, it } from 'vitest';
import type { StaleOutcome } from '../types';
import { closedTabs, closeLabel } from './CloseStaleDialog';

const out = (closed: boolean): StaleOutcome => ({ key: 'ttys004:4821', title: 'T', closed, message: closed ? 'Closed.' : 'It’s the tab you’re in.' });

describe('closeLabel', () => {
  it('counts the ticked tabs', () => {
    expect(closeLabel(1)).toBe('Close 1 tab');
    expect(closeLabel(3)).toBe('Close 3 tabs');
  });
});

describe('closedTabs', () => {
  it('says how many closed, leaving out what didn’t', () => {
    expect(closedTabs([out(true), out(true), out(false)])).toBe('Closed 2 tabs.');
    expect(closedTabs([out(true)])).toBe('Closed 1 tab.');
    expect(closedTabs([out(false)])).toBe('');
  });
});
