import { describe, expect, it } from 'vitest';
import { VIEW_CHORDS, viewOfKey } from './mainKeys';
import { CHORDS, chordKeys } from './menus';
import { setPlatform } from './platform';
import { QUEUE_CHORDS } from './queue';

const VIEWS = [{ id: 'sessions' }, { id: 'worktrees' }, { id: 'prs' }, { id: 'todo' }, { id: 'canvas' }] as const;
// A key as macOS reports it with ⇧ held: the capital letter.
const press = (letter: string, mods: { ctrl?: boolean; cmd?: boolean; alt?: boolean; shift?: boolean } = { cmd: true, shift: true }) => ({
  key: mods.shift ? letter.toUpperCase() : letter.toLowerCase(),
  code: `Key${letter.toUpperCase()}`,
  ctrlKey: !!mods.ctrl,
  metaKey: !!mods.cmd,
  altKey: !!mods.alt,
  shiftKey: !!mods.shift,
});

describe('view chords', () => {
  it('go to each view by a letter of its name, ⇧⌘ with it', () => {
    expect(VIEWS.map((v) => chordKeys(VIEW_CHORDS[v.id]).join(''))).toEqual(['⇧⌘S', '⇧⌘W', '⇧⌘P', '⇧⌘T', '⇧⌘B']);
    expect(['s', 'w', 'p', 't', 'b'].map((l) => viewOfKey(press(l), VIEWS))).toEqual(VIEWS.map((v) => v.id));
  });

  it('need exactly ⇧⌘: the same letter with ⌘, ⌃⌘ or ⌥⇧⌘ is another key', () => {
    expect(viewOfKey(press('s', { cmd: true }), VIEWS)).toBeNull();
    expect(viewOfKey(press('p', { cmd: true, ctrl: true }), VIEWS)).toBeNull();
    expect(viewOfKey(press('w', { cmd: true, shift: true, alt: true }), VIEWS)).toBeNull();
    expect(viewOfKey(press('t', { shift: true }), VIEWS)).toBeNull();
  });

  it('leave ⇧⌘C to clipboard managers and the other ⇧⌘ keys to what has them', () => {
    expect(viewOfKey(press('c'), VIEWS)).toBeNull();
    const taken = [CHORDS.done, 'Shift+Cmd+F', 'Shift+Cmd+N', ...Object.values(QUEUE_CHORDS).flat()];
    expect(Object.values(VIEW_CHORDS).filter((c) => taken.includes(c))).toEqual([]);
  });

  it('are Ctrl+Shift and the letter on Linux, where ⌘ is Ctrl', () => {
    setPlatform('linux');
    try {
      expect(VIEWS.map((v) => chordKeys(VIEW_CHORDS[v.id]).join('+'))).toEqual(['Ctrl+Shift+S', 'Ctrl+Shift+W', 'Ctrl+Shift+P', 'Ctrl+Shift+T', 'Ctrl+Shift+B']);
      expect(['s', 'w', 'p', 't', 'b'].map((l) => viewOfKey(press(l, { ctrl: true, shift: true }), VIEWS))).toEqual(VIEWS.map((v) => v.id));
      expect(viewOfKey(press('s'), VIEWS)).toBeNull();
    } finally {
      setPlatform(null);
    }
  });
});
