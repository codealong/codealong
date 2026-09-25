import { describe, expect, it } from 'vitest';
import { landingLabel, riskLabel } from './insights';

describe('landingLabel', () => {
  it('names how far the work got, and nothing for no change', () => {
    expect(landingLabel({ landing: 'pushed' })).toMatchObject({ label: 'Pushed', tone: 'info' });
    expect(landingLabel({ landing: 'uncommitted' })?.label).toBe('Not committed');
    expect(landingLabel({ landing: 'none' })).toBeNull();
    expect(landingLabel(undefined)).toBeNull();
  });
});

describe('riskLabel', () => {
  it('reads the score to the nearest level, only while the session waits', () => {
    expect(riskLabel({ status: 'needs', insight: { risk: 0.2 } })?.label).toBe('Reads only');
    expect(riskLabel({ status: 'needs', insight: { risk: 2.6 } })).toMatchObject({ label: 'Destructive', tone: 'bad' });
    expect(riskLabel({ status: 'needs', insight: { risk: 1.4 } })?.label).toBe('Local changes');
    expect(riskLabel({ status: 'working', insight: { risk: 3 } })).toBeNull();
    expect(riskLabel({ status: 'needs', insight: {} })).toBeNull();
  });
});
