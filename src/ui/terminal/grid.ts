// A terminal's screen as a window keeps it, from the frames the server sends (terminals.rs,
// screen.rs `Frame`): each row's runs, with the looks they name copied in (a frame's table of
// looks is its own), the cursor, and which rows changed since they were last drawn. The server
// holds the terminal; this only keeps what to draw, so a window that missed a frame asks for the
// whole screen again (`apply` says so) rather than drawing on a guess.

import type { TerminalCaret, TerminalFrame, TerminalLook } from '../../types';

/** A look's flags (screen.rs). */
export const BOLD = 1;
export const ITALIC = 1 << 1;
export const FAINT = 1 << 2;
export const BLINK = 1 << 3;
export const STRIKE = 1 << 4;
export const OVERLINE = 1 << 5;
export const SELECTED = 1 << 6;
export const LINK = 1 << 7;
/** The underline's kind, in three bits from here: 1 single, 2 double, 3 curly, 4 dotted, 5 dashed. */
export const UNDERLINE_SHIFT = 8;

export function underlineOf(look: TerminalLook): number {
  return ((look.a ?? 0) >> UNDERLINE_SHIFT) & 7;
}

/** Cells side by side drawn alike: one entry a cell, " " blank, "" the second half of a wide one. */
export interface GridRun {
  x: number;
  look: TerminalLook;
  cells: string[];
}

export interface GridState {
  cols: number;
  rows: number;
  /** The last frame's, to tell a missed one. */
  seq: number;
  fg: number;
  bg: number;
  cursor: TerminalCaret | null;
  scroll: TerminalFrame['scroll'];
  mouse: boolean;
  /** Each row's runs, from the top of the view. */
  lines: GridRun[][];
}

export function emptyGrid(): GridState {
  return { cols: 0, rows: 0, seq: 0, fg: 0, bg: 0xffffff, cursor: null, scroll: { total: 0, offset: 0, len: 0 }, mouse: false, lines: [] };
}

/** What a frame came to: the rows to draw again, every one when `all`; or `missed`, a frame that
 *  doesn't follow the last, when the window should ask for the whole screen. */
export type Applied = { all: boolean; rows: number[]; cursorMoved: boolean } | 'missed';

/** Applies a frame to the screen, in place. Before the first whole frame nothing is drawn. */
export function apply(grid: GridState, frame: TerminalFrame): Applied {
  if (!frame.full && (grid.seq === 0 || frame.seq !== grid.seq + 1)) return 'missed';
  const resized = frame.cols !== grid.cols || frame.rows !== grid.rows;
  const recolored = frame.fg !== grid.fg || frame.bg !== grid.bg;
  const before = grid.cursor;
  grid.seq = frame.seq;
  grid.cols = frame.cols;
  grid.rows = frame.rows;
  grid.fg = frame.fg;
  grid.bg = frame.bg;
  grid.cursor = frame.cursor;
  grid.scroll = frame.scroll;
  grid.mouse = frame.mouse;
  if (frame.full || grid.lines.length !== frame.rows) {
    grid.lines = Array.from({ length: frame.rows }, (_, y) => grid.lines[y] ?? []);
  }
  const rows: number[] = [];
  for (const line of frame.lines) {
    if (line.y < 0 || line.y >= frame.rows) continue;
    grid.lines[line.y] = line.runs.map((r) => ({ x: r.x, look: frame.looks[r.l] ?? {}, cells: r.c }));
    rows.push(line.y);
  }
  const cursorMoved = before?.x !== frame.cursor?.x || before?.y !== frame.cursor?.y || before?.shape !== frame.cursor?.shape;
  return { all: frame.full || resized || recolored, rows, cursorMoved };
}

/** A row's text, a cell to a character (the first of a cell's), with where each cell starts in
 *  it: for finding a link under the pointer. */
export function rowText(runs: GridRun[], cols: number): { text: string; cellAt: number[] } {
  const cells: string[] = Array.from({ length: cols }, () => ' ');
  for (const run of runs) {
    run.cells.forEach((c, i) => {
      const x = run.x + i;
      if (x < cols) cells[x] = c === '' ? '' : c;
    });
  }
  let text = '';
  const cellAt: number[] = [];
  cells.forEach((c, x) => {
    cellAt[x] = text.length;
    text += c;
  });
  return { text, cellAt };
}

const URL = /\b(?:https?|file):\/\/[^\s<>"'`]+[^\s<>"'`.,;:!?)\]}]/g;

/** The web address shown at column `col` of a row, if any. */
export function linkAt(runs: GridRun[], cols: number, col: number): string | null {
  const { text, cellAt } = rowText(runs, cols);
  const at = cellAt[col];
  if (at === undefined) return null;
  for (const m of text.matchAll(URL)) {
    const start = m.index ?? 0;
    if (at >= start && at < start + m[0].length) return m[0];
  }
  return null;
}
