import { describe, expect, it } from 'vitest';
import { tabStop } from './focusTrap';

describe('tabStop', () => {
  const stops = ['close', 'name', 'cancel', 'rename'];

  it('goes round from the last stop to the first, and back', () => {
    expect(tabStop(stops, 'rename', false)).toBe('close');
    expect(tabStop(stops, 'close', true)).toBe('rename');
  });

  it('leaves a move between two stops inside to the browser', () => {
    expect(tabStop(stops, 'name', false)).toBeNull();
    expect(tabStop(stops, 'name', true)).toBeNull();
    expect(tabStop(stops, 'close', false)).toBeNull();
    expect(tabStop(stops, 'rename', true)).toBeNull();
  });

  it('comes in at an end from the dialog itself or the window behind it', () => {
    expect(tabStop(stops, 'the board', false)).toBe('close');
    expect(tabStop(stops, 'the board', true)).toBe('rename');
    expect(tabStop(stops, null, false)).toBe('close');
  });

  it('stays on the one stop there is, and has nowhere to go with none', () => {
    expect(tabStop(['done'], 'done', false)).toBe('done');
    expect(tabStop(['done'], 'done', true)).toBe('done');
    expect(tabStop([], 'done', false)).toBeNull();
  });
});
