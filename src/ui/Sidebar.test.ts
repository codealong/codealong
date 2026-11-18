import { afterEach, describe, expect, it } from 'vitest';
import { setPlatform } from '../platform';
import { analyticsTitle } from './Sidebar';

afterEach(() => setPlatform(null));

describe('analyticsTitle', () => {
  it('says the View menu’s key only where there is a View menu', () => {
    setPlatform('mac');
    expect(analyticsTitle()).toBe('Analytics: what sessions used and cost (⇧⌘U)');
    setPlatform('linux');
    expect(analyticsTitle()).toBe('Analytics: what sessions used and cost');
    setPlatform('windows');
    expect(analyticsTitle()).toBe('Analytics: what sessions used and cost');
  });
});
