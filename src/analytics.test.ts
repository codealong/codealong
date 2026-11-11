import { describe, expect, it } from 'vitest';
import { BEGAN, HANDS, STANCES, cameIn, countOf, countTicks, markLines, partByProject, percent, responses, secondHandCost, secondHandSessions, segments, stanceStack, stoodStack, yours } from './analytics';
import type { Outcomes, Part, PartRow, Spent } from './types';

const spent = (key: string, api: number): Spent => ({ key, api, paid: 0, tokens: { input: 0, cacheRead: 0, cacheWrite: 0, output: 0, reasoning: 0 }, calls: 1, unpriced: 0, lastAt: 0 });
const marks = { mid: 0, afterStop: 0, pickedFirst: 0, pickedOffered: 0, ownWords: 0, allowed: 0, denied: 0, suggested: 0, chars: 0 };
const part = (over: Partial<Part>): Part => ({ days: [], sources: [], hands: [], stances: [], marks, accounts: [], folders: [], sessions: [], prompts: [], unread: 0, reading: false, ...over });

describe('the series', () => {
  it('stack in their own order, whatever each one has', () => {
    const stack = stanceStack({ day: '2026-11-10', sources: {}, stances: { '': 9, redirects: 1, agrees: 4 } });
    expect(stack.map((s) => [s.label, s.n])).toEqual([
      ['Agreed', 4],
      ['Redirected', 1],
      ['Not read', 9],
    ]);
    // A color follows the thing, not its place in the stack.
    expect(stack[1].color).toBe(STANCES[3].color);
  });

  it('leave nothing counted out: a word they do not name goes last under itself', () => {
    expect(segments(HANDS, { you: 2, robot: 1 }).map((s) => s.label)).toEqual(['You', 'robot']);
  });

  it('name every beginning the engine notes', () => {
    expect(BEGAN.map((b) => b.id)).toEqual(['you', 'fork', 'helper', 'agent', 'review', 'schedule', 'headless', '']);
  });
});

describe('percent', () => {
  it('says a share as a whole percent, and never rounds something to nothing', () => {
    expect(percent(1, 4)).toBe('25%');
    expect(percent(1, 400)).toBe('<1%');
    expect(percent(0, 400)).toBe('0%');
    expect(percent(3, 0)).toBe('—');
  });
});

describe('a count axis', () => {
  it('marks whole numbers past the most, however few there are', () => {
    expect(countTicks(1)).toEqual([0, 1]);
    expect(countTicks(7)).toEqual([0, 5, 10]);
    expect(countTicks(8)).toEqual([0, 5, 10]);
    expect(countTicks(90)).toEqual([0, 50, 100]);
    expect(countTicks(240)).toEqual([0, 100, 200, 300]);
  });
});

describe('what your prompts did', () => {
  it('counts only the ones that answered a reply as agreeing, steering or redirecting', () => {
    const stances = [
      { key: 'sets-out', n: 10 },
      { key: 'agrees', n: 6 },
      { key: 'steers', n: 3 },
      { key: 'redirects', n: 1 },
      { key: 'asks', n: 5 },
      { key: '', n: 40 },
    ];
    expect(responses(stances)).toEqual({ agreed: 6, steered: 3, redirected: 1, all: 10 });
    expect(countOf(stances, 'command')).toBe(0);
  });

  it('says what the records are sure of, and nothing of what never happened', () => {
    expect(markLines(part({}))).toEqual([]);
    const lines = markLines(part({ marks: { ...marks, mid: 1, afterStop: 2, pickedFirst: 3, pickedOffered: 1, ownWords: 0, allowed: 12, denied: 1, suggested: 1 } }));
    expect(lines).toEqual([
      '1 was sent while the agent worked',
      '2 came after you stopped its turn',
      '4 answers to its questions: 3 its first choice, 1 another, 0 in your own words',
      '12 calls allowed and 1 denied from Codealong',
      '1 suggested reply sent as written',
    ]);
  });
});

describe('second hand', () => {
  it('is what the turns others began cost, of what is known', () => {
    expect(secondHandCost([spent('you', 60), spent('agent', 30), spent('codealong', 10), spent('', 25)])).toEqual({ second: 40, known: 100 });
    expect(secondHandCost([spent('', 25)])).toBeNull();
    expect(secondHandCost(undefined)).toBeNull();
  });

  it('is the sessions agents and Codealong began; a copy you made is yours', () => {
    const began = [
      { key: 'you', n: 10 },
      { key: 'fork', n: 2 },
      { key: 'helper', n: 5 },
      { key: 'review', n: 3 },
      { key: '', n: 7 },
    ];
    expect(secondHandSessions(began)).toEqual({ second: 8, known: 20 });
  });
});

describe('rows', () => {
  const row = (key: string, you: number, agent: number, stances: Record<string, number>, lastAt = 0): PartRow => ({ key, counts: { you, agent }, stances, lastAt });

  it('count a row’s prompts as the ones you typed, not every turn that counts as yours', () => {
    // One prompt, and four reports of a task it started, which count with it.
    expect(yours(row('claude:claude:a', 5, 0, { agrees: 1 }))).toBe(1);
    expect(cameIn(row('claude:claude:a', 5, 2, { agrees: 1 }))).toBe(7);
  });

  it('sum folders by project, the most prompted first', () => {
    const folders = [row('/work/app', 2, 1, { agrees: 2 }, 5), row('/work/app-wt', 3, 0, { steers: 1, agrees: 2 }, 9), row('/work/site', 12, 4, { '': 9 }, 1), row('/tmp/x', 1, 0, { asks: 1 })];
    const rows = partByProject(folders, (f) => (f.startsWith('/work/app') ? 'App' : f.startsWith('/work/site') ? 'Site' : ''));
    expect(rows.map((r) => [r.key, yours(r), cameIn(r)])).toEqual([
      ['Site', 9, 16],
      ['App', 5, 6],
      ['No project', 1, 1],
    ]);
    expect(rows[1].stances).toEqual({ agrees: 4, steers: 1 });
    expect(rows[1].lastAt).toBe(9);
  });
});

describe('how you left sessions', () => {
  it('stacks them finished first, the ones with nothing to show left out', () => {
    const o = { finished: 8, lookedFinished: 0, unfinished: 3, unknown: 1 } as Outcomes;
    expect(stoodStack(o).map((s) => [s.label, s.n])).toEqual([
      ['Finished', 8],
      ['Unfinished', 3],
      ['Not known', 1],
    ]);
  });
});
