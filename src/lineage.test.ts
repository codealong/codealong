import { describe, expect, it } from 'vitest';
import { arrowhead, cameLines, cameLineWords, cameOf, cameWord, connector, lineageOf, movedTargets, originWord, pathOf, pointAt, polyline, screenHeadline, sortStarting, startingState } from './lineage';
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
    // Not the headless runs its commands started, which its row counts.
    const run = { session: fx.session({ key: 'r', headless: true, origin: { key: 'a', how: 'started' } }) };
    expect(cameOf('a', [...all, run]).map((x) => x.session.key)).toEqual(['b', 'd']);
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
    // An automatic review of a PR it opened is its helper too, though nothing started it from there.
    const review = { lane, session: fx.session({ key: 'codex:codex:review', helper: { parent: 'claude:claude:boss', pr: 'acme/app#7' } }) };
    homes.set(review.session.key, review);
    expect(lineageOf(boss.session, homes).to.map((k) => `${cameWord(k.how)} ${k.session.key}`)).toContain('Its helper codex:codex:review');
    expect(lineageOf(review.session, homes).from).toBeNull();
    // A helper whose original parent moved points `from` at the moved copy (Helper of), even if
    // the helper itself was forked from an earlier helper session.
    const movedBoss = { lane, session: fx.session({ key: 'claude:claude2:boss-moved', account: 'claude2', origin: { key: 'claude:claude:boss', how: 'fork' } }) };
    const movedHelper = { lane, session: fx.session({ key: 'codex:codex:rev2', origin: { key: 'claude:claude:boss', how: 'started' }, helper: { parent: 'claude:claude2:boss-moved' } }) };
    const forkedHelper = { lane, session: fx.session({ key: 'codex:codex:rev3', origin: { key: 'codex:codex:rev', how: 'fork' }, helper: { parent: 'claude:claude2:boss-moved' } }) };
    homes.set(movedBoss.session.key, movedBoss);
    homes.set(movedHelper.session.key, movedHelper);
    homes.set(forkedHelper.session.key, forkedHelper);
    expect(lineageOf(movedHelper.session, homes).from?.session.key).toBe('claude:claude2:boss-moved');
    expect(originWord(lineageOf(movedHelper.session, homes).from!.how)).toBe('Helper of');
    expect(lineageOf(forkedHelper.session, homes).from?.session.key).toBe('claude:claude2:boss-moved');
    expect(originWord(lineageOf(forkedHelper.session, homes).from!.how)).toBe('Helper of');
    expect(lineageOf(movedBoss.session, homes).to.map((k) => `${cameWord(k.how)} ${k.session.key}`)).toEqual(['Its helper codex:codex:rev2', 'Its helper codex:codex:rev3']);
  });
});

describe('cameLines', () => {
  it('lists what came of a session, its runs together and its side conversations among them, the latest started first', () => {
    const lane = fx.lane();
    const at = (n: number) => fx.NOW - n * 60_000;
    const boss = { lane, session: fx.session({ key: 'boss', title: 'Boss' }) };
    const fork = { lane, session: fx.session({ key: 'fork', title: 'A fork', startedAt: at(30), origin: { key: 'boss', how: 'fork' } }) };
    const run = (key: string, mins: number) => ({ lane, session: fx.session({ key, startedAt: at(mins), headless: true, origin: { key: 'boss', how: 'started' } }) });
    const homes = new Map([boss, fork, run('r1', 50), run('r2', 10)].map((p) => [p.session.key, p]));
    const side = (id: string, mins: number) => ({ id, question: `why ${id}?`, asked: 1, running: false, open: false, startedAt: at(mins), lastAt: at(mins) });
    const lineage = lineageOf(boss.session, homes, { sides: { boss: [side('s1', 5), side('s2', 40)], other: [side('x', 1)] } });
    expect(lineage.to.map((k) => k.session.key)).toEqual(['fork']);
    expect(lineage.runs.map((k) => k.session.key).sort()).toEqual(['r1', 'r2']);
    const lines = cameLines(lineage);
    expect(lines.map(cameLineWords)).toEqual(['On the side: why s1?', '2 headless runs', 'Forked into A fork', 'On the side: why s2?']);
    // The runs go by the latest of them, which leads their line.
    expect(lines[1].kind === 'runs' && lines[1].runs[0].session.key).toBe('r2');
    expect(cameLines(lineageOf(fork.session, homes))).toEqual([]);
  });

  it('lists a start whose session is still on its way among them, by how its prompt opens', () => {
    const lane = fx.lane();
    const at = (n: number) => fx.NOW - n * 60_000;
    const boss = { lane, session: fx.session({ key: 'boss', title: 'Boss' }) };
    const review = { lane, session: fx.session({ key: 'review', title: 'Review the diff', startedAt: at(20), helper: { parent: 'boss' } }) };
    const homes = new Map([boss, review].map((p) => [p.session.key, p]));
    const start = { at: at(4), account: 'codex', harness: 'Codex', lane: lane.id, place: lane.callsign, helper: true, opens: 'Draft the northwind release notes', terminal: 'Rex', stuck: false, screen: null, pid: null, choices: [] };
    const untitled = { ...start, at: at(30), helper: false, opens: null };
    const lineage = lineageOf(boss.session, homes, { starting: { boss: [start, untitled], other: [start] } });
    expect(lineage.starting).toEqual([start, untitled]);
    // The latest started first: the one the header shows while the rest are folded away.
    expect(cameLines(lineage).map(cameLineWords)).toEqual(['Its helper, starting: Draft the northwind release notes', 'Its helper Review the diff', 'Started, starting: Codex on codex']);
    expect(lineageOf(review.session, homes, { starting: { boss: [start] } }).starting).toEqual([]);
    // One stuck in its terminal waits on you, so it comes first however long ago it started.
    const stuck = { ...untitled, helper: true, opens: 'Check the atlas migration', stuck: true, screen: '  \n  Hooks need review\n› 1. Review hooks', pid: 4242, choices: [{ number: 1, words: 'Review hooks' }] };
    const waiting = lineageOf(boss.session, homes, { starting: { boss: [start, stuck] } });
    expect(cameLines(waiting).map(cameLineWords)).toEqual(['Its helper, starting: Check the atlas migration', 'Its helper, starting: Draft the northwind release notes', 'Its helper Review the diff']);
    expect(screenHeadline(stuck.screen!)).toBe('Hooks need review');
    expect(startingState(stuck)).toBe('“Hooks need review”');
    expect(startingState({ ...stuck, screen: null })).toBe('Waiting in Rex');
    expect(startingState(start)).toBe('Starting');
    expect(sortStarting([start, untitled, stuck]).map((x) => x.at)).toEqual([stuck.at, start.at, untitled.at]);
  });
});

describe('movedTargets', () => {
  it('maps moved sessions to their latest destination copy and ignores same-account forks', () => {
    const lane = fx.lane();
    const at = (n: number) => fx.NOW - n * 60_000;
    const a = { lane, session: fx.session({ key: 'claude:claude:a', account: 'claude', archived: true, startedAt: at(30) }) };
    const b = { lane, session: fx.session({ key: 'claude:claude2:b', account: 'claude2', archived: true, startedAt: at(20), origin: { key: 'claude:claude:a', how: 'fork' } }) };
    const c = { lane, session: fx.session({ key: 'claude:claude3:c', account: 'claude3', startedAt: at(10), origin: { key: 'claude:claude2:b', how: 'fork' } }) };
    const sameFork = { lane, session: fx.session({ key: 'claude:claude:f', account: 'claude', startedAt: at(15), origin: { key: 'claude:claude:a', how: 'fork' } }) };
    const started = { lane, session: fx.session({ key: 'claude:claude2:s', account: 'claude2', startedAt: at(5), origin: { key: 'claude:claude:a', how: 'started' } }) };
    const homes = new Map([a, b, c, sameFork, started].map((p) => [p.session.key, p]));
    const targets = movedTargets(homes);
    // Follows the chain A -> B -> C to the latest copy.
    expect(targets.get('claude:claude:a')?.session.key).toBe('claude:claude3:c');
    expect(targets.get('claude:claude2:b')?.session.key).toBe('claude:claude3:c');
    expect(targets.has('claude:claude3:c')).toBe(false);
    expect(targets.has('claude:claude:f')).toBe(false);
    expect(targets.has('claude:claude2:s')).toBe(false);
  });

  it('picks the latest move when moved more than once and guards against cycles', () => {
    const lane = fx.lane();
    const at = (n: number) => fx.NOW - n * 60_000;
    const src = { lane, session: fx.session({ key: 'claude:claude:src', account: 'claude', startedAt: at(30) }) };
    const older = { lane, session: fx.session({ key: 'claude:claude2:old', account: 'claude2', startedAt: at(20), origin: { key: 'claude:claude:src', how: 'fork' } }) };
    const newer = { lane, session: fx.session({ key: 'claude:claude3:new', account: 'claude3', startedAt: at(5), origin: { key: 'claude:claude:src', how: 'fork' } }) };
    const x = { lane, session: fx.session({ key: 'claude:claude:x', account: 'claude', startedAt: at(10), origin: { key: 'claude:claude2:y', how: 'fork' } }) };
    const y = { lane, session: fx.session({ key: 'claude:claude2:y', account: 'claude2', startedAt: at(10), origin: { key: 'claude:claude:x', how: 'fork' } }) };
    const targets = movedTargets(new Map([src, older, newer, x, y].map((p) => [p.session.key, p])));
    expect(targets.get('claude:claude:src')?.session.key).toBe('claude:claude3:new');
    expect(targets.get('claude:claude:x')?.session.key).toBe('claude:claude2:y');
    expect(targets.get('claude:claude2:y')?.session.key).toBe('claude:claude:x');
  });

  it('prefers the active or newer copy when moved copies share the source transcript start time', () => {
    const lane = fx.lane();
    const at = (n: number) => fx.NOW - n * 60_000;
    const src = { lane, session: fx.session({ key: 'claude:claude:src', account: 'claude', archived: true, startedAt: at(60), lastActivity: at(50) }) };
    const dead = { lane, session: fx.session({ key: 'claude:claude2:dead', account: 'claude2', live: false, startedAt: at(60), lastActivity: at(50), origin: { key: 'claude:claude:src', how: 'fork' } }) };
    const active = { lane, session: fx.session({ key: 'claude:claude3:active', account: 'claude3', live: true, startedAt: at(60), lastActivity: at(5), origin: { key: 'claude:claude:src', how: 'fork' } }) };
    // Even when the dead copy appears first in homes, the live copy wins.
    const targets = movedTargets(new Map([src, dead, active].map((p) => [p.session.key, p])));
    expect(targets.get('claude:claude:src')?.session.key).toBe('claude:claude3:active');
  });
});
