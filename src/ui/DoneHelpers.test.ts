import { describe, expect, it } from 'vitest';
import { session } from '../testing/fixtures';
import { doneHelpersWords, standsWords } from './DoneHelpers';

const NOW = 1_800_000_000_000;

describe('Mark as done’s question about helpers', () => {
  it('says whose helpers they are, how many, and what each choice does', () => {
    const boss = session({ title: 'Ship the release' });
    const h = session({ key: 'codex:codex:h', title: 'Review the diff' });
    expect(doneHelpersWords({ sessions: [boss], helpers: [h] })).toEqual({
      title: 'Mark “Ship the release” as done?',
      lede: '“Ship the release” has a helper whose work isn’t over. Make it your own to go on with it in your lists, or end it too: a turn under way stops, and a question waiting on you goes unanswered.',
    });
    expect(doneHelpersWords({ sessions: [boss, session({ title: 'Other' })], helpers: [h, h] })).toEqual({
      title: 'Mark 2 sessions as done?',
      lede: 'The 2 sessions you picked have 2 helpers whose work isn’t over. Make them your own to go on with them in your lists, or end them too: a turn under way stops, and a question waiting on you goes unanswered.',
    });
  });

  it('says where each stands, as its session’s helpers chip does, or what it left running', () => {
    const at = (over: Parameters<typeof session>[0]) => standsWords(session({ statusSince: NOW - 3 * 60_000, ...over }), NOW);
    expect(at({ status: 'working' })).toBe('working for 3m');
    expect(at({ status: 'needs' })).toMatch(/^waits on you, /);
    expect(at({ status: 'idle', background: 'npm test' })).toBe('left a command running in the background');
    expect(at({ status: 'idle', background: undefined, agents: ['Explore the repo'] })).toBe('left agents running in the background');
  });
});
