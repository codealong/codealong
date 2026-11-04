// What a window does for one terminal it shows (TerminalView.tsx): measures the cell in the
// `--type-terminal` font, sizes the grid to the room it has and tells the server, says every few
// seconds that it shows the terminal (terminals.rs `watch`), keeps the screen from the frames that
// come (grid.ts) and draws it (render.ts) once a display frame, and sends what's typed and done
// with the mouse in order, one call at a time with whatever came meanwhile, so a fast typist's
// keys reach the program as typed however long a call takes.

import { onTerminalFrame, openUrl, terminalCopy, terminalInput, terminalPaste, terminalResize, terminalText, terminalTheme, terminalWatch } from '../../api';
import { rememberSize, themeOf } from '../../terminals';
import type { TerminalInput, TerminalTheme } from '../../types';
import { cmd } from '../../platform';
import { apply, emptyGrid, linkAt, type GridState } from './grid';
import { modsOf, routeKey } from './keys';
import { makeRenderer, type Font, type Renderer } from './render';
import { buildScene, type Metrics } from './scene';

/** How often a window says again that it shows a terminal: well within the server's 20 s. */
const WATCH_EVERY = 8000;
/** How long the cursor shows, and hides, as it blinks. */
const BLINK_MS = 530;

export interface Hooks {
  /** The keyboard goes back to where it came from (⌃`). */
  onLeave: () => void;
  /** Pasted text that could run commands as it's pasted, held for a yes. */
  onUnsafePaste: (text: string) => void;
  /** Something went wrong in the background: said once, quietly. */
  onError: (why: string) => void;
  /** The pointer is over a link (⌘ held), or isn't. */
  onPointer: (cursor: string) => void;
}

/** What waits to go to the server, in order: inputs that go together, or a paste. */
type Sending = { kind: 'inputs'; inputs: TerminalInput[] } | { kind: 'paste'; text: string; force: boolean } | { kind: 'copy'; all: boolean };

interface Measured extends Metrics {
  font: Font;
  dpr: number;
}

function measure(el: HTMLElement): Measured {
  const cs = getComputedStyle(el);
  const dpr = window.devicePixelRatio || 1;
  const sizeCss = parseFloat(cs.fontSize) || 12;
  const lineCss = parseFloat(cs.lineHeight) || Math.round(sizeCss * 1.35);
  const font: Font = { family: cs.fontFamily, size: Math.round(sizeCss * dpr * 100) / 100 };
  const ctx = document.createElement('canvas').getContext('2d');
  let cellW = Math.round(sizeCss * 0.6 * dpr);
  let ascent = font.size * 0.8;
  let descent = font.size * 0.2;
  if (ctx) {
    ctx.font = `400 ${font.size}px ${font.family}`;
    const m = ctx.measureText('M');
    cellW = Math.max(1, Math.round(m.width));
    ascent = m.fontBoundingBoxAscent || m.actualBoundingBoxAscent || ascent;
    descent = m.fontBoundingBoxDescent || m.actualBoundingBoxDescent || descent;
  }
  const cellH = Math.max(1, Math.round(lineCss * dpr));
  const baseline = Math.round((cellH - (ascent + descent)) / 2 + ascent);
  return { font, dpr, cellW, cellH, baseline, line: Math.max(1, Math.round(dpr)) };
}

export class TerminalController {
  private grid: GridState = emptyGrid();
  private renderer: Renderer | null = null;
  private m: Measured | null = null;
  private cols = 0;
  private rows = 0;
  private queue: Sending[] = [];
  private sending = false;
  private drawPending = false;
  private focused = false;
  private blinkOn = true;
  private timers: number[] = [];
  private cleanups: (() => void)[] = [];
  private theme: TerminalTheme | null = null;
  private wheelRest = 0;
  private pressed: { x: number; y: number; link: string | null } | null = null;
  private stopped = false;

  constructor(
    readonly id: string,
    private readonly wrap: HTMLElement,
    private readonly canvas: HTMLCanvasElement,
    private readonly hooks: Hooks,
  ) {}

  start(): void {
    try {
      this.renderer = makeRenderer(this.canvas);
      // Which drew it, for whoever looks at the page (a browser check, the web inspector).
      this.canvas.dataset.renderer = this.renderer.kind;
    } catch (e) {
      this.hooks.onError(e instanceof Error ? e.message : String(e));
      return;
    }
    this.cleanups.push(
      onTerminalFrame((f) => {
        if (f.id !== this.id) return;
        const done = apply(this.grid, f);
        // A frame missed (the window was busy): the whole screen again.
        if (done === 'missed') this.watch();
        else this.redraw();
      }),
    );
    const observer = new ResizeObserver(() => this.fit());
    observer.observe(this.wrap);
    this.cleanups.push(() => observer.disconnect());
    // The font may still be loading: measure once it's in.
    void document.fonts?.ready.then(() => {
      if (!this.stopped) this.fit(true);
    });
    const scheme = window.matchMedia('(prefers-color-scheme: dark)');
    const recolor = () => this.sendTheme();
    scheme.addEventListener('change', recolor);
    const themeAttr = new MutationObserver(recolor);
    themeAttr.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme', 'class', 'style'] });
    this.cleanups.push(() => scheme.removeEventListener('change', recolor), () => themeAttr.disconnect());
    this.fit(true);
    this.sendTheme();
    this.watch();
    this.timers.push(window.setInterval(() => this.watch(), WATCH_EVERY));
    this.timers.push(
      window.setInterval(() => {
        if (!this.focused || !this.grid.cursor?.blink) return;
        this.blinkOn = !this.blinkOn;
        this.redraw();
      }, BLINK_MS),
    );
  }

  stop(): void {
    this.stopped = true;
    for (const c of this.cleanups) c();
    for (const t of this.timers) window.clearInterval(t);
    this.renderer?.dispose();
    void terminalWatch(this.id, false).catch(() => {
      // Closed already, or the server went: nothing is sent either way.
    });
  }

  private watch() {
    void terminalWatch(this.id, true).catch((e: unknown) => this.hooks.onError(String(e)));
  }

  private sendTheme() {
    const theme = themeOf();
    if (JSON.stringify(theme) === JSON.stringify(this.theme)) return;
    this.theme = theme;
    void terminalTheme(this.id, theme).catch((e: unknown) => this.hooks.onError(String(e)));
  }

  /** Sizes the grid to the room the terminal has, and tells the server when it changed. */
  fit(remeasure = false): void {
    if (!this.renderer) return;
    if (remeasure || !this.m || this.m.dpr !== (window.devicePixelRatio || 1)) this.m = measure(this.wrap);
    const m = this.m;
    const rect = this.wrap.getBoundingClientRect();
    const cols = Math.max(2, Math.floor((rect.width * m.dpr) / m.cellW));
    const rows = Math.max(1, Math.floor((rect.height * m.dpr) / m.cellH));
    const width = cols * m.cellW;
    const height = rows * m.cellH;
    // A canvas given a size, even the one it has, is wiped: only a new size is given, and it's
    // drawn again at once, before the page is painted, rather than on the next frame, so a
    // resize never shows it blank in between.
    if (width !== this.canvas.width || height !== this.canvas.height || remeasure) {
      this.canvas.style.width = `${width / m.dpr}px`;
      this.canvas.style.height = `${height / m.dpr}px`;
      this.renderer.resize(width, height, m.cellW, m.cellH, m.baseline, m.font);
      this.drawNow();
    }
    if (cols !== this.cols || rows !== this.rows || remeasure) {
      this.cols = cols;
      this.rows = rows;
      const size = { cols, rows, cellWidth: m.cellW, cellHeight: m.cellH };
      rememberSize(size);
      void terminalResize(this.id, size).catch((e: unknown) => this.hooks.onError(String(e)));
    }
  }

  private redraw() {
    if (this.drawPending) return;
    this.drawPending = true;
    requestAnimationFrame(() => {
      this.drawPending = false;
      this.drawNow();
    });
  }

  private drawNow() {
    if (!this.renderer || !this.m || this.stopped) return;
    const colors = { selection: hexVar('--term-selection', 0xcddcf7), cursor: this.theme?.cursor ?? this.grid.fg };
    this.renderer.draw(buildScene(this.grid, this.m, colors, this.focused, this.blinkOn));
    // The terminal's own background behind and around the canvas: the part of a cell the grid
    // leaves at its edges, and the canvas itself if it's ever shown before it's drawn.
    // Only once a frame has said what it is; till then the theme's.
    if (this.grid.seq === 0) return;
    const bg = `#${this.grid.bg.toString(16).padStart(6, '0')}`;
    if (this.wrap.style.getPropertyValue('--term-shown-bg') !== bg) this.wrap.style.setProperty('--term-shown-bg', bg);
  }

  send(input: TerminalInput): void {
    const last = this.queue[this.queue.length - 1];
    if (last?.kind === 'inputs') last.inputs.push(input);
    else this.queue.push({ kind: 'inputs', inputs: [input] });
    this.flush();
  }

  /** Sends what's waiting, in the order it came: a run of inputs in one call, a paste in one of its
   *  own, never two calls at once. */
  private flush() {
    if (this.sending) return;
    const next = this.queue.shift();
    if (!next) return;
    this.sending = true;
    const sent =
      next.kind === 'inputs'
        ? terminalInput(this.id, next.inputs)
        : next.kind === 'paste'
          ? terminalPaste(this.id, next.text, next.force).then((pasted) => {
              if (!pasted) this.hooks.onUnsafePaste(next.text);
            })
          : (next.all ? terminalText(this.id) : terminalCopy(this.id)).then((text) => (text ? navigator.clipboard.writeText(text) : undefined));
    sent
      .catch((e: unknown) => this.hooks.onError(String(e)))
      .finally(() => {
        this.sending = false;
        this.flush();
      });
  }

  focus(on: boolean): void {
    this.focused = on;
    this.blinkOn = true;
    this.send({ kind: 'focus', on });
    this.redraw();
  }

  /** A key pressed or let go in the terminal; true when it was taken (and its default kept from
   *  happening). */
  key(e: KeyboardEvent, down: boolean): boolean {
    const route = routeKey(e, down);
    switch (route.to) {
      case 'terminal':
        // A key let go is only for programs that ask for releases; the server drops the rest.
        this.blinkOn = true;
        this.send(route.input);
        return true;
      case 'copy':
        if (down) this.copy();
        return true;
      case 'selectAll':
        if (down) this.send({ kind: 'select', all: true });
        return true;
      case 'paste':
        // The paste event that follows has the clipboard (`paste`).
        return false;
      case 'compose':
        return false;
      case 'app':
        if (down && e.key === '`' && e.ctrlKey) {
          this.hooks.onLeave();
          return true;
        }
        return false;
    }
  }

  /** Copies the selection, as it is once what came before (a click, Select all) has reached the
   *  terminal. */
  copy(): void {
    this.queue.push({ kind: 'copy', all: false });
    this.flush();
  }

  /** Copies everything the terminal holds, scrollback included. */
  copyAll(): void {
    this.queue.push({ kind: 'copy', all: true });
    this.flush();
  }

  selectAll(): void {
    this.send({ kind: 'select', all: true });
  }

  /** A right-click goes to a program that takes the mouse, unless Shift is held. */
  takesRightClick(e: MouseEvent): boolean {
    return this.grid.mouse && !e.shiftKey;
  }

  /** Text pasted, with a yes to text that could run commands as it's pasted. */
  /** Text pasted, in its turn among what's typed: text that could run commands as it's pasted
   *  comes back to `onUnsafePaste` unpasted, and goes with `force` once you say so. */
  paste(text: string, force = false): void {
    if (!text) return;
    this.queue.push({ kind: 'paste', text, force });
    this.flush();
  }

  /** Text an input method or a dead key typed. */
  typed(text: string): void {
    if (text) this.send({ kind: 'text', text });
  }

  private at(e: MouseEvent): { x: number; y: number } {
    const r = this.canvas.getBoundingClientRect();
    const dpr = this.m?.dpr ?? 1;
    return { x: (e.clientX - r.left) * dpr, y: (e.clientY - r.top) * dpr };
  }

  private linkUnder(p: { x: number; y: number }): string | null {
    if (!this.m) return null;
    const col = Math.floor(p.x / this.m.cellW);
    const row = Math.floor(p.y / this.m.cellH);
    return linkAt(this.grid.lines[row] ?? [], this.grid.cols, col);
  }

  pointer(e: PointerEvent, action: 'down' | 'up' | 'move'): void {
    const p = this.at(e);
    const button = e.button === 0 ? 'left' : e.button === 1 ? 'middle' : e.button === 2 ? 'right' : undefined;
    // ⌘ and a click opens a link, as in Ghostty; the program doesn't get the click.
    const link = cmd(e) ? this.linkUnder(p) : null;
    if (action === 'move') this.hooks.onPointer(link ? 'pointer' : this.grid.mouse && !e.shiftKey ? 'default' : 'text');
    if (action === 'down') {
      this.pressed = { ...p, link };
      if (link) return;
    }
    if (action === 'up' && this.pressed?.link) {
      const { link: opened, x, y } = this.pressed;
      this.pressed = null;
      // A press that turned into a drag isn't a click on the link.
      if (Math.hypot(p.x - x, p.y - y) < 6) void openUrl(opened);
      return;
    }
    if (action === 'up') this.pressed = null;
    // Moves only matter while a button is held, or to a program that follows the pointer.
    if (action === 'move' && !e.buttons && !this.grid.mouse) return;
    if (button === 'right' && action !== 'move' && !(this.grid.mouse && !e.shiftKey)) return;
    this.send({ kind: 'mouse', action, button: action === 'move' ? undefined : button, x: p.x, y: p.y, mods: modsOf(e) });
  }

  wheel(e: WheelEvent): void {
    if (!this.m) return;
    const rowCss = this.m.cellH / this.m.dpr;
    const rows = e.deltaMode === 1 ? e.deltaY : e.deltaMode === 2 ? e.deltaY * this.rows : e.deltaY / rowCss;
    this.wheelRest += rows;
    const whole = Math.trunc(this.wheelRest);
    if (!whole) return;
    this.wheelRest -= whole;
    const p = this.at(e);
    this.send({ kind: 'wheel', rows: whole, x: p.x, y: p.y, mods: modsOf(e) });
  }

  /** The view's place in the scrollback, for a scroll bar: rows above, shown, in all. */
  scroll(): GridState['scroll'] {
    return this.grid.scroll;
  }
}

function hexVar(name: string, fallback: number): number {
  const v = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  const m = /^#([0-9a-f]{6})$/i.exec(v);
  return m ? parseInt(m[1], 16) : fallback;
}
