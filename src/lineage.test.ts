import { describe, expect, it } from 'vitest';
import { arrowhead, cameOf, cameWord, connector, lineageOf, originWord, pathOf, pointAt, polyline } from './lineage';
import * as fx from './testing/fixtures';

const card = (x: number, y: number) => ({ x, y, w: 200, h: 50 });

describe('connector', () => {
  it('goes across between cards side by side, from facing edge to facing edge', () => {
    const c = connector(card(0, 0), card(300, 100));
    expect(c.a).toEqual({ x: 200, y: 25 });
    expect(c.b).toEqual({ x: 300, y: 125 });
    // Leaving and arriving level, pulled halfway across.
    expect(c.c1).toEqual({ x: 250, y: 25 });
    expect(c.c2).toEqual({ x: 250, y: 125 });
    // And the other way round.
    const back = connector(card(300, 100), card(0, 0));
    expect(back.a).toEqual({ x: 300, y: 125 });
    expect(back.b).toEqual({ x: 200, y: 25 });
  });

  it('bows out past one side of cards one over the other, clear of both', () => {
    const c = connector(card(10, 0), card(0, 60));
    expect(c.a).toEqual({ x: 10, y: 25 });
    expect(c.b).toEqual({ x: 0, y: 85 });
    expect(c.c1.x).toBeLessThan(0);
    expect(c.c2.x).toBe(c.c1.x);
    // Its farthest point is left of both cards.
    expect(Math.min(...polyline(c).map((p) => p.x))).toBeLessThan(0);
    const r = connector(card(10, 0), card(0, 60), 'right');
    expect(r.a).toEqual({ x: 210, y: 25 });
    expect(r.b).toEqual({ x: 200, y: 85 });
    expect(r.c1.x).toBeGreaterThan(210);
  });

  it('bows further for cards further apart, up to a limit', () => {
    const near = connector(card(0, 0), card(0, 60));
    const far = connector(card(0, 0), card(0, 400));
    const farthest = connector(card(0, 0), card(0, 5000));
    expect(far.c1.x).toBeLessThan(near.c1.x);
    expect(farthest.c1.x).toBe(-90);
  });
});

describe('the curve', () => {
  const c = connector(card(0, 0), card(300, 100));

  it('starts and ends at its ends', () => {
    expect(pointAt(c, 0)).toEqual(c.a);
    expect(pointAt(c, 1)).toEqual(c.b);
    const pts = polyline(c, 10);
    expect(pts).toHaveLength(11);
    expect(pts[10]).toEqual(c.b);
  });

  it('has its arrowhead pointing where it arrives, behind the tip', () => {
    // Arriving from the left, level: both barbs are left of the tip, one above and one below.
    const [b1, b2] = arrowhead(c, 8, 0.5);
    expect(b1.x).toBeLessThan(c.b.x);
    expect(b2.x).toBeLessThan(c.b.x);
    expect(Math.sign(b1.y - c.b.y)).toBe(-Math.sign(b2.y - c.b.y));
    expect(Math.hypot(b1.x - c.b.x, b1.y - c.b.y)).toBeCloseTo(8);
  });

  it('writes itself as an SVG path', () => {
    expect(pathOf(c)).toBe('M 200.0 25.0 C 250.0 25.0, 250.0 125.0, 300.0 125.0');
  });
});

describe('cameOf', () => {
  it('lists the sessions that came of one', () => {
    const s = (key: string, from?: string) => ({ session: fx.session({ key, origin: from ? { key: from, how: 'fork' } : undefined }) });
    const all = [s('a'), s('b', 'a'), s('c', 'b'), s('d', 'a')];
    expect(cameOf('a', all).map((x) => x.session.key)).toEqual(['b', 'd']);
    expect(cameOf('d', all)).toEqual([]);
  });
});

describe('lineageOf', () => {
  it('tells a fork into another account as a move', () => {
    const lane = fx.lane();
    const placed = (key: string, account: string, from?: string) => ({ lane, session: fx.session({ key, account, origin: from ? { key: from, how: 'fork' } : undefined }) });
    const all = [placed('claude:claude:a', 'claude'), placed('claude:claude2:b', 'claude2', 'claude:claude:a'), placed('claude:claude:c', 'claude', 'claude:claude:a')];
    const homes = new Map(all.map((p) => [p.session.key, p]));
    const moved = lineageOf(all[1].session, homes);
    expect(moved.from?.session.key).toBe('claude:claude:a');
    expect(originWord(moved.from!.how)).toBe('Moved from');
    // The original lists the move apart from a fork in its own account.
    expect(lineageOf(all[0].session, homes).to.map((k) => `${cameWord(k.how)} ${k.session.key}`)).toEqual(['Moved to claude:claude2:b', 'Forked into claude:claude:c']);
  });

  it('tells a helper its agent started as its helper', () => {
    const lane = fx.lane();
    const boss = { lane, session: fx.session({ key: 'claude:claude:boss' }) };
    const helper = { lane, session: fx.session({ key: 'codex:codex:rev', origin: { key: 'claude:claude:boss', how: 'started' }, helper: { parent: 'claude:claude:boss' } }) };
    const started = { lane, session: fx.session({ key: 'codex:codex:other', origin: { key: 'claude:claude:boss', how: 'started' } }) };
    const homes = new Map([boss, helper, started].map((p) => [p.session.key, p]));
    expect(originWord(lineageOf(helper.session, homes).from!.how)).toBe('Helper of');
    expect(lineageOf(boss.session, homes).to.map((k) => `${cameWord(k.how)} ${k.session.key}`)).toEqual(['Its helper codex:codex:rev', 'Started codex:codex:other']);
  });
});
