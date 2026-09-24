import { describe, expect, it } from 'vitest';
import { dragWidth } from './Resizer';

describe('dragWidth', () => {
  it('follows the pointer', () => {
    expect(dragWidth(264, 40, 200, 300)).toBe(304);
    expect(dragWidth(264, -40, 200, 300)).toBe(224);
  });

  it('stops at the panel minimum', () => {
    expect(dragWidth(264, -200, 200, 300)).toBe(200);
  });

  it('takes no more than the board can spare', () => {
    expect(dragWidth(340, 500, 300, 120)).toBe(460);
  });

  it('only narrows when the board is already at its minimum or under it', () => {
    expect(dragWidth(340, 50, 300, 0)).toBe(340);
    expect(dragWidth(340, 50, 300, -20)).toBe(340);
    expect(dragWidth(340, -30, 300, -20)).toBe(310);
  });

  it('rounds to whole pixels', () => {
    expect(dragWidth(264.4, 10.3, 200, 300)).toBe(275);
  });
});
