import { describe, expect, it } from 'vitest';
import { CLAIM_FOR_MS, claimed, TIP_AFTER_MS, TIP_EVERY_MS, turnOf, workingTip, type TipClaim } from './working';

const T0 = 1_760_000_000_000;

describe('workingTip', () => {
  const turn = turnOf('s1', T0 - 60_000);
  const claim = (over: Partial<TipClaim>): TipClaim => ({ turn: turnOf('s0', T0 - 3_600_000), id: 'find', at: T0 - TIP_EVERY_MS, ...over });

  it('gives none in the first moments of a turn', () => {
    expect(workingTip([], turn, TIP_AFTER_MS - 1, T0)).toBe(null);
    expect(workingTip([], turn, TIP_AFTER_MS, T0)).toBe('take');
  });
  it('gives none for a while after the last, wherever it showed', () => {
    expect(workingTip([claim({ at: T0 - TIP_EVERY_MS + 1 })], turn, 60_000, T0)).toBe(null);
    expect(workingTip([claim({ at: T0 - TIP_EVERY_MS })], turn, 60_000, T0)).toBe('take');
    expect(workingTip([claim({ at: T0 - TIP_EVERY_MS }), claim({ turn: 'b:1', at: T0 - 1_000 })], turn, 60_000, T0)).toBe(null);
  });
  it('keeps the tip a turn got for as long as it goes on', () => {
    const got = claim({ turn, at: T0 - 1_000 });
    expect(workingTip([got], turn, 0, T0)).toEqual({ keep: got });
    const old = claim({ turn, at: T0 - 2 * TIP_EVERY_MS });
    expect(workingTip([old], turn, 10 * TIP_EVERY_MS, T0)).toEqual({ keep: old });
  });
  it('keeps it though other turns got theirs since, for hours', () => {
    const got = claim({ turn, id: 'find', at: T0 });
    let claims = [got];
    for (let i = 1; i <= 20; i++) claims = claimed(claims, claim({ turn: `b${i}:1`, id: 'side', at: T0 + i * TIP_EVERY_MS }));
    expect(workingTip(claims, turn, 21 * TIP_EVERY_MS, T0 + 20 * TIP_EVERY_MS + 1_000)).toEqual({ keep: got });
  });
  it('takes one when the last says it showed later than now (the clock went back)', () => {
    expect(workingTip([claim({ at: T0 + 60_000 })], turn, 60_000, T0)).toBe('take');
  });
});

describe('claimed', () => {
  it('puts the claim last, once for its turn', () => {
    const a = { turn: 'a:1', id: 'find', at: 1 };
    const b = { turn: 'b:1', id: 'side', at: 2 };
    expect(claimed([a], b)).toEqual([a, b]);
    expect(claimed([a, b], { turn: 'a:1', id: 'fork', at: 3 })).toEqual([b, { turn: 'a:1', id: 'fork', at: 3 }]);
  });
  it('lets go of claims older than any turn, and any from a clock gone back', () => {
    const old = { turn: 'a:1', id: 'find', at: T0 - CLAIM_FOR_MS };
    const recent = { turn: 'b:1', id: 'side', at: T0 - CLAIM_FOR_MS + 1 };
    const ahead = { turn: 'c:1', id: 'fork', at: T0 + 60_000 };
    expect(claimed([old, recent, ahead], { turn: 'd:1', id: 'find', at: T0 }).map((c) => c.turn)).toEqual(['b:1', 'd:1']);
  });
});
