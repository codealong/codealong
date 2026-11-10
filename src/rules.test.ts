// lanes.ts works out again what the engine works out of a worktree's sessions: its bay, glyph and
// sentence, the order its sessions come in, each session's own sentence, and a few things said of
// a session that no field carries. It has to, where a filter leaves some sessions out or a change
// made in a window runs ahead of the engine (ahead.ts), and each rule is a copy of one in board.rs
// kept in step by hand. This holds the copies to worktrees the engine composed, with sessions in
// every state the rules tell apart, and to what it says of each worktree with one of its
// sessions left out: board.rs `rule_cases`, which writes testing/ruleCases.json and fails while
// it's stale.
import { describe, expect, it } from 'vitest';
import cases from './testing/ruleCases.json';
import { bySession, helpsAnAgent, needOf, relaned, sessionHeadline, unattended, waitsInBackground, waitsOnYou } from './lanes';
import type { Bay, Lane, Session, Status } from './types';

/** What the engine's own rules say of a session that it isn't sent with. */
interface Says {
  unattended: boolean;
  helpsAnAgent: boolean;
  waitsInBackground: boolean;
  waitsAtAPrompt: boolean;
}

/** A worktree as the engine has it once one of its sessions is gone. */
interface Without {
  lane: string;
  session: string;
  bay: Bay;
  glyph: Status;
  headline: string;
  detail: string | null;
}

const lanes = cases.lanes as unknown as Lane[];
const says = cases.says as Record<string, Says>;
const without = cases.without as Without[];

const said = (l: Pick<Lane, 'bay' | 'glyph' | 'headline' | 'detail'>) => ({ bay: l.bay, glyph: l.glyph, headline: l.headline, detail: l.detail });
/** The sessions a worktree's status is worked out from (lanes.ts `inStatus`). */
const own = (sessions: Session[]) => sessions.filter((s) => !s.visitor && !s.archived && !s.earlier && !unattended(s));

describe('the windows work out what the engine does', () => {
  it('for worktrees in every bay', () => {
    expect(new Set(lanes.map((l) => l.bay))).toEqual(new Set(['needs', 'working', 'unread', 'review', 'idle', 'merged']));
  });

  for (const lane of lanes) {
    describe(`the worktree ${lane.callsign}`, () => {
      // Whatever order its sessions are handed over in: a change made here leaves them where
      // they were, and the sentence is the first one's in the engine's order.
      it('has the bay, glyph and sentence the engine gave it', () => {
        expect(said(relaned(lane, [...lane.sessions].reverse()))).toEqual(said(lane));
      });

      // The rest are listed after its own, and lanes.ts leaves them where the engine put them.
      it('orders its own sessions as the engine does', () => {
        const theirs = own(lane.sessions);
        expect([...theirs].reverse().sort(bySession).map((s) => s.key)).toEqual(theirs.map((s) => s.key));
      });

      for (const s of lane.sessions.filter((s) => !s.visitor)) {
        it(`says of ${s.id} what the engine does`, () => {
          expect(sessionHeadline(s)).toEqual([s.headline, s.detail]);
          expect({
            unattended: unattended(s),
            helpsAnAgent: helpsAnAgent(s),
            waitsInBackground: waitsInBackground(s),
            waitsAtAPrompt: waitsOnYou(s),
          }).toEqual(says[s.key]);
        });

        // What it needs you for is the engine's word, but for what the session says outright,
        // which is read here too: with the engine's word gone, stuck and a reply that asks are
        // still told, and nothing else is guessed; and once it works again, it needs nothing,
        // whatever it was last said to wait on.
        it(`tells what ${s.id} needs you for as the engine does`, () => {
          expect(needOf(s)).toEqual(s.need);
          const outright = s.need === 'stuck' || s.need === 'reply' ? s.need : undefined;
          expect(needOf({ ...s, need: undefined })).toEqual(outright);
          expect(needOf({ ...s, need: 'approval' })).toEqual(s.status !== 'needs' && s.need !== 'stuck' ? undefined : (outright ?? 'approval'));
          expect(needOf({ ...s, status: 'working', stalledSince: null })).toBeUndefined();
          // Nor is it still stuck, or asking in its reply, once the session no longer says so.
          for (const stale of ['stuck', 'reply'] as const) expect(needOf({ ...s, status: 'needs', asks: false, stalledSince: null, need: stale })).toBeUndefined();
        });
      }

      // A filter leaves a session out, and the worktree is what its others make it. A PR's own
      // sentence is the engine's to word (its checks, its reviews, whose session is still open),
      // and lanes.ts doesn't work it out again: a worktree that's its PR's keeps the sentence the
      // engine gave it, and one that comes to be says it more plainly until the engine does. So
      // of a worktree left as its PR's, only the bay and glyph are held.
      for (const left of without.filter((w) => w.lane === lane.id)) {
        it(`is what the engine makes it without ${left.session}`, () => {
          const again = relaned(lane, lane.sessions.filter((s) => s.key !== left.session).reverse());
          const itsPrs = left.bay === 'review' || left.bay === 'merged';
          if (itsPrs) expect({ bay: again.bay, glyph: again.glyph }).toEqual({ bay: left.bay, glyph: left.glyph });
          else expect(said(again)).toEqual(said(left));
        });
      }
    });
  }
});
