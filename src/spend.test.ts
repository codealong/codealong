import { describe, expect, it } from 'vitest';
import { byProject, cacheShare, contextFigure, contextLevel, contextShare, contextWords, cumulative, leverage, money, moneyTick, seriesColor, sessionLabel, stackOf, ticks, tokenCount } from './spend';
import * as fx from './testing/fixtures';
import type { Spent, SpendDay } from './types';

function spent(key: string, api: number, over: Partial<Spent> = {}): Spent {
  return { key, api, paid: 0, tokens: { input: 1, cacheRead: 2, cacheWrite: 3, output: 4, reasoning: 0 }, calls: 1, unpriced: 0, lastAt: 0, ...over };
}

describe('money and tokens in words', () => {
  it('says cents under a thousand dollars and whole dollars above', () => {
    expect(money(412.8)).toBe('$412.80');
    expect(money(15090.87)).toBe('$15,091');
    expect(money(0)).toBe('$0');
    expect(money(0.001)).toBe('<$0.01');
    expect(moneyTick(1500)).toBe('$1.5K');
    expect(moneyTick(250)).toBe('$250');
  });

  it('says token counts in a few characters', () => {
    expect(tokenCount(980)).toBe('980');
    expect(tokenCount(12_900)).toBe('12.9K');
    expect(tokenCount(38_200_000)).toBe('38.2M');
    expect(tokenCount(1_411_986_972)).toBe('1.4B');
    expect(tokenCount(250_000_000)).toBe('250M');
  });

  it('says how much of what went in came from the cache', () => {
    expect(cacheShare({ input: 10, cacheRead: 90, cacheWrite: 0, output: 5, reasoning: 0 })).toBe(0.9);
    expect(cacheShare({ input: 0, cacheRead: 0, cacheWrite: 0, output: 0, reasoning: 0 })).toBe(0);
  });

  it('says what the work was worth for each dollar paid, and nothing while nothing was paid', () => {
    expect(leverage(412.8, 212)).toBe('1.9×');
    expect(leverage(15090, 400)).toBe('38×');
    expect(leverage(100, 0)).toBeNull();
  });
});

describe('the chart', () => {
  it('runs each day on from the one before', () => {
    const days: SpendDay[] = [
      { day: '2026-10-11', api: { claude: 10, codex: 5 }, paid: 6, tokens: 0 },
      { day: '2026-10-12', api: { claude: 20 }, paid: 6, tokens: 0 },
    ];
    expect(cumulative(days)).toEqual([
      { day: '2026-10-11', api: 15, paid: 6 },
      { day: '2026-10-12', api: 35, paid: 12 },
    ]);
  });

  it('steps its axis in round numbers past the top', () => {
    expect(ticks(900)).toEqual([0, 250, 500, 750, 1000]);
    expect(ticks(14_000)).toEqual([0, 5000, 10000, 15000]);
    expect(ticks(0)).toEqual([0, 1]);
  });

  it('colors an account by its place in config.toml, whatever the report shows', () => {
    const order = ['claude', 'claude2', 'codex'];
    expect(seriesColor('codex', order)).toBe('var(--chart-3)');
    expect(seriesColor('somewhere', order)).toBe('var(--line-strong)');
    const day = { day: '2026-10-12', api: { codex: 3, claude: 1, gone: 2 }, paid: 0, tokens: 0 };
    expect(stackOf(day, order)).toEqual([
      { account: 'claude', api: 1 },
      { account: 'codex', api: 3 },
      { account: 'Other', api: 2 },
    ]);
  });
});

describe('the context meter', () => {
  it('says the share of the window, or the tokens where the window isn’t known', () => {
    expect(contextFigure({ used: 820_093, window: 1_000_000 })).toBe('82%');
    expect(contextFigure({ used: 117_721 })).toBe('118K');
    expect(contextWords({ used: 189_236, window: 258_400 })).toBe('189K of 258K tokens');
    expect(contextShare({ used: 300_000, window: 200_000 })).toBe(1);
  });

  it('looks full past 80% and all but full past 95%', () => {
    expect(contextLevel(0.5)).toBe('ok');
    expect(contextLevel(0.82)).toBe('high');
    expect(contextLevel(0.97)).toBe('full');
    expect(contextLevel(null)).toBe('ok');
  });
});

describe('rows', () => {
  it('gathers folders into the board projects that hold them', () => {
    const lanes = [fx.lane({ id: 'a', project: 'Codealong', path: '/p/codealong' }), fx.lane({ id: 'b', project: 'Codealong', path: '/p/codealong/.claude/worktrees/x' })];
    const rows = byProject([spent('/p/codealong/.claude/worktrees/x', 5), spent('/p/codealong', 2), spent('/elsewhere/tool', 9)], lanes);
    expect(rows.map((r) => [r.key, r.api])).toEqual([
      ['elsewhere/tool', 9],
      ['Codealong', 7],
    ]);
    expect(rows[1].tokens.output).toBe(8);
  });

  it('names a session by its title on the board, else by its folder', () => {
    const s = fx.session({ key: 'claude:claude:abc', title: 'Fix the bar' });
    const on = new Map([[s.key, s]]);
    expect(sessionLabel(spent('claude:claude:abc', 1), on)).toBe('Fix the bar');
    expect(sessionLabel(spent('claude:claude:gone', 1, { folder: '/p/app' }), on)).toBe('p/app');
  });
});
