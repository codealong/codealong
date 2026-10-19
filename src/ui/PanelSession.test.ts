import { describe, expect, it } from 'vitest';
import * as fx from '../testing/fixtures';
import { primaryFirst, type Place } from './PanelSession';

const place = (id: string, ...keys: string[]): Place => ({ lane: fx.lane({ id }), sessions: keys.map((key) => fx.session({ key })) });
const order = (places: Place[]) => places.map((p) => [p.lane.id, p.sessions.map((s) => s.key)]);

describe('primaryFirst', () => {
  it("puts the primary session's worktree first, and that session first in it", () => {
    const places = [place('held', 'a', 'b'), place('other', 'c'), place('main', 'd', 'e')];
    expect(order(primaryFirst(places, 'e'))).toEqual([
      ['main', ['e', 'd']],
      ['held', ['a', 'b']],
      ['other', ['c']],
    ]);
  });

  it('leaves the order as it is without a primary session, or one not listed', () => {
    const places = [place('held', 'a'), place('other', 'b')];
    expect(primaryFirst(places, null)).toBe(places);
    expect(primaryFirst(places, 'gone')).toBe(places);
  });
});
