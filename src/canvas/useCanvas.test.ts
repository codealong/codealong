import { beforeEach, describe, expect, it, vi } from 'vitest';
import { emptyDoc, type CanvasDoc } from './model';

// The file: what the last save wrote, read by canvasLoad.
const file = new Map<string, CanvasDoc>();
let onRead: (() => void) | null = null;

vi.mock('../api', () => ({
  canvasSave: vi.fn((project: string, doc: CanvasDoc) => {
    file.set(project, doc);
    return Promise.resolve();
  }),
  canvasLoad: vi.fn((project: string) => {
    const doc = file.get(project) ?? null;
    onRead?.();
    return Promise.resolve(doc);
  }),
}));

const { readCanvas, saveCanvas } = await import('./useCanvas');

const noted = (text: string): CanvasDoc => ({ ...emptyDoc(), items: [{ kind: 'note', id: text, text, x: 0, y: 0, w: 220, h: 140, color: 'gray' }] });

describe('readCanvas', () => {
  beforeEach(() => {
    file.clear();
    onRead = null;
  });

  it('reads the file once the window’s saves of it are written', async () => {
    void saveCanvas('Acme', noted('one'));
    expect(await readCanvas('Acme')).toEqual(noted('one'));
    expect(await readCanvas('Elsewhere')).toBeNull();
  });

  it('reads again when the window saves the project while it reads', async () => {
    let saves = 0;
    onRead = () => {
      if (saves++ === 0) void saveCanvas('Acme', noted('newer'));
    };
    await saveCanvas('Acme', noted('older'));
    expect(await readCanvas('Acme')).toEqual(noted('newer'));
  });

  it('fails rather than give a read a save overtook every time', async () => {
    let n = 0;
    onRead = () => void saveCanvas('Acme', noted(`note ${++n}`));
    await expect(readCanvas('Acme')).rejects.toThrow('The canvas kept changing while it was read');
  });
});
