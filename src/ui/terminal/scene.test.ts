import { describe, expect, it } from 'vitest';
import { BOLD, emptyGrid, FAINT, SELECTED, UNDERLINE_SHIFT, type GridState } from './grid';
import { routeKey, type KeyLike } from './keys';
import { buildScene, type Metrics } from './scene';

const M: Metrics = { cellW: 10, cellH: 20, baseline: 15, line: 1 };
const COLORS = { selection: 0x0000ff, cursor: 0x00ff00 };

function grid(over: Partial<GridState>): GridState {
  return { ...emptyGrid(), cols: 10, rows: 2, fg: 0x111111, bg: 0xeeeeee, ...over };
}

describe('what a terminal screen draws', () => {
  it('draws each cell’s text where it is, a wide one across two cells, and blanks not at all', () => {
    const s = buildScene(grid({ lines: [[{ x: 2, look: {}, cells: ['a', ' ', '字', '', 'b'] }], []] }), M, COLORS, true, true);
    expect(s.glyphs.map((g) => [g.text, g.x, g.span])).toEqual([
      ['a', 20, 1],
      ['字', 40, 2],
      ['b', 60, 1],
    ]);
    expect(s.rects).toEqual([]);
  });

  it('draws a background behind a run that has one, and the selection over any', () => {
    const s = buildScene(grid({ lines: [[{ x: 0, look: { b: 0xff0000 }, cells: ['x'] }, { x: 3, look: { a: SELECTED }, cells: ['y', 'z'] }]] }), M, COLORS, true, true);
    expect(s.rects).toEqual([
      { x: 0, y: 0, w: 10, h: 20, color: 0xff0000, alpha: 1 },
      { x: 30, y: 0, w: 20, h: 20, color: 0x0000ff, alpha: 1 },
    ]);
  });

  it('takes a look’s colors and style, faint text at half strength', () => {
    const s = buildScene(grid({ lines: [[{ x: 0, look: { f: 0xabcdef, a: BOLD | FAINT | (1 << UNDERLINE_SHIFT) }, cells: ['q'] }]] }), M, COLORS, true, true);
    expect(s.glyphs[0]).toMatchObject({ color: 0xabcdef, bold: true, alpha: 0.5 });
    expect(s.over).toHaveLength(1);
    expect(s.over[0]).toMatchObject({ y: 17, w: 10, color: 0xabcdef });
  });

  it('draws a block cursor with the character under it in the background’s color, hollow without the keyboard', () => {
    const g = grid({ lines: [[{ x: 0, look: {}, cells: ['k'] }]], cursor: { x: 0, y: 0, shape: 'block', blink: true, password: false } });
    const focused = buildScene(g, M, COLORS, true, true);
    expect(focused.over).toEqual([{ x: 0, y: 0, w: 10, h: 20, color: 0x00ff00, alpha: 1 }]);
    expect(focused.overGlyphs[0]).toMatchObject({ text: 'k', color: 0xeeeeee });
    expect(buildScene(g, M, COLORS, true, false).over).toEqual([]);
    expect(buildScene(g, M, COLORS, false, false).over[0]).toMatchObject({ outline: true });
  });
});

function key(key: string, code: string, mods: Partial<KeyLike> = {}): KeyLike {
  return { key, code, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, repeat: false, isComposing: false, ...mods };
}

describe('where a key goes in a terminal', () => {
  it('sends the program what it typed, and Ctrl keys by their key alone', () => {
    expect(routeKey(key('a', 'KeyA'), true)).toEqual({ to: 'terminal', input: { kind: 'key', code: 'KeyA', text: 'a', unshifted: 'a', mods: { shift: false, ctrl: false, alt: false, meta: false }, action: 'press' } });
    expect(routeKey(key('c', 'KeyC', { ctrlKey: true }), true)).toMatchObject({ input: { code: 'KeyC', text: undefined, mods: { ctrl: true } } });
    expect(routeKey(key('Enter', 'Enter'), true)).toMatchObject({ input: { code: 'Enter', text: undefined } });
    expect(routeKey(key('å', 'KeyA', { altKey: true }), true)).toMatchObject({ input: { text: 'å', unshifted: undefined, mods: { alt: true } } });
  });

  it('says which letter a key is on your layout, which Ctrl goes by', () => {
    // AZERTY: the key in Q's place on a US keyboard types a.
    expect(routeKey(key('a', 'KeyQ', { ctrlKey: true }), true)).toMatchObject({ input: { code: 'KeyQ', text: undefined, unshifted: 'a' } });
    expect(routeKey(key('A', 'KeyQ', { shiftKey: true }), true)).toMatchObject({ input: { text: 'A', unshifted: 'a' } });
    // A shifted symbol says nothing of the key's own: AZERTY's digits are shifted.
    expect(routeKey(key('1', 'Digit1', { shiftKey: true }), true)).toMatchObject({ input: { unshifted: undefined } });
    // Its release names it as its press did.
    expect(routeKey(key('a', 'KeyQ', { ctrlKey: true }), false)).toMatchObject({ input: { code: 'KeyQ', unshifted: 'a', action: 'release' } });
  });

  it('leaves ⌘ to the app but for copying, pasting and selecting all', () => {
    expect(routeKey(key('w', 'KeyW', { metaKey: true }), true)).toEqual({ to: 'app' });
    expect(routeKey(key('c', 'KeyC', { metaKey: true }), true)).toEqual({ to: 'copy' });
    expect(routeKey(key('v', 'KeyV', { metaKey: true }), true)).toEqual({ to: 'paste' });
    expect(routeKey(key('a', 'KeyA', { metaKey: true }), true)).toEqual({ to: 'selectAll' });
    expect(routeKey(key('`', 'Backquote', { ctrlKey: true }), true)).toEqual({ to: 'app' });
  });

  it('leaves an input method’s keys to it', () => {
    expect(routeKey(key('Process', 'KeyN', { isComposing: true }), true)).toEqual({ to: 'compose' });
    expect(routeKey(key('Dead', 'KeyE', { altKey: true }), true)).toEqual({ to: 'compose' });
  });
});
