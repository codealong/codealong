// lanes.ts works out again what the engine works out of a worktree's sessions: its bay, glyph and
// sentence, the order its sessions come in, each session's own sentence, and a few things said of
// a session that no field carries. It has to, where a filter leaves some sessions out or a change
// made in a window runs ahead of the engine (ahead.ts), and each rule is a copy of one in board.rs
// kept in step by hand. This holds the copies to worktrees the engine composed, with sessions in
// every state the rules tell apart: board.rs `rule_cases`, which writes testing/ruleCases.json
// and fails while it's stale.
import { describe, expect, it } from 'vitest';
import cases from './testing/ruleCases.json';
import { bySession, helpsAnAgent, needOf, relaned, sessionHeadline, unattended, waitsInBackground, waitsOnYou } from './lanes';
import type { Lane } from './types';

/** What the engine's own rules say of a session that it isn't sent with. */
interface Says {
  unattended: boolean;
  helpsAnAgent: boolean;
  waitsInBackground: boolean;
  waitsAtAPrompt: boolean;
}

const lanes = cases.lanes as unknown as Lane[];
const says = cases.says as Record<string, Says>;

describe('the windows work out what the engine does', () => {
  it('for worktrees in every bay', () => {
    expect(new Set(lanes.map((l) => l.bay))).toEqual(new Set(['needs', 'working', 'unread', 'review', 'idle', 'merged']));
  });

  for (const lane of lanes) {
    describe(`the worktree ${lane.callsign}`, () => {
      it('has the bay, glyph and sentence the engine gave it', () => {
        const again = relaned(lane, lane.sessions);
        expect({ bay: again.bay, glyph: again.glyph, headline: again.headline, detail: again.detail }).toEqual({ bay: lane.bay, glyph: lane.glyph, headline: lane.headline, detail: lane.detail });
      });

      // The sessions its status is worked out from (lanes.ts `inStatus`): the rest are listed
      // after them, and lanes.ts leaves them where the engine put them.
      it('orders its own sessions as the engine does', () => {
        const own = lane.sessions.filter((s) => !s.visitor && !s.archived && !s.earlier && !unattended(s));
        expect([...own].reverse().sort(bySession).map((s) => s.key)).toEqual(own.map((s) => s.key));
      });

      for (const s of lane.sessions) {
        it(`says of ${s.id} what the engine does`, () => {
          expect(sessionHeadline(s)).toEqual([s.headline, s.detail]);
          expect(needOf(s)).toEqual(s.need);
          expect({
            unattended: unattended(s),
            helpsAnAgent: helpsAnAgent(s),
            waitsInBackground: waitsInBackground(s),
            waitsAtAPrompt: waitsOnYou(s),
          }).toEqual(says[s.key]);
        });
      }
    });
  }
});
