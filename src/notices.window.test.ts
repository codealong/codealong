import { describe, expect, it, vi } from 'vitest';
import type { WindowNotice } from './api';
import type { Notice } from './types';

// What goes between windows, caught here: what this window hands on, and a way to hand it some.
const sent: WindowNotice[] = [];
let handTo: ((m: WindowNotice) => void) | null = null;
vi.mock('./api', () => ({
  emitNotice: (m: WindowNotice) => sent.push(m),
  onWindowNotice: (cb: (m: WindowNotice) => void) => {
    handTo = cb;
    return () => (handTo = null);
  },
}));

const { clearNotice, drawNoticesHere, reportedNotices, reportNotice } = await import('./notices');

const save: Notice = { key: 'canvas', title: 'The canvas isn’t saving', text: 'Couldn’t save the canvas: disk full', until: null };

describe('notices a window reports', () => {
  it('hands them on from a window that doesn’t draw them, once each, and their ends only for those', () => {
    reportNotice(save);
    reportNotice(save);
    reportNotice({ ...save, text: 'Couldn’t save the canvas: no such folder' });
    clearNotice('canvas');
    clearNotice('canvas');
    clearNotice('github');
    expect(sent).toEqual([{ notice: save }, { notice: { ...save, text: 'Couldn’t save the canvas: no such folder' } }, { clear: 'canvas' }]);
  });

  it('keeps them, and those handed on, in a window that draws them', () => {
    sent.length = 0;
    const stop = drawNoticesHere();
    reportNotice(save);
    const stale: Notice = { key: 'conversation:k', title: 'A conversation isn’t updating', text: 'x', until: null };
    handTo?.({ notice: stale });
    handTo?.({ notice: stale });
    expect(reportedNotices()).toEqual([save, stale]);
    handTo?.({ clear: 'conversation:k' });
    expect(reportedNotices()).toEqual([save]);
    clearNotice('canvas');
    expect(reportedNotices()).toEqual([]);
    expect(sent).toEqual([]);
    stop();
    expect(handTo).toBeNull();
  });
});
