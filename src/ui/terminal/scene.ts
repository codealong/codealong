// What a terminal's screen comes to on the canvas, in device pixels: the rectangles behind cells,
// each cell's text with its color and style, the lines that decorate them (underlines, strikes),
// and the cursor. The WebGL and Canvas 2D renderers (render.ts) draw this alike; working it out
// apart from either keeps what's drawn testable without a canvas.

import type { TerminalLook } from '../../types';
import { BOLD, FAINT, ITALIC, OVERLINE, SELECTED, STRIKE, underlineOf, type GridState } from './grid';

export interface Metrics {
  /** A cell, in device pixels. */
  cellW: number;
  cellH: number;
  /** The text's baseline from a cell's top, in device pixels. */
  baseline: number;
  /** How thick a line is: an underline, a bar cursor. */
  line: number;
}

/** The colors drawn where the screen sets none, as 0xRRGGBB. */
export interface Colors {
  selection: number;
  cursor: number;
}

export interface SceneRect {
  x: number;
  y: number;
  w: number;
  h: number;
  color: number;
  /** 0 to 1. */
  alpha: number;
  /** An outline `line` thick rather than filled (a hollow cursor). */
  outline?: boolean;
}

export interface SceneGlyph {
  text: string;
  x: number;
  y: number;
  /** How many cells it takes: 2 for a wide character. */
  span: number;
  color: number;
  alpha: number;
  bold: boolean;
  italic: boolean;
}

export interface Scene {
  bg: number;
  rects: SceneRect[];
  glyphs: SceneGlyph[];
  /** Over the text: decorations, then the cursor and the glyph it covers, drawn in reverse. */
  over: SceneRect[];
  overGlyphs: SceneGlyph[];
}

/** What's drawn: the screen, and whether the window has the keyboard (an unfocused cursor is
 *  hollow) and the cursor's blink is on. */
export function buildScene(grid: GridState, m: Metrics, colors: Colors, focused: boolean, blinkOn: boolean): Scene {
  const scene: Scene = { bg: grid.bg, rects: [], glyphs: [], over: [], overGlyphs: [] };
  grid.lines.forEach((runs, row) => {
    const y = row * m.cellH;
    for (const run of runs) {
      const look = run.look;
      const flags = look.a ?? 0;
      const selected = (flags & SELECTED) !== 0;
      const bg = selected ? colors.selection : look.b;
      const fg = look.f ?? grid.fg;
      const width = run.cells.length * m.cellW;
      const x0 = run.x * m.cellW;
      if (bg !== undefined) scene.rects.push({ x: x0, y, w: width, h: m.cellH, color: bg, alpha: 1 });
      run.cells.forEach((text, i) => {
        if (text === '' || text === ' ') return;
        const span = run.cells[i + 1] === '' ? 2 : 1;
        scene.glyphs.push({ text, x: x0 + i * m.cellW, y, span, color: fg, alpha: flags & FAINT ? 0.5 : 1, bold: (flags & BOLD) !== 0, italic: (flags & ITALIC) !== 0 });
      });
      decorate(scene.over, look, x0, y, width, m, fg);
    }
  });
  const c = grid.cursor;
  if (c && c.y < grid.rows && c.x < Math.max(grid.cols, 1)) {
    const x = c.x * m.cellW;
    const y = c.y * m.cellH;
    const color = c.color ?? colors.cursor;
    const shape = focused ? c.shape : 'hollow';
    const shown = !focused || !c.blink || blinkOn;
    if (shown) {
      if (shape === 'block') {
        scene.over.push({ x, y, w: m.cellW, h: m.cellH, color, alpha: 1 });
        // The character under a block cursor, in the background's color, so it still reads.
        const under = glyphAt(grid, c.x, c.y);
        if (under) scene.overGlyphs.push({ ...under, x, y, color: grid.bg, alpha: 1 });
      } else if (shape === 'bar') {
        scene.over.push({ x, y, w: m.line * 2, h: m.cellH, color, alpha: 1 });
      } else if (shape === 'underline') {
        scene.over.push({ x, y: y + m.cellH - m.line * 2, w: m.cellW, h: m.line * 2, color, alpha: 1 });
      } else {
        scene.over.push({ x, y, w: m.cellW, h: m.cellH, color, alpha: 1, outline: true });
      }
    }
  }
  return scene;
}

function decorate(out: SceneRect[], look: TerminalLook, x: number, y: number, w: number, m: Metrics, fg: number) {
  const flags = look.a ?? 0;
  const kind = underlineOf(look);
  const color = look.u ?? fg;
  const under = y + m.baseline + Math.max(m.line, Math.round(m.cellH / 12));
  if (kind === 1 || kind === 3) out.push({ x, y: under, w, h: m.line, color, alpha: 1 });
  if (kind === 2) {
    out.push({ x, y: under, w, h: m.line, color, alpha: 1 });
    out.push({ x, y: under + m.line * 2, w, h: m.line, color, alpha: 1 });
  }
  if (kind === 4 || kind === 5) {
    // Dotted and dashed: a dot a line wide, a dash a quarter cell, with as much space between.
    const step = kind === 4 ? m.line * 2 : Math.max(m.line * 2, Math.round(m.cellW / 2));
    for (let at = x; at < x + w; at += step * 2) out.push({ x: at, y: under, w: Math.min(step, x + w - at), h: m.line, color, alpha: 1 });
  }
  if (flags & STRIKE) out.push({ x, y: y + Math.round(m.cellH / 2), w, h: m.line, color: fg, alpha: 1 });
  if (flags & OVERLINE) out.push({ x, y, w, h: m.line, color: fg, alpha: 1 });
}

/** The glyph drawn at a cell, if any, for the cursor over it. */
function glyphAt(grid: GridState, col: number, row: number): Omit<SceneGlyph, 'x' | 'y' | 'color' | 'alpha'> | null {
  for (const run of grid.lines[row] ?? []) {
    const i = col - run.x;
    if (i < 0 || i >= run.cells.length) continue;
    const text = run.cells[i];
    if (text === '' || text === ' ') return null;
    const flags = run.look.a ?? 0;
    return { text, span: run.cells[i + 1] === '' ? 2 : 1, bold: (flags & BOLD) !== 0, italic: (flags & ITALIC) !== 0 };
  }
  return null;
}
