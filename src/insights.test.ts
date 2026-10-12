import { describe, expect, it } from 'vitest';
import { landingLabel, readBy, riskLabel } from './insights';

describe('landingLabel', () => {
  it('names how far the work got, and nothing for no change', () => {
    expect(landingLabel({ landing: 'pushed' })).toMatchObject({ label: 'Pushed', tone: 'info' });
    expect(landingLabel({ landing: 'uncommitted' })?.label).toBe('Not committed');
    expect(landingLabel({ landing: 'none' })).toBeNull();
    expect(landingLabel(undefined)).toBeNull();
  });

  it('says who read it: Jev, or the agent that tagged it, with its note', () => {
    expect(landingLabel({ landing: 'pushed' })?.title).toBe('Jev’s read of the last reply: the change is pushed to a branch that isn’t merged yet.');
    const tagged = landingLabel({ landing: 'landed', by: 'claude:claude:boss', byTitle: 'Orchestrator', note: 'PR #12 merged' });
    expect(tagged?.title).toBe('Tagged by Orchestrator of the last reply: the change is merged or on the main branch. PR #12 merged.');
    expect(readBy({ by: 'claude:claude:boss' })).toBe('Tagged by an agent');
    expect(riskLabel({ status: 'needs', insight: { risk: 3, by: 'k', byTitle: 'Orchestrator' } })?.title).toContain('Tagged by Orchestrator of the command');
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
