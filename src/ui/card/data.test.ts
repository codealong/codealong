import { describe, expect, it } from 'vitest';
import * as fx from '../../testing/fixtures';
import { cardSessions, type CardData } from './data';

describe('cardSessions', () => {
  const working = fx.session({ key: 'a', status: 'working', live: true });
  const waiting = fx.session({ key: 'b', status: 'unread', live: false });
  const idle = fx.session({ key: 'c', status: 'idle', live: false });
  const stopped = fx.session({ key: 'd', status: 'stopped', live: false });
  const open = fx.session({ key: 'e', status: 'idle', live: true });
  const archived = fx.session({ key: 'f', status: 'working', live: true, archived: true });
  const visiting = fx.session({ key: 'g', status: 'working', live: true, visitor: 'reads it from another worktree' });
  const lane = fx.lane({ id: 'w', sessions: [working, waiting, idle, stopped, open, archived, visiting] });
  const keys = (d: CardData | null, lanes = [lane]) => cardSessions(d, lanes).map((x) => x.session.key);

  it("lists a worktree's sessions that run or wait on you, its own only", () => {
    expect(keys({ ref: 'worktree', lane })).toEqual(['a', 'b', 'e']);
  });

  it("lists every session of a todo's, as the todo has them", () => {
    const sessions = [{ lane, session: idle }, { lane, session: stopped }];
    expect(cardSessions({ ref: 'todo', todo: fx.todo(), labels: [], sessions, lane }, [lane])).toBe(sessions);
  });

  it("lists a PR's from the worktrees its card was sent", () => {
    const other = fx.lane({ id: 'x', sessions: [fx.session({ key: 'h', status: 'needs', live: true })] });
    const pr = { ref: 'pr' } as CardData;
    expect(keys(pr, [lane, other])).toEqual(['a', 'b', 'e', 'g', 'h']);
    expect(cardSessions(pr, [lane, other])[4].lane).toBe(other);
  });

  it('lists none under a session, or a card the board has lost', () => {
    expect(keys({ ref: 'session', lane, session: working })).toEqual([]);
    expect(keys(null)).toEqual([]);
  });
});
