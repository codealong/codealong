import { afterEach, describe, expect, it, vi } from 'vitest';
import type { Font, Renderer } from './render';
import type { TerminalInput } from '../../types';

/** Every call the controller makes, in the order the server would take them, each held until
 *  the test lets it go. */
const sent: string[] = [];
const held: (() => void)[] = [];
const hold = (what: string, answer: unknown = undefined) =>
  new Promise((resolve) => {
    sent.push(what);
    held.push(() => resolve(answer));
  });

vi.mock('../../api', async (original) => ({
  ...(await original<typeof import('../../api')>()),
  terminalInput: vi.fn((_id: string, inputs: TerminalInput[]) => hold(inputs.map((i) => (i.kind === 'key' ? i.text : i.kind)).join(''))),
  terminalPaste: vi.fn((_id: string, text: string, force: boolean) => hold(`paste:${text}${force ? '!' : ''}`, !/\n/.test(text) || force)),
  terminalCopy: vi.fn(() => hold('copy', null)),
  terminalResize: vi.fn((_id: string, size: { cols: number; rows: number }) => {
    resizes.push(`${size.cols}x${size.rows}`);
    return Promise.resolve();
  }),
}));
const resizes: string[] = [];

const { TerminalController } = await import('./controller');

const key = (text: string): TerminalInput => ({ kind: 'key', code: 'KeyA', text, mods: {} });

/** Lets the call in flight answer, and waits for the next to go. */
async function answer() {
  held.shift()?.();
  await new Promise((r) => setTimeout(r, 0));
}

describe('what a window sends a terminal', () => {
  it('goes in the order it came, a paste among the keys, one call at a time', async () => {
    const unsafe: string[] = [];
    const c = new TerminalController('t', {} as HTMLElement, {} as HTMLCanvasElement, { onLeave: () => {}, onUnsafePaste: (t) => unsafe.push(t), onError: () => {}, onPointer: () => {} });
    c.send(key('a'));
    // While the first call is out, more keys and a paste wait their turn.
    c.send(key('b'));
    c.paste('p');
    c.send(key('c'));
    c.send(key('d'));
    expect(sent).toEqual(['a']);
    await answer();
    expect(sent).toEqual(['a', 'b']);
    await answer();
    expect(sent).toEqual(['a', 'b', 'paste:p']);
    await answer();
    expect(sent, 'the keys after it together, after it').toEqual(['a', 'b', 'paste:p', 'cd']);
    await answer();
    // A paste that could run commands comes back to be asked about, and goes once allowed.
    c.paste('rm x\nls');
    await answer();
    expect(unsafe).toEqual(['rm x\nls']);
    c.paste('rm x\nls', true);
    await answer();
    expect(sent[sent.length - 1]).toBe('paste:rm x\nls!');
  });

  it('copies the selection only once what came before it has reached the terminal', async () => {
    sent.length = 0;
    const c = new TerminalController('t', {} as HTMLElement, {} as HTMLCanvasElement, { onLeave: () => {}, onUnsafePaste: () => {}, onError: () => {}, onPointer: () => {} });
    c.send(key('x'));
    c.selectAll();
    c.copy();
    expect(sent).toEqual(['x']);
    await answer();
    expect(sent).toEqual(['x', 'select']);
    await answer();
    expect(sent).toEqual(['x', 'select', 'copy']);
    await answer();
  });
});

describe('a terminal fitted to its room', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('takes a new cell and font when a zoom changes the pixel ratio, though its canvas comes out as many pixels', () => {
    // No 2D context here, so a cell is measured from the font's size: 7 by 16 pixels at 1, 8 by
    // 18 at 1.1, where a room of 560 by 288 pixels holds 80 by 18 cells, then 70 by 16.
    let dpr = 1;
    vi.stubGlobal('window', { get devicePixelRatio() { return dpr; } });
    vi.stubGlobal('getComputedStyle', () => ({ fontSize: '12px', lineHeight: '16px', fontFamily: 'monospace', getPropertyValue: () => '' }));
    vi.stubGlobal('document', { documentElement: {}, createElement: () => ({ getContext: () => null }), hasFocus: () => true });
    const wrap = { getBoundingClientRect: () => ({ width: 560 / dpr, height: 288 / dpr }) } as unknown as HTMLElement;
    const canvas = { width: 0, height: 0, style: {} } as unknown as HTMLCanvasElement;
    const given: string[] = [];
    const renderer: Renderer = {
      kind: 'canvas2d',
      resize(width: number, height: number, cellW: number, cellH: number, _baseline: number, font: Font) {
        canvas.width = width;
        canvas.height = height;
        given.push(`${width}x${height} cell ${cellW}x${cellH} font ${font.size}`);
      },
      draw: () => {},
      dispose: () => {},
    };
    const c = new TerminalController('t', wrap, canvas, { onLeave: () => {}, onUnsafePaste: () => {}, onError: () => {}, onPointer: () => {} });
    (c as unknown as { renderer: Renderer }).renderer = renderer;
    resizes.length = 0;
    c.fit(true);
    c.fit();
    expect(given, 'a fit to the same room gives the canvas no size, which would wipe it').toEqual(['560x288 cell 7x16 font 12']);
    dpr = 1.1;
    c.fit();
    expect(given).toEqual(['560x288 cell 7x16 font 12', '560x288 cell 8x18 font 13.2']);
    expect(resizes).toEqual(['80x18', '70x16']);
  });

  /** A controller with a renderer that notes what it's given, in a room `own` CSS pixels wide
   *  and tall drawn `zoom` times bigger, as a card's CSS zoom draws it. */
  function fitted(own: { width: number; height: number }, zoom: number, focused: () => boolean) {
    vi.stubGlobal('window', { devicePixelRatio: 1 });
    vi.stubGlobal('getComputedStyle', () => ({ fontSize: '12px', lineHeight: '16px', fontFamily: 'monospace', getPropertyValue: () => '' }));
    vi.stubGlobal('document', { documentElement: {}, createElement: () => ({ getContext: () => null }), hasFocus: focused });
    const wrap = { offsetWidth: own.width, getBoundingClientRect: () => ({ width: own.width * zoom, height: own.height * zoom }) } as unknown as HTMLElement;
    const canvas = { width: 0, height: 0, style: {} as Record<string, string> } as unknown as HTMLCanvasElement;
    const given: string[] = [];
    const renderer: Renderer = {
      kind: 'canvas2d',
      resize(width: number, height: number, cellW: number, cellH: number, _baseline: number, font: Font) {
        canvas.width = width;
        canvas.height = height;
        given.push(`${width}x${height} cell ${cellW}x${cellH} font ${font.size}`);
      },
      draw: () => {},
      dispose: () => {},
    };
    const c = new TerminalController('t', wrap, canvas, { onLeave: () => {}, onUnsafePaste: () => {}, onError: () => {}, onPointer: () => {} });
    (c as unknown as { renderer: Renderer }).renderer = renderer;
    return { c, canvas, given };
  }

  it('in a card drawn at a zoom, measures the cell at the card’s scale and fills what it has on the screen', () => {
    // At 125% the font is 15 pixels and the cell 9 by 20, so 560 by 288 on the screen holds 62
    // by 14 cells; the canvas, in the card's own pixels, comes out that size on the screen.
    const { c, canvas, given } = fitted({ width: 448, height: 230.4 }, 1.25, () => true);
    resizes.length = 0;
    c.fit(true);
    expect(given).toEqual(['558x280 cell 9x20 font 15']);
    expect(canvas.style.width, 'drawn 1.25 times bigger, 558 pixels on the screen').toBe('446.4px');
    expect(resizes).toEqual(['62x14']);
  });

  it('gives its size only from the window you’re in, and again as you come back to it', () => {
    let focused = false;
    const { c } = fitted({ width: 560, height: 288 }, 1, () => focused);
    resizes.length = 0;
    c.fit(true);
    expect(resizes, 'another window that shows it keeps its size meanwhile').toEqual([]);
    focused = true;
    c.give();
    expect(resizes).toEqual(['80x18']);
    c.give();
    expect(resizes, 'and each time you come back, as another may have given its own').toEqual(['80x18', '80x18']);
  });
});
