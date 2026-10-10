import { describe, expect, it } from 'vitest';
import { viewChord, viewOfKey } from './mainKeys';
import { chordKeys } from './menus';

const VIEWS = [{ id: 'sessions' }, { id: 'worktrees' }, { id: 'prs' }, { id: 'todo' }, { id: 'canvas' }] as const;
const press = (code: string, mods: { ctrl?: boolean; cmd?: boolean; alt?: boolean; shift?: boolean } = { ctrl: true }) => ({ code, ctrlKey: !!mods.ctrl, metaKey: !!mods.cmd, altKey: !!mods.alt, shiftKey: !!mods.shift });

describe('view chords', () => {
  it('go to each view by its place, ⌃1 the first', () => {
    expect(VIEWS.map((_, i) => chordKeys(viewChord(i)))).toEqual([
      ['⌃', '1'],
      ['⌃', '2'],
      ['⌃', '3'],
      ['⌃', '4'],
      ['⌃', '5'],
    ]);
    expect(VIEWS.map((_, i) => viewOfKey(press(`Digit${i + 1}`), VIEWS))).toEqual(VIEWS.map((v) => v.id));
  });

  it('leave the digits ⌘, ⌥⌘, ⌃⌘ and ⇧ already have alone', () => {
    expect(viewOfKey(press('Digit1', { cmd: true }), VIEWS)).toBeNull();
    expect(viewOfKey(press('Digit1', { cmd: true, alt: true }), VIEWS)).toBeNull();
    expect(viewOfKey(press('Digit1', { cmd: true, ctrl: true }), VIEWS)).toBeNull();
    expect(viewOfKey(press('Digit1', { ctrl: true, shift: true }), VIEWS)).toBeNull();
    expect(viewOfKey(press('Digit1', { ctrl: true, alt: true }), VIEWS)).toBeNull();
  });

  it('do nothing past the last view or on ⌃0', () => {
    expect(viewOfKey(press('Digit6'), VIEWS)).toBeNull();
    expect(viewOfKey(press('Digit0'), VIEWS)).toBeNull();
    expect(viewOfKey(press('KeyS'), VIEWS)).toBeNull();
  });
});
