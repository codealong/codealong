// Codealong's own terminals: one as the board lists it (terminals.rs), and its screen as a window
// draws it with what a window sends it (src-tauri/vt/src/screen.rs).

/** A terminal of Codealong's own as the board lists it (terminals.rs `Glance`). */
export interface TerminalGlance {
  id: string;
  /** The session it's for. */
  session: string;
  /** The title its program set, if any. */
  title: string;
  /** The shell's name (`zsh`), or a command's first word. */
  shell: string;
  /** The command it runs, when it was opened for one rather than for a shell. */
  command?: string;
  /** Where it started, or where its shell last said it is. */
  folder: string;
  startedAt: number;
  /** Its process hasn't ended. */
  running: boolean;
  /** How its process ended: its status, or 128 and the signal that ended it. */
  exit?: number;
  /** The program in front, when it isn't the shell itself. */
  program?: string;
  /** When its program last rang the bell. */
  bellAt?: number;
  /** Its device (`/dev/ttys012`). */
  tty?: string;
  /** The session's own process runs in it (a start or resume in Codealong's terminal): where the
   *  session lives, shown in its panel, not a terminal beside it. */
  hosts?: boolean;
  /** What its program waits on you to type, as it asks it (`Continue? [y/N]`). */
  asks?: string;
  /** Since when it has asked. */
  asksSince?: number;
}

/** A terminal's size: in cells, and a cell's in pixels. */
export interface TerminalSize {
  cols: number;
  rows: number;
  cellWidth: number;
  cellHeight: number;
}

/** A window's colors, for a terminal it shows, as 0xRRGGBB. */
export interface TerminalTheme {
  fg: number;
  bg: number;
  cursor?: number | null;
  /** The 16 ANSI colors. */
  palette: number[];
  dark: boolean;
}

/** How cells are drawn (screen.rs `Look`): colors as 0xRRGGBB, absent for the default; `a` the
 *  flags (terminal/grid.ts says which bit is which). */
export interface TerminalLook {
  f?: number;
  b?: number;
  u?: number;
  a?: number;
}

/** Cells side by side drawn alike, from column `x`: one entry a cell, " " a blank one, "" the
 *  second half of the wide character before it. `l` is the look's index in the frame. */
export interface TerminalRun {
  x: number;
  l: number;
  c: string[];
}

export interface TerminalLine {
  y: number;
  runs: TerminalRun[];
}

export interface TerminalCaret {
  x: number;
  y: number;
  shape: 'block' | 'bar' | 'underline' | 'hollow';
  blink: boolean;
  color?: number;
  password: boolean;
}

/** A terminal's screen, or the rows of it that changed (screen.rs `Frame`). */
export interface TerminalFrame {
  id: string;
  /** Counts up with every frame of the terminal. */
  seq: number;
  cols: number;
  rows: number;
  /** Every row is here. */
  full: boolean;
  lines: TerminalLine[];
  looks: TerminalLook[];
  cursor: TerminalCaret | null;
  fg: number;
  bg: number;
  /** Where the view is in the scrollback, in rows. */
  scroll: { total: number; offset: number; len: number };
  /** The program takes the mouse: clicks and the wheel go to it (Shift selects anyway). */
  mouse: boolean;
}

export interface TerminalMods {
  shift?: boolean;
  ctrl?: boolean;
  alt?: boolean;
  meta?: boolean;
}

/** What a window sends a terminal (screen.rs `Input`). Positions are in pixels from the grid's
 *  top left. */
export type TerminalInput =
  | { kind: 'key'; code: string; text?: string; unshifted?: string; mods: TerminalMods; action?: 'press' | 'repeat' | 'release' }
  | { kind: 'text'; text: string }
  | { kind: 'mouse'; action: 'down' | 'up' | 'move'; button?: 'left' | 'middle' | 'right'; x: number; y: number; mods: TerminalMods }
  | { kind: 'wheel'; rows: number; x: number; y: number; mods: TerminalMods }
  | { kind: 'focus'; on: boolean }
  | { kind: 'scroll'; to: 'top' | 'bottom' | 'pageUp' | 'pageDown' | 'promptUp' | 'promptDown' }
  | { kind: 'select'; all: boolean };
