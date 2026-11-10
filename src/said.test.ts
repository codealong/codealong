import { describe, expect, it } from 'vitest';
import { CUE_SHOWS, hearingNamed, hearingSays, latestTalk, lingers, saidNow, saidUntil, talkSays } from './said';
import type { BarListening, Talk } from './types';

const talk = (over: Partial<Talk> = {}): Talk => ({ id: 1, phase: 'listening', to: 'looking', text: '', level: 0, ...over });
const listening = (over: Partial<BarListening> = {}): BarListening => ({ key: 'k', laneId: '/w', title: 'Renewal call', id: 'a', startedAt: 0, role: 'coach', mic: true, playback: true, transcript: '', notes: '', cueAt: 0, ...over });

const NOW = 1_000_000;

describe('what the bar shows of voice', () => {
  it('shows a key held to talk for as long as it’s held, and a moment after', () => {
    expect(saidNow(talk(), NOW - 60_000, [], 0, NOW)).toEqual({ kind: 'talk', talk: talk() });
    const typed = talk({ phase: 'typed', text: 'Rebase it.' });
    expect(saidNow(typed, NOW - 1000, [], 0, NOW)).toEqual({ kind: 'talk', talk: typed });
    expect(saidNow(typed, NOW - 2500, [], 0, NOW)).toBeNull();
    // What went wrong stays long enough to read.
    expect(saidNow(talk({ phase: 'failed' }), NOW - 6000, [], 0, NOW)).not.toBeNull();
    expect(lingers('listening')).toBeNull();
    expect(lingers('settling')).toBeNull();
  });

  it('shows a coach’s newest cue while it’s fresh, and not a note-taker’s', () => {
    const cue = listening({ cue: 'Ask when they renew.', cueAt: NOW - 3000 });
    expect(saidNow(null, 0, [cue], 0, NOW)).toEqual({ kind: 'cue', listening: cue });
    expect(saidNow(null, 0, [listening({ cue: 'Ask when they renew.', cueAt: NOW - CUE_SHOWS })], 0, NOW)).toBeNull();
    expect(saidNow(null, 0, [listening({ role: 'notes', cue: 'Noted the date.', cueAt: NOW - 3000 })], 0, NOW)).toBeNull();
    expect(saidNow(null, 0, [listening()], 0, NOW)).toBeNull();
    const newer = listening({ key: 'other', cue: 'They said Q3, not Q2.', cueAt: NOW - 1000 });
    expect(saidNow(null, 0, [cue, newer], 0, NOW)).toEqual({ kind: 'cue', listening: newer });
  });

  it('puts a cue away when asked, until the next', () => {
    const cue = listening({ cue: 'Ask when they renew.', cueAt: NOW - 3000 });
    expect(saidNow(null, 0, [cue], cue.cueAt, NOW)).toBeNull();
    const next = listening({ cue: 'They agreed.', cueAt: NOW - 500 });
    expect(saidNow(null, 0, [next], cue.cueAt, NOW)).toEqual({ kind: 'cue', listening: next });
  });

  it('shows the key held to talk over a cue, which comes back after it', () => {
    const cue = listening({ cue: 'Ask when they renew.', cueAt: NOW - 3000 });
    expect(saidNow(talk(), NOW, [cue], 0, NOW)?.kind).toBe('talk');
    expect(saidNow(talk({ phase: 'sent' }), NOW - 3000, [cue], 0, NOW)?.kind).toBe('cue');
  });

  it('keeps showing the press under way when an earlier one settles', () => {
    const second = talk({ id: 2, text: 'And then' });
    expect(latestTalk(null, second)).toBe(second);
    expect(latestTalk(second, talk({ id: 1, phase: 'typed', text: 'First.' }))).toBe(second);
    const more = talk({ id: 2, text: 'And then the tests' });
    expect(latestTalk(second, more)).toBe(more);
    const third = talk({ id: 3 });
    expect(latestTalk(second, third)).toBe(third);
  });

  it('says which sessions hear, and what’s wrong with one', () => {
    expect(hearingSays(undefined)).toBeNull();
    expect(hearingSays([])).toBeNull();
    expect(hearingSays([listening()])).toBe('Renewal call is listening');
    expect(hearingSays([listening(), listening({ key: 'b', title: 'Standup', problem: 'Nothing has come from what this Mac plays yet.' })])).toBe('Renewal call is listening\nStandup is listening: Nothing has come from what this Mac plays yet.');
  });

  it('names the one session hearing, and counts several', () => {
    expect(hearingNamed([listening()])).toBe('Renewal call');
    expect(hearingNamed([listening(), listening({ key: 'b', title: 'Standup' })])).toBe('2 sessions listening');
  });

  it('knows when what shows runs out', () => {
    expect(saidUntil(null, 0)).toBeNull();
    expect(saidUntil({ kind: 'talk', talk: talk() }, NOW)).toBeNull();
    expect(saidUntil({ kind: 'talk', talk: talk({ phase: 'typed' }) }, NOW)).toBe(NOW + 2500);
    expect(saidUntil({ kind: 'cue', listening: listening({ cueAt: NOW }) }, 0)).toBe(NOW + CUE_SHOWS);
  });

  it('says who’s listening and where the words went', () => {
    expect(talkSays(talk())).toBe('Listening');
    expect(talkSays(talk({ to: 'orchestrator' }))).toBe('Listening, for your orchestrator');
    expect(talkSays(talk({ phase: 'typed' }))).toBe('Typed');
    expect(talkSays(talk({ phase: 'typed', to: 'orchestrator', title: 'Orchestrator' }))).toBe('In Orchestrator’s box');
    expect(talkSays(talk({ phase: 'sent', to: 'orchestrator', title: 'Orchestrator' }))).toBe('Sent to Orchestrator');
    expect(talkSays(talk({ phase: 'asked' }))).toBe('In the quick panel');
    expect(talkSays(talk({ phase: 'nothing' }))).toBe('Nothing heard');
  });
});
