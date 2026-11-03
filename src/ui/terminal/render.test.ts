import { beforeAll, describe, expect, it } from 'vitest';
import type { SceneGlyph } from './scene';

// A 2D context that draws nothing, for the atlas's canvas outside a browser.
beforeAll(() => {
  const ctx = new Proxy({}, { get: (_, name) => (name === 'measureText' ? () => ({ width: 10 }) : () => {}), set: () => true });
  (globalThis as { document?: unknown }).document = { createElement: () => ({ width: 0, height: 0, getContext: () => ctx }) };
});

const glyph = (text: string): SceneGlyph => ({ text, x: 0, y: 0, span: 1, color: 0, alpha: 1, bold: false, italic: false });

describe('the glyph atlas', () => {
  it('says it’s full before it starts again, so nothing placed before is drawn from a moved slot', async () => {
    const { Atlas } = await import('./render');
    // Cells this big fill a 2048-pixel atlas with a few dozen glyphs.
    const atlas = new Atlas(200, 400, 300, { family: 'monospace', size: 300 });
    const drawn: string[][] = [];
    let placed: { text: string; x: number; y: number }[] = [];
    atlas.onFull = () => {
      drawn.push(placed.map((p) => p.text));
      placed = [];
    };
    const many = Array.from({ length: 120 }, (_, i) => String.fromCharCode(0x4e00 + i));
    for (const text of many) {
      const s = atlas.get(glyph(text));
      // No two glyphs waiting to be drawn share a slot.
      expect(placed.some((p) => p.x === s.x && p.y === s.y), text).toBe(false);
      placed.push({ text, x: s.x, y: s.y });
    }
    drawn.push(placed.map((p) => p.text));
    expect(drawn.length).toBeGreaterThan(1);
    expect(drawn.flat()).toEqual(many);
  });
});
