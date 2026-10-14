import { describe, expect, it } from 'vitest';
import { afterStop, cardTitle, flagged, flagLabel, readNote, trainingOf, trainingTag } from './training';
import type { Training } from './types';

const reading = (state: Training['state'], account = 'claude'): Training => ({ account, state, note: 'Help improve Claude is on.', canStop: true, url: null, asOf: 1 });

describe('training', () => {
  it('flags only accounts the provider may train on, or where nothing is chosen', () => {
    expect([reading('on'), reading('unset'), reading('off'), reading('unknown')].map(flagged)).toEqual([true, true, false, false]);
    expect(flagged(undefined)).toBe(false);
  });

  it('finds an account by name, none before the first read', () => {
    const list = [reading('on', 'claude'), reading('off', 'codex')];
    expect(trainingOf(list, 'codex')?.state).toBe('off');
    expect(trainingOf(list, 'cursor')).toBeUndefined();
    expect(trainingOf(undefined, 'claude')).toBeUndefined();
  });

  it('says what the flag is for', () => {
    expect(flagLabel(reading('on'))).toBe('Training on');
    expect(flagLabel(reading('unset'))).toBe('Training not set');
    expect(cardTitle(reading('on'))).toBe('Training is on for claude');
    expect(cardTitle(reading('unset'))).toBe("Training isn't turned off for claude");
  });

  it('tags every account Settings can tell about', () => {
    expect(trainingTag(reading('off'))).toEqual({ word: 'Not trained on', ok: true });
    expect(trainingTag(reading('on'))).toEqual({ word: 'Training on', ok: false });
    expect(trainingTag(reading('unknown'))).toBeNull();
  });

  it('says when it was read, and why not since', () => {
    const now = 10 * 60e3;
    expect(readNote({ ...reading('off'), asOf: now - 3 * 60e3 }, now)).toMatch(/^Read /);
    expect(readNote({ ...reading('off'), asOf: now - 3 * 60e3, stale: 'Sign-in expired: run `claude` once.' }, now)).toMatch(/; reading it again failed: Sign-in expired: run claude once\.$/);
    expect(readNote({ ...reading('unknown'), asOf: null }, now)).toBeNull();
  });

  it('says when a turn-off did not take', () => {
    expect(afterStop(reading('off'))).toBeNull();
    expect(afterStop(reading('on'))).toMatch(/^It was asked to turn off, but still reads as on/);
    expect(afterStop({ ...reading('unknown'), note: 'Sign-in expired.' })).toBe("It was asked to turn off, but Codealong couldn't read it back to check: Sign-in expired.");
  });
});
