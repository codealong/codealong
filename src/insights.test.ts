import { describe, expect, it } from 'vitest';
import { endingLabel, finishedLabel, landingLabel, readBy, riskLabel } from './insights';

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

describe('finishedLabel', () => {
  it('says a session is finished with why, and only looks so when just its reply says it', () => {
    const sure = finishedLabel({ status: 'unread', finished: { sure: true, why: ['#12 merged', 'its todo is done'] } });
    expect(sure).toMatchObject({ label: 'Finished', tone: 'good' });
    expect(sure?.title).toBe('Its work is done and nothing is left for you: #12 merged, its todo is done. Mark it as done once you’ve read it.');
    const looks = finishedLabel({ status: 'idle', finished: { sure: false, why: ['its reply reads as done'] } });
    expect(looks).toMatchObject({ label: 'Looks finished', tone: 'quiet' });
    expect(looks?.title).toContain('its reply reads as done. Nothing else says so');
    expect(finishedLabel({ status: 'unread' })).toBeNull();
  });

  it('says nothing of one a window set working ahead of the engine', () => {
    const finished = { sure: true, why: ['#12 merged'] };
    expect(finishedLabel({ status: 'working', finished })).toBeNull();
    expect(finishedLabel({ status: 'needs', finished })).toBeNull();
    expect(finishedLabel({ status: 'stopped', finished })?.label).toBe('Finished');
  });
});

describe('endingLabel', () => {
  it('says how a turn that’s yours to read leaves things with you, where that says more', () => {
    expect(endingLabel({ status: 'unread', insight: { ending: 'handoff' } })).toMatchObject({ label: 'Next step is yours', tone: 'info' });
    expect(endingLabel({ status: 'unread', insight: { ending: 'waiting' } })?.label).toBe('Waits on its own work');
    expect(endingLabel({ status: 'unread', insight: { ending: 'offer' } })?.label).toBe('Offers more');
    expect(endingLabel({ status: 'unread', insight: { ending: 'handoff', by: 'k', byTitle: 'Orchestrator', note: 'Merge #7 when ready' } })?.title).toBe(
      'Tagged by Orchestrator of the last reply: its work is done, and it leaves you a next step to take yourself. Merge #7 when ready.',
    );
    // A report says nothing more than Your turn does, and one that asks is Needs you, which says so.
    for (const ending of ['report', 'question', 'blocked'] as const) expect(endingLabel({ status: 'unread', insight: { ending } })).toBeNull();
    expect(endingLabel({ status: 'unread', insight: {} })).toBeNull();
  });

  it('only while it’s Your turn, and not beside Finished', () => {
    expect(endingLabel({ status: 'idle', insight: { ending: 'handoff' } })).toBeNull();
    expect(endingLabel({ status: 'working', insight: { ending: 'waiting' } })).toBeNull();
    expect(endingLabel({ status: 'unread', insight: { ending: 'offer' }, finished: { sure: true, why: ['#12 merged'] } })).toBeNull();
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
