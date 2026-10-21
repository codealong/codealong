import { describe, expect, it } from 'vitest';
import { reorderKey, VIEW_CHORDS, viewOfKey } from './mainKeys';
import { CHORDS, chordKeys } from './menus';
import { setPlatform } from './platform';
import { QUEUE_CHORDS } from './queue';

const VIEWS = [{ id: 'sessions' }, { id: 'prs' }, { id: 'todo' }, { id: 'canvas' }] as const;
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
    expect(VIEWS.map((v) => chordKeys(VIEW_CHORDS[v.id]).join(''))).toEqual(['⇧⌘S', '⇧⌘P', '⇧⌘T', '⇧⌘B']);
    expect(['s', 'p', 't', 'b'].map((l) => viewOfKey(press(l), VIEWS))).toEqual(VIEWS.map((v) => v.id));
    // ⇧⌘W went with the Worktrees view, which Sessions grouped by worktree took the place of.
    expect(viewOfKey(press('w'), VIEWS)).toBeNull();
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
      expect(VIEWS.map((v) => chordKeys(VIEW_CHORDS[v.id]).join('+'))).toEqual(['Ctrl+Shift+S', 'Ctrl+Shift+P', 'Ctrl+Shift+T', 'Ctrl+Shift+B']);
      expect(['s', 'p', 't', 'b'].map((l) => viewOfKey(press(l, { ctrl: true, shift: true }), VIEWS))).toEqual(VIEWS.map((v) => v.id));
      expect(viewOfKey(press('s'), VIEWS)).toBeNull();
    } finally {
      setPlatform(null);
    }
  });
});

describe('moving a row', () => {
  const arrow = (key: string, mods: { ctrl?: boolean; cmd?: boolean; alt?: boolean; shift?: boolean }) =>
    ({ key, code: key, ctrlKey: !!mods.ctrl, metaKey: !!mods.cmd, altKey: !!mods.alt, shiftKey: !!mods.shift }) as KeyboardEvent;

  it('is ⌃⌘↑ and ⌃⌘↓ on macOS, and nothing else held', () => {
    setPlatform('mac');
    try {
      expect(reorderKey(arrow('ArrowUp', { ctrl: true, cmd: true }))).toBe(-1);
      expect(reorderKey(arrow('ArrowDown', { ctrl: true, cmd: true }))).toBe(1);
      expect(reorderKey(arrow('ArrowUp', { ctrl: true, cmd: true, shift: true }))).toBe(0);
      expect(reorderKey(arrow('ArrowUp', { cmd: true }))).toBe(0);
    } finally {
      setPlatform(null);
    }
  });

  it('is Ctrl+Shift+PageUp and PageDown on Linux, where Ctrl+Alt and an arrow are the desktop’s', () => {
    setPlatform('linux');
    try {
      expect(reorderKey(arrow('PageUp', { ctrl: true, shift: true }))).toBe(-1);
      expect(reorderKey(arrow('PageDown', { ctrl: true, shift: true }))).toBe(1);
      expect(reorderKey(arrow('ArrowUp', { ctrl: true, alt: true }))).toBe(0);
      expect(reorderKey(arrow('PageUp', { ctrl: true }))).toBe(0);
    } finally {
      setPlatform(null);
    }
  });
});
